use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use agent_client_protocol::{
    JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, UntypedMessage,
    schema::ProtocolVersion, schema::v1 as acp,
};
use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::events;
use crate::output;
use crate::policy::{self, PermissionPolicy};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_PROMPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

type RpcResult = Result<Value, String>;
type PendingRequests = Arc<Mutex<HashMap<i64, oneshot::Sender<RpcResult>>>>;

pub struct AcpClient {
    tx: mpsc::UnboundedSender<Message>,
    pending: PendingRequests,
    next_id: AtomicI64,
    initialized: Mutex<Option<acp::InitializeResponse>>,
    assistant_text: Arc<Mutex<String>>,
}

impl AcpClient {
    pub async fn connect(
        url: &str,
        endpoint: &str,
        permission: PermissionPolicy,
        stream_events: bool,
    ) -> Result<Self> {
        let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(url))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "connect {endpoint} timed out after {}s",
                    CONNECT_TIMEOUT.as_secs()
                )
            })?
            .with_context(|| format!("connect {endpoint}"))?;
        let (mut write, mut read) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));

        let pending_write = pending.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if let Err(err) = write.send(message).await {
                    fail_pending(&pending_write, format!("WebSocket write failed: {err}")).await;
                    return;
                }
            }
            fail_pending(&pending_write, "WebSocket writer stopped").await;
        });

        let pending_read = pending.clone();
        let tx_read = tx.clone();
        let assistant_text = Arc::new(Mutex::new(String::new()));
        let assistant_read = assistant_text.clone();
        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        if let Err(err) = handle_inbound(
                            &pending_read,
                            &tx_read,
                            &assistant_read,
                            text.as_ref(),
                            permission,
                            stream_events,
                        )
                        .await
                        {
                            eprintln!("inbound error: {err:#}");
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("WebSocket read failed: {err}");
                        break;
                    }
                }
            }
            fail_pending(&pending_read, "WebSocket connection closed").await;
        });

        Ok(Self {
            tx,
            pending,
            next_id: AtomicI64::new(1),
            initialized: Mutex::new(None),
            assistant_text,
        })
    }

    pub async fn take_assistant_text(&self) -> String {
        let mut text = self.assistant_text.lock().await;
        std::mem::take(&mut *text)
    }

    pub async fn snapshot_assistant_text(&self) -> String {
        self.assistant_text.lock().await.clone()
    }

    pub async fn initialize(&self) -> Result<acp::InitializeResponse> {
        if let Some(cached) = self.initialized.lock().await.clone() {
            return Ok(cached);
        }
        let response = self
            .request(
                acp::InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(client_capabilities())
                    .client_info(acp::Implementation::new(
                        "qmacp",
                        env!("CARGO_PKG_VERSION"),
                    )),
            )
            .await?;
        *self.initialized.lock().await = Some(response.clone());
        Ok(response)
    }

    pub async fn extension(&self, method: &str, params: Value) -> Result<Value> {
        let wire = if method.starts_with('_') {
            method.to_string()
        } else {
            format!("_{method}")
        };
        self.request(UntypedMessage::new(&wire, params)?).await
    }

    pub async fn new_session(
        &self,
        cwd: impl Into<std::path::PathBuf>,
        profile: Option<&str>,
    ) -> Result<(acp::NewSessionResponse, Value)> {
        let mut request = acp::NewSessionRequest::new(cwd.into());
        if let Some(profile_id) = profile {
            request = request.meta(crate::session::profile_meta(profile_id));
        }
        self.request_json(request).await
    }

    pub async fn list_sessions(
        &self,
        cwd: Option<std::path::PathBuf>,
        cursor: Option<String>,
    ) -> Result<acp::ListSessionsResponse> {
        let mut request = acp::ListSessionsRequest::new();
        if let Some(cwd) = cwd {
            request = request.cwd(cwd);
        }
        if let Some(cursor) = cursor {
            request = request.cursor(cursor);
        }
        self.request(request).await
    }

    pub async fn load_session(
        &self,
        session_id: String,
        cwd: std::path::PathBuf,
    ) -> Result<(acp::LoadSessionResponse, Value)> {
        self.request_json(acp::LoadSessionRequest::new(session_id, cwd))
            .await
    }

    pub async fn resume_session(
        &self,
        session_id: String,
        cwd: std::path::PathBuf,
    ) -> Result<(acp::ResumeSessionResponse, Value)> {
        self.request_json(acp::ResumeSessionRequest::new(session_id, cwd))
            .await
    }

    pub async fn close_session(&self, session_id: String) -> Result<acp::CloseSessionResponse> {
        self.request(acp::CloseSessionRequest::new(session_id))
            .await
    }

    pub async fn set_mode(
        &self,
        session_id: String,
        mode: String,
    ) -> Result<acp::SetSessionModeResponse> {
        self.request(acp::SetSessionModeRequest::new(session_id, mode))
            .await
    }

    pub async fn set_config(
        &self,
        session_id: String,
        config_id: &str,
        value: &str,
    ) -> Result<(acp::SetSessionConfigOptionResponse, Value)> {
        self.request_json(acp::SetSessionConfigOptionRequest::new(
            session_id,
            config_id.to_string(),
            value,
        ))
        .await
    }

    pub async fn prompt(
        &self,
        session_id: String,
        text: String,
        timeout: Option<Duration>,
    ) -> Result<acp::PromptResponse> {
        self.request_with_timeout(
            acp::PromptRequest::new(
                session_id,
                vec![acp::ContentBlock::Text(acp::TextContent::new(text))],
            ),
            timeout.unwrap_or(DEFAULT_PROMPT_TIMEOUT),
        )
        .await
    }

    pub fn cancel(&self, session_id: String) -> Result<()> {
        self.notify(acp::CancelNotification::new(session_id))
    }

    async fn request<R>(&self, request: R) -> Result<R::Response>
    where
        R: JsonRpcRequest + Send + Sync + 'static,
        R::Response: Send + 'static,
    {
        Ok(self.request_with_json(request, REQUEST_TIMEOUT).await?.0)
    }

    pub async fn request_json<R>(&self, request: R) -> Result<(R::Response, Value)>
    where
        R: JsonRpcRequest + Send + Sync + 'static,
        R::Response: Send + 'static,
    {
        self.request_with_json(request, REQUEST_TIMEOUT).await
    }

    async fn request_with_timeout<R>(&self, request: R, timeout: Duration) -> Result<R::Response>
    where
        R: JsonRpcRequest + Send + Sync + 'static,
        R::Response: Send + 'static,
    {
        Ok(self.request_with_json(request, timeout).await?.0)
    }

    async fn request_with_json<R>(
        &self,
        request: R,
        timeout: Duration,
    ) -> Result<(R::Response, Value)>
    where
        R: JsonRpcRequest + Send + Sync + 'static,
        R::Response: Send + 'static,
    {
        let message = request.to_untyped_message()?;
        let method = message.method.clone();
        let result = self.request_raw(&method, message.params, timeout).await?;
        let typed = R::Response::from_value(&method, result.clone())?;
        Ok((typed, result))
    }

    fn notify<N>(&self, notification: N) -> Result<()>
    where
        N: JsonRpcNotification + Send + Sync + 'static,
    {
        let message = notification.to_untyped_message()?;
        let envelope = json!({
            "jsonrpc": "2.0",
            "method": message.method,
            "params": message.params,
        });
        self.tx
            .send(Message::Text(envelope.to_string().into()))
            .context("WebSocket writer is closed")?;
        Ok(())
    }

    async fn request_raw(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if self
            .tx
            .send(Message::Text(envelope.to_string().into()))
            .is_err()
        {
            self.pending.lock().await.remove(&id);
            bail!("WebSocket writer is closed");
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result.map_err(anyhow::Error::msg),
            Ok(Err(_)) => bail!("request {method} was dropped"),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                bail!("request {method} timed out after {}s", timeout.as_secs());
            }
        }
    }
}

async fn fail_pending(pending: &PendingRequests, message: impl Into<String>) {
    let message = message.into();
    for (_, waiter) in pending.lock().await.drain() {
        let _ = waiter.send(Err(message.clone()));
    }
}

async fn handle_inbound(
    pending: &PendingRequests,
    tx: &mpsc::UnboundedSender<Message>,
    assistant_text: &Mutex<String>,
    text: &str,
    permission: PermissionPolicy,
    stream_events: bool,
) -> Result<()> {
    let value: Value = serde_json::from_str(text).context("inbound JSON")?;
    if value.get("method").is_none() {
        let Some(id) = value.get("id").and_then(Value::as_i64) else {
            return Ok(());
        };
        let result = if let Some(error) = value.get("error") {
            Err(error.to_string())
        } else {
            Ok(value.get("result").cloned().unwrap_or(Value::Null))
        };
        if let Some(waiter) = pending.lock().await.remove(&id) {
            let _ = waiter.send(result);
        }
        return Ok(());
    }

    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    if stream_events && method == "session/update" {
        let session_id = params.get("sessionId").cloned().unwrap_or(Value::Null);
        let update = params.get("update").cloned().unwrap_or(Value::Null);
        if let Some(event) = events::compact_session_update(&session_id, &update) {
            events::observe_assistant_text(&mut *assistant_text.lock().await, &event);
            let _ = output::write_event(&event);
        }
    } else if stream_events
        && matches!(
            method.as_str(),
            "querymt/session/inputState" | "_querymt/session/inputState"
        )
        && let Some(event) = events::compact_input_state(&params)
    {
        let _ = output::write_event(&event);
    }

    if let Some(id) = value.get("id").cloned() {
        let (reply, event) = inbound_request_reply(id, &method, &params, permission)?;
        if stream_events && let Some(event) = event {
            let _ = output::write_event(&event);
        }
        tx.send(Message::Text(reply.to_string().into()))
            .context("reply to inbound request")?;
    }
    Ok(())
}

fn inbound_request_reply(
    id: Value,
    method: &str,
    params: &Value,
    permission: PermissionPolicy,
) -> Result<(Value, Option<Value>)> {
    if acp::RequestPermissionRequest::matches_method(method) {
        let request = acp::RequestPermissionRequest::parse_message(method, params)?;
        let (response, event) = policy::permission_response(&request, permission);
        let result = response.into_json(method)?;
        return Ok((
            json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Some(event),
        ));
    }
    if acp::CreateElicitationRequest::matches_method(method) {
        let request = acp::CreateElicitationRequest::parse_message(method, params)?;
        let result = policy::elicitation_cancel_response().into_json(method)?;
        return Ok((
            json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Some(policy::elicitation_event(&id, &request)),
        ));
    }

    Ok((
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("qmacp does not support {method}") },
        }),
        None,
    ))
}

fn client_capabilities() -> acp::ClientCapabilities {
    acp::ClientCapabilities::new().terminal(false).elicitation(
        acp::ElicitationCapabilities::new().form(acp::ElicitationFormCapabilities::new()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_reply_uses_allow_once_by_default() {
        let request = acp::RequestPermissionRequest::new(
            "session-1",
            acp::ToolCallUpdate::new("tool-1", acp::ToolCallUpdateFields::new()),
            vec![
                acp::PermissionOption::new(
                    "reject",
                    "Reject",
                    acp::PermissionOptionKind::RejectOnce,
                ),
                acp::PermissionOption::new("allow", "Allow", acp::PermissionOptionKind::AllowOnce),
            ],
        );
        let params = request.to_untyped_message().unwrap().params;
        let (reply, event) = inbound_request_reply(
            json!("querymt:conn:1"),
            "session/request_permission",
            &params,
            PermissionPolicy::AllowOnce,
        )
        .unwrap();
        assert_eq!(reply["id"], "querymt:conn:1");
        assert_eq!(reply["result"]["outcome"]["optionId"], "allow");
        assert_eq!(event.unwrap()["optionId"], "allow");
    }

    #[test]
    fn elicitation_reply_cancels_and_keeps_string_id() {
        let request = acp::CreateElicitationRequest::new(
            acp::ElicitationFormMode::new(
                acp::ElicitationSessionScope::new("session-1"),
                acp::ElicitationSchema::new().string("selection", true),
            ),
            "choose",
        );
        let params = request.to_untyped_message().unwrap().params;
        let (reply, event) = inbound_request_reply(
            json!("elic-1"),
            "elicitation/create",
            &params,
            PermissionPolicy::AllowOnce,
        )
        .unwrap();
        assert_eq!(reply["id"], "elic-1");
        assert_eq!(reply["result"]["action"], "cancel");
        assert_eq!(event.unwrap()["cancelled"], true);
    }
}
