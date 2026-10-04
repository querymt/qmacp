use std::io::{self, Read};
use std::path::PathBuf;
use std::time::Duration;

use agent_client_protocol::schema::v1 as acp;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use crate::client::AcpClient;
use crate::error::{CliError, ExitCode};
use crate::output;
use crate::session;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Delivery {
    #[default]
    Auto,
    Prompt,
    Steer,
    Queue,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Protocol version, agent info, ACP session features, and QueryMT capabilities.
    Caps,
    /// List QueryMT profiles advertised by the server.
    Profiles,
    /// List models advertised by the server.
    Models {
        #[arg(long)]
        refresh: bool,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        provider: Option<String>,
    },
    /// Create a session.
    New {
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
    },
    /// List sessions with ACP pagination.
    Sessions {
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        cursor: Option<String>,
        /// Follow nextCursor until exhausted or the page cap is reached.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Client-side search over session/list pages.
    Find {
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        phase: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Compact session history from session/load.
    Inspect {
        session_id: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        messages: Option<usize>,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        tools: bool,
    },
    /// Current mode/model/profile/effort for a session.
    Status {
        session_id: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Set session mode (build|plan|review).
    SetMode { session_id: String, mode: String },
    /// Set session model id.
    SetModel { session_id: String, model: String },
    /// Set reasoning effort (auto|low|medium|high|max).
    SetEffort { session_id: String, effort: String },
    /// Send a prompt. `prompt --new TEXT` or `prompt SESSION_ID TEXT`.
    Prompt {
        /// Create a session, then prompt. Remaining args are the prompt text.
        #[arg(long)]
        new: bool,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        #[arg(long)]
        timeout: Option<u64>,
        /// How to send text to an existing session.
        #[arg(long, value_enum, default_value_t = Delivery::Auto)]
        delivery: Delivery,
        /// Wait until runtime is idle, including queued turns. Default true.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        until_idle: bool,
        /// `SESSION_ID TEXT` or, with --new, just `TEXT`. Use `-` to read stdin.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Current QueryMT runtime state for a session.
    Runtime { session_id: String },
    /// Stream compact session events until idle.
    Follow {
        session_id: String,
        #[arg(long, default_value_t = 1)]
        interval: u64,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
    },
    /// Poll runtime state until idle or timeout.
    Watch {
        session_id: String,
        #[arg(long, default_value_t = 1)]
        interval: u64,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        idle: bool,
    },
    /// Steer an in-flight turn.
    Steer {
        session_id: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        text: Vec<String>,
    },
    /// Queue input for after the current turn.
    Queue {
        session_id: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        text: Vec<String>,
    },
    /// Remove an input that has not started from the session queue.
    DiscardQueued {
        session_id: String,
        input_id: String,
    },
    /// Run multiple commands on one connection. Lines from stdin or remaining args.
    Exec {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        commands: Vec<String>,
    },
    /// Cancel the current turn for a session.
    Cancel { session_id: String },
    /// Close a materialized session.
    Close { session_id: String },
}

pub struct CommandOutcome {
    pub exit: ExitCode,
}

pub async fn run(
    client: &AcpClient,
    command: Command,
    pretty: bool,
) -> Result<CommandOutcome, CliError> {
    match command {
        Command::Caps => write_ok(pretty, caps(client).await?),
        Command::Profiles => write_ok(pretty, profiles(client).await?),
        Command::Models {
            refresh,
            query,
            provider,
        } => write_ok(pretty, models(client, refresh, query, provider).await?),
        Command::New {
            cwd,
            profile,
            mode,
            model,
            effort,
        } => write_ok(
            pretty,
            new_session(client, cwd, profile, mode, model, effort).await?,
        ),
        Command::Sessions {
            cwd,
            cursor,
            all,
            limit,
        } => write_ok(pretty, sessions(client, cwd, cursor, all, limit).await?),
        Command::Find {
            cwd,
            query,
            phase,
            limit,
        } => write_ok(pretty, find(client, cwd, query, phase, limit).await?),
        Command::Inspect {
            session_id,
            cwd,
            full,
            messages,
            tools,
        } => write_ok(
            pretty,
            inspect(client, session_id, cwd, full, messages, tools).await?,
        ),
        Command::Status { session_id, cwd } => {
            write_ok(pretty, status(client, session_id, cwd).await?)
        }
        Command::SetMode { session_id, mode } => {
            write_ok(pretty, set_mode(client, session_id, mode).await?)
        }
        Command::SetModel { session_id, model } => {
            write_ok(pretty, set_model(client, session_id, model).await?)
        }
        Command::SetEffort { session_id, effort } => {
            write_ok(pretty, set_effort(client, session_id, effort).await?)
        }
        Command::Prompt {
            new,
            cwd,
            profile,
            mode,
            model,
            effort,
            timeout,
            delivery,
            until_idle,
            args,
        } => {
            prompt(
                client,
                PromptArgs {
                    new,
                    cwd,
                    profile,
                    mode,
                    model,
                    effort,
                    timeout,
                    delivery,
                    until_idle,
                    args,
                    pretty,
                },
            )
            .await
        }
        Command::Runtime { session_id } => write_ok(pretty, runtime(client, session_id).await?),
        Command::Follow {
            session_id,
            interval,
            timeout,
        } => follow(client, session_id, interval, timeout, pretty).await,
        Command::Watch {
            session_id,
            interval,
            timeout,
            idle,
        } => watch(client, session_id, interval, timeout, idle, pretty).await,
        Command::Steer {
            session_id,
            run_id,
            text,
        } => write_ok(
            pretty,
            submit_input(client, "querymt/session/steer", session_id, run_id, text).await?,
        ),
        Command::Queue { session_id, text } => write_ok(
            pretty,
            submit_input(client, "querymt/session/queue", session_id, None, text).await?,
        ),
        Command::DiscardQueued {
            session_id,
            input_id,
        } => write_ok(pretty, discard_queued(client, session_id, input_id).await?),
        Command::Exec { commands } => exec(client, commands, pretty).await,
        Command::Cancel { session_id } => write_ok(pretty, cancel(client, session_id).await?),
        Command::Close { session_id } => write_ok(pretty, close(client, session_id).await?),
    }
}

fn write_ok(pretty: bool, value: Value) -> Result<CommandOutcome, CliError> {
    output::write_json(&output::ok(value), pretty).map_err(CliError::rpc)?;
    Ok(CommandOutcome { exit: ExitCode::Ok })
}

async fn caps(client: &AcpClient) -> Result<Value, CliError> {
    let initialized = client.initialize().await.map_err(CliError::from_request)?;
    let querymt = match client.extension("querymt/capabilities", json!({})).await {
        Ok(value) => extension_payload(&value),
        Err(_) => Value::Null,
    };
    let agent = initialized.agent_info.as_ref();
    let session = &initialized.agent_capabilities.session_capabilities;
    Ok(json!({
        "protocolVersion": initialized.protocol_version,
        "agent": {
            "name": agent.map(|info| info.name.clone()),
            "title": agent.and_then(|info| info.title.clone()),
            "version": agent.map(|info| info.version.clone()),
        },
        "authMethods": initialized.auth_methods.iter().map(|method| method.id().to_string()).collect::<Vec<_>>(),
        "acp": {
            "loadSession": initialized.agent_capabilities.load_session,
            "list": session.list.is_some(),
            "resume": session.resume.is_some(),
            "close": session.close.is_some(),
            "delete": session.delete.is_some(),
        },
        "querymt": querymt,
    }))
}

async fn profiles(client: &AcpClient) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let response = client
        .extension("querymt/profiles", json!({}))
        .await
        .map_err(CliError::from_request)?;
    Ok(extension_payload(&response))
}

async fn models(
    client: &AcpClient,
    refresh: bool,
    query: Option<String>,
    provider: Option<String>,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let (method, params) = if refresh {
        (
            "querymt/refreshModels",
            json!({ "wait_for_completion": true }),
        )
    } else {
        ("querymt/models", json!({}))
    };
    let response = client
        .extension(method, params)
        .await
        .map_err(CliError::from_request)?;
    Ok(session::filter_models(
        extension_payload(&response),
        query.as_deref(),
        provider.as_deref(),
    ))
}

async fn new_session(
    client: &AcpClient,
    cwd: Option<PathBuf>,
    profile: Option<String>,
    mode: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let cwd = session::resolve_cwd(cwd).map_err(CliError::rpc)?;
    let (created, created_raw) = client
        .new_session(cwd.clone(), profile.as_deref())
        .await
        .map_err(CliError::from_request)?;
    let session_id = created.session_id.to_string();
    apply_config(client, &session_id, mode, model, effort).await?;
    let status = match open_session(client, session_id.clone(), cwd.clone()).await {
        Ok(opened) => opened.status,
        Err(_) => session::config_status(&session_id, created.modes.as_ref(), &created_raw),
    };
    Ok(json!({
        "sessionId": session_id,
        "cwd": cwd,
        "status": status,
    }))
}

async fn sessions(
    client: &AcpClient,
    cwd: Option<PathBuf>,
    cursor: Option<String>,
    all: bool,
    limit: Option<usize>,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let cwd = session::resolve_optional_cwd(cwd).map_err(CliError::rpc)?;
    let limit = limit.unwrap_or(if all { usize::MAX } else { 20 }).max(1);
    let mut sessions = Vec::new();
    let mut next = cursor;
    let mut pages = 0usize;
    loop {
        pages += 1;
        if pages > session::find_page_cap() {
            return Ok(json!({
                "cwd": cwd,
                "sessions": sessions,
                "nextCursor": next,
                "truncated": true,
            }));
        }
        let page = client
            .list_sessions(cwd.clone(), next.clone())
            .await
            .map_err(CliError::from_request)?;
        for item in page.sessions {
            sessions.push(session::session_summary(&item));
        }
        if sessions.len() >= limit {
            return Ok(json!({
                "cwd": cwd,
                "sessions": sessions,
                "nextCursor": page.next_cursor,
                "truncated": page.next_cursor.is_some(),
            }));
        }
        match page.next_cursor {
            Some(cursor) if all => next = Some(cursor),
            next_cursor => {
                return Ok(json!({
                    "cwd": cwd,
                    "sessions": sessions,
                    "nextCursor": next_cursor,
                    "truncated": false,
                }));
            }
        }
    }
}

async fn find(
    client: &AcpClient,
    cwd: Option<PathBuf>,
    query: Option<String>,
    phase: Option<String>,
    limit: usize,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let cwd = session::resolve_optional_cwd(cwd).map_err(CliError::rpc)?;
    let query = query.unwrap_or_default();
    let limit = limit.max(1);
    let mut matches = Vec::new();
    let mut next = None;
    let mut pages = 0usize;
    loop {
        pages += 1;
        if pages > session::find_page_cap() {
            break;
        }
        let page = client
            .list_sessions(cwd.clone(), next.clone())
            .await
            .map_err(CliError::from_request)?;
        for listed in page.sessions {
            let summary = session::session_summary(&listed);
            if session::matches_query_and_phase(&summary, &query, phase.as_deref()) {
                matches.push(summary);
                if matches.len() >= limit {
                    return Ok(json!({
                        "query": query,
                        "phase": phase,
                        "cwd": cwd,
                        "sessions": matches,
                    }));
                }
            }
        }
        match page.next_cursor {
            Some(cursor) => next = Some(cursor),
            None => break,
        }
    }
    Ok(json!({
        "query": query,
        "phase": phase,
        "cwd": cwd,
        "sessions": matches,
    }))
}

async fn inspect(
    client: &AcpClient,
    session_id: String,
    cwd: Option<PathBuf>,
    full: bool,
    messages: Option<usize>,
    tools: bool,
) -> Result<Value, CliError> {
    let initialized = client.initialize().await.map_err(CliError::from_request)?;
    if !initialized.agent_capabilities.load_session {
        return Err(CliError::rpc("agent does not support session/load"));
    }
    let cwd = session::resolve_cwd(cwd).map_err(CliError::rpc)?;
    let (loaded, load_value) = client
        .load_session(session_id.clone(), cwd.clone())
        .await
        .map_err(CliError::from_request)?;
    if full {
        return Ok(json!({
            "sessionId": session_id,
            "cwd": cwd,
            "status": session::config_status(
                &session_id,
                loaded.modes.as_ref(),
                &load_value,
            ),
            "raw": load_value,
        }));
    }
    Ok(session::compact_inspect(
        &session_id,
        &cwd,
        loaded.modes.as_ref(),
        &load_value,
        messages,
        tools,
    ))
}

async fn status(
    client: &AcpClient,
    session_id: String,
    cwd: Option<PathBuf>,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let cwd = session::resolve_cwd(cwd).map_err(CliError::rpc)?;
    let opened = open_session(client, session_id.clone(), cwd).await?;
    Ok(opened.status)
}

async fn set_mode(client: &AcpClient, session_id: String, mode: String) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    client
        .set_mode(session_id.clone(), mode.clone())
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "mode": mode,
    }))
}

async fn set_model(
    client: &AcpClient,
    session_id: String,
    model: String,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let (_, raw) = client
        .set_config(session_id.clone(), "model", &model)
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "model": session::config_values_from_raw(&raw)
            .get("model")
            .cloned()
            .unwrap_or(json!(model)),
        "config": session::config_values_from_raw(&raw),
    }))
}

async fn set_effort(
    client: &AcpClient,
    session_id: String,
    effort: String,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let (_, raw) = client
        .set_config(session_id.clone(), "reasoning_effort", &effort)
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "reasoningEffort": session::config_values_from_raw(&raw)
            .get("reasoning_effort")
            .cloned()
            .unwrap_or(json!(effort)),
        "config": session::config_values_from_raw(&raw),
    }))
}

struct PromptArgs {
    new: bool,
    cwd: Option<PathBuf>,
    profile: Option<String>,
    mode: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    timeout: Option<u64>,
    delivery: Delivery,
    until_idle: bool,
    args: Vec<String>,
    pretty: bool,
}

async fn prompt(client: &AcpClient, args: PromptArgs) -> Result<CommandOutcome, CliError> {
    let PromptArgs {
        new,
        cwd,
        profile,
        mode,
        model,
        effort,
        timeout,
        delivery,
        until_idle,
        args,
        pretty,
    } = args;
    let initialized = client.initialize().await.map_err(CliError::from_request)?;
    let cwd = session::resolve_cwd(cwd).map_err(CliError::rpc)?;
    let (session_id, text) = parse_prompt_args(new, args)?;
    let text = read_prompt_text(text).map_err(CliError::rpc)?;
    let session_id = if new {
        let (created, _) = client
            .new_session(cwd.clone(), profile.as_deref())
            .await
            .map_err(CliError::from_request)?;
        created.session_id.to_string()
    } else {
        let Some(session_id) = session_id else {
            return Err(CliError::Usage(
                "prompt requires SESSION_ID or --new".into(),
            ));
        };
        let _ = open_session(client, session_id.clone(), cwd.clone()).await?;
        session_id
    };
    apply_config(client, &session_id, mode, model, effort).await?;
    let chosen = if new {
        Delivery::Prompt
    } else {
        choose_delivery(client, &session_id, delivery).await?
    };
    output::write_event(&json!({
        "type": "session",
        "sessionId": session_id,
        "cwd": cwd,
        "delivery": delivery_name(chosen),
    }))
    .map_err(CliError::rpc)?;
    let _ = client.take_assistant_text().await;

    let timeout = timeout.map(Duration::from_secs);
    let stop_reason = match chosen {
        Delivery::Prompt => {
            let response = client
                .prompt(session_id.clone(), text, timeout)
                .await
                .map_err(CliError::from_request)?;
            stop_reason_name(&response.stop_reason)
        }
        Delivery::Steer => {
            submit_input(
                client,
                "querymt/session/steer",
                session_id.clone(),
                None,
                vec![text],
            )
            .await?;
            "steered".to_string()
        }
        Delivery::Queue => {
            submit_input(
                client,
                "querymt/session/queue",
                session_id.clone(),
                None,
                vec![text],
            )
            .await?;
            "queued".to_string()
        }
        Delivery::Auto => unreachable!("auto is resolved before send"),
    };

    let mut text = client.take_assistant_text().await;
    if until_idle {
        wait_until_idle(
            client,
            &session_id,
            timeout.unwrap_or(Duration::from_secs(120)),
        )
        .await?;
        let more = client.snapshot_assistant_text().await;
        if !more.is_empty() {
            text = more;
        }
        if let Ok(inspected) = inspect(
            client,
            session_id.clone(),
            Some(cwd.clone()),
            false,
            Some(12),
            false,
        )
        .await
            && let Some(last) = session::last_assistant_text(&inspected)
        {
            text = last;
        }
    }

    let done = json!({
        "type": "done",
        "ok": true,
        "sessionId": session_id,
        "delivery": delivery_name(chosen),
        "stopReason": stop_reason,
        "text": text,
        "loadSession": initialized.agent_capabilities.load_session,
    });
    if pretty {
        output::write_json(&done, true).map_err(CliError::rpc)?;
    } else {
        output::write_event(&done).map_err(CliError::rpc)?;
    }
    Ok(CommandOutcome {
        exit: output::exit_for_stop_reason(&stop_reason),
    })
}

fn delivery_name(delivery: Delivery) -> &'static str {
    match delivery {
        Delivery::Auto => "auto",
        Delivery::Prompt => "prompt",
        Delivery::Steer => "steer",
        Delivery::Queue => "queue",
    }
}

async fn choose_delivery(
    client: &AcpClient,
    session_id: &str,
    requested: Delivery,
) -> Result<Delivery, CliError> {
    if requested != Delivery::Auto {
        return Ok(requested);
    }
    let state = runtime(client, session_id.to_string()).await?;
    let phase = state
        .pointer("/runtime/phase")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let steerable = state
        .pointer("/runtime/steerable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(delivery_from_runtime(phase, steerable))
}

fn delivery_from_runtime(phase: &str, steerable: bool) -> Delivery {
    if phase == "idle" {
        Delivery::Prompt
    } else if steerable {
        Delivery::Steer
    } else {
        Delivery::Queue
    }
}

async fn wait_until_idle(
    client: &AcpClient,
    session_id: &str,
    timeout: Duration,
) -> Result<(), CliError> {
    wait_until_idle_every(client, session_id, timeout, Duration::from_millis(400)).await
}

async fn wait_until_idle_every(
    client: &AcpClient,
    session_id: &str,
    timeout: Duration,
    interval: Duration,
) -> Result<(), CliError> {
    let deadline = tokio::time::Instant::now() + timeout.max(Duration::from_secs(1));
    loop {
        let value = runtime(client, session_id.to_string()).await?;
        let phase = value
            .pointer("/runtime/phase")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if phase == "idle" {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(CliError::Timeout(format!(
                "timed out after {}s while session {session_id} was {phase}",
                timeout.as_secs()
            )));
        }
        tokio::time::sleep(interval.max(Duration::from_millis(200))).await;
    }
}

async fn follow(
    client: &AcpClient,
    session_id: String,
    interval: u64,
    timeout: u64,
    pretty: bool,
) -> Result<CommandOutcome, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let cwd = session::resolve_cwd(None).map_err(CliError::rpc)?;
    let _ = open_session(client, session_id.clone(), cwd).await;
    let _ = client.take_assistant_text().await;
    wait_until_idle_every(
        client,
        &session_id,
        Duration::from_secs(timeout.max(1)),
        Duration::from_secs(interval.max(1)),
    )
    .await?;
    let text = client.take_assistant_text().await;
    let done = json!({
        "type": "done",
        "ok": true,
        "sessionId": session_id,
        "stopReason": "idle",
        "text": text,
        "interval": interval,
    });
    if pretty {
        output::write_json(&done, true).map_err(CliError::rpc)?;
    } else {
        output::write_event(&done).map_err(CliError::rpc)?;
    }
    Ok(CommandOutcome { exit: ExitCode::Ok })
}

async fn runtime(client: &AcpClient, session_id: String) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let response = client
        .extension(
            "querymt/session/runtimeState",
            json!({ "session_id": session_id }),
        )
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "runtime": extension_payload(&response),
    }))
}

async fn watch(
    client: &AcpClient,
    session_id: String,
    interval: u64,
    timeout: u64,
    wait_idle: bool,
    pretty: bool,
) -> Result<CommandOutcome, CliError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout.max(1));
    let interval = Duration::from_secs(interval.max(1));
    loop {
        let value = runtime(client, session_id.clone()).await?;
        let phase = value
            .pointer("/runtime/phase")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        output::write_event(&json!({
            "type": "runtime",
            "sessionId": session_id,
            "phase": phase,
            "runtime": value.get("runtime"),
        }))
        .map_err(CliError::rpc)?;
        if !wait_idle || phase == "idle" {
            return write_ok(pretty, value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(CliError::Timeout(format!(
                "watch timed out after {timeout}s while session {session_id} was {phase}"
            )));
        }
        tokio::time::sleep(interval).await;
    }
}

async fn submit_input(
    client: &AcpClient,
    method: &str,
    session_id: String,
    run_id: Option<String>,
    text: Vec<String>,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let text =
        join_prompt_args(text).ok_or_else(|| CliError::Usage(format!("{method} requires TEXT")))?;
    let run_id = if method.ends_with("/steer") && run_id.is_none() {
        runtime(client, session_id.clone())
            .await?
            .pointer("/runtime/active_run_id")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        run_id
    };
    let client_input_id = uuid::Uuid::new_v4().to_string();
    let mut params = json!({
        "session_id": session_id,
        "client_input_id": client_input_id,
        "prompt": [{ "type": "text", "text": text }],
    });
    if let Some(run_id) = run_id {
        params
            .as_object_mut()
            .expect("params object")
            .insert("expected_run_id".to_string(), json!(run_id));
    }
    let response = client
        .extension(method, params)
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "clientInputId": client_input_id,
        "result": extension_payload(&response),
    }))
}

async fn exec(
    client: &AcpClient,
    commands: Vec<String>,
    pretty: bool,
) -> Result<CommandOutcome, CliError> {
    let lines = if commands.is_empty() {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(CliError::rpc)?;
        buf.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        commands
    };
    if lines.is_empty() {
        return Err(CliError::Usage(
            "exec requires commands on stdin or as arguments".into(),
        ));
    }
    let mut last = CommandOutcome { exit: ExitCode::Ok };
    for line in lines {
        let command = parse_exec_line(&line)?;
        last = Box::pin(run(client, command, pretty)).await?;
        if last.exit != ExitCode::Ok {
            return Ok(last);
        }
    }
    Ok(last)
}

#[derive(Parser)]
#[command(name = "qmacp")]
struct ExecLine {
    #[command(subcommand)]
    command: Command,
}

fn parse_exec_line(line: &str) -> Result<Command, CliError> {
    let args = split_exec_args(line);
    let mut argv = vec!["qmacp".to_string()];
    argv.extend(args);
    ExecLine::try_parse_from(&argv)
        .map(|cli| cli.command)
        .map_err(|err| CliError::Usage(err.to_string()))
}

fn split_exec_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None::<char>;
    for ch in line.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => quote = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (None, c) => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

async fn discard_queued(
    client: &AcpClient,
    session_id: String,
    input_id: String,
) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    let response = client
        .extension(
            "querymt/session/discardQueuedInput",
            json!({
                "session_id": session_id,
                "input_id": input_id,
            }),
        )
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "inputId": input_id,
        "result": extension_payload(&response),
    }))
}

async fn cancel(client: &AcpClient, session_id: String) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    client.cancel(session_id.clone()).map_err(CliError::rpc)?;
    Ok(json!({
        "sessionId": session_id,
        "cancelled": true,
    }))
}

fn parse_prompt_args(
    new: bool,
    args: Vec<String>,
) -> Result<(Option<String>, Option<String>), CliError> {
    if new {
        return Ok((None, join_prompt_args(args)));
    }
    let mut args = args.into_iter();
    let Some(session_id) = args.next() else {
        return Err(CliError::Usage(
            "prompt requires SESSION_ID TEXT, or --new TEXT".into(),
        ));
    };
    Ok((Some(session_id), join_prompt_args(args.collect())))
}

fn join_prompt_args(args: Vec<String>) -> Option<String> {
    if args.is_empty() {
        None
    } else {
        Some(args.join(" "))
    }
}

async fn close(client: &AcpClient, session_id: String) -> Result<Value, CliError> {
    client.initialize().await.map_err(CliError::from_request)?;
    client
        .close_session(session_id.clone())
        .await
        .map_err(CliError::from_request)?;
    Ok(json!({
        "sessionId": session_id,
        "closed": true,
    }))
}

async fn apply_config(
    client: &AcpClient,
    session_id: &str,
    mode: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<(), CliError> {
    if let Some(mode) = mode {
        client
            .set_mode(session_id.to_string(), mode)
            .await
            .map_err(CliError::from_request)?;
    }
    if let Some(model) = model {
        client
            .set_config(session_id.to_string(), "model", &model)
            .await
            .map_err(CliError::from_request)?;
    }
    if let Some(effort) = effort {
        client
            .set_config(session_id.to_string(), "reasoning_effort", &effort)
            .await
            .map_err(CliError::from_request)?;
    }
    Ok(())
}

struct OpenedSession {
    status: Value,
}

async fn open_session(
    client: &AcpClient,
    session_id: String,
    cwd: PathBuf,
) -> Result<OpenedSession, CliError> {
    let resumed_status = match client.resume_session(session_id.clone(), cwd.clone()).await {
        Ok((response, raw)) => {
            let status = session::config_status(&session_id, response.modes.as_ref(), &raw);
            if status.get("model").is_some_and(|value| !value.is_null()) {
                return Ok(OpenedSession { status });
            }
            Some(status)
        }
        Err(_) => None,
    };
    match client.load_session(session_id.clone(), cwd).await {
        Ok((loaded, raw)) => Ok(OpenedSession {
            status: session::config_status(&session_id, loaded.modes.as_ref(), &raw),
        }),
        Err(err) => match resumed_status {
            Some(status) => Ok(OpenedSession { status }),
            None => Err(CliError::from_request(err)),
        },
    }
}

fn extension_payload(response: &Value) -> Value {
    response
        .get("data")
        .cloned()
        .unwrap_or_else(|| response.clone())
}

fn read_prompt_text(text: Option<String>) -> Result<String> {
    match text {
        Some(text) if text == "-" => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).context("read stdin")?;
            Ok(buf)
        }
        Some(text) => Ok(text),
        None => {
            if atty_stdin() {
                bail!("prompt text is required (pass TEXT or - for stdin)");
            }
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).context("read stdin")?;
            Ok(buf)
        }
    }
}

fn atty_stdin() -> bool {
    std::io::IsTerminal::is_terminal(&io::stdin())
}

fn stop_reason_name(reason: &acp::StopReason) -> String {
    match reason {
        acp::StopReason::EndTurn => "endTurn".to_string(),
        acp::StopReason::MaxTokens => "maxTokens".to_string(),
        acp::StopReason::MaxTurnRequests => "maxTurnRequests".to_string(),
        acp::StopReason::Refusal => "refusal".to_string(),
        acp::StopReason::Cancelled => "cancelled".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_new_uses_remaining_args_as_text() {
        let (session_id, text) =
            parse_prompt_args(true, vec!["fix".into(), "the".into(), "build".into()]).unwrap();
        assert!(session_id.is_none());
        assert_eq!(text.as_deref(), Some("fix the build"));
    }

    #[test]
    fn prompt_existing_takes_session_then_text() {
        let (session_id, text) =
            parse_prompt_args(false, vec!["sess-1".into(), "continue".into()]).unwrap();
        assert_eq!(session_id.as_deref(), Some("sess-1"));
        assert_eq!(text.as_deref(), Some("continue"));
    }

    #[test]
    fn prompt_without_session_is_usage_error() {
        assert!(matches!(
            parse_prompt_args(false, Vec::new()),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn exec_line_parses_nested_command() {
        let command = parse_exec_line("sessions --cwd /tmp --limit 5").unwrap();
        match command {
            Command::Sessions { limit, .. } => assert_eq!(limit, Some(5)),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn exec_line_parses_discard_queued_command() {
        let command = parse_exec_line("discard-queued session-1 input-1").unwrap();
        assert!(matches!(
            command,
            Command::DiscardQueued { session_id, input_id }
                if session_id == "session-1" && input_id == "input-1"
        ));
    }

    #[test]
    fn split_exec_args_keeps_quoted_text() {
        assert_eq!(
            split_exec_args(r#"prompt --new "fix the build""#),
            vec!["prompt", "--new", "fix the build"]
        );
    }

    #[test]
    fn auto_delivery_is_prompt_when_idle() {
        assert!(matches!(
            delivery_from_runtime("idle", false),
            Delivery::Prompt
        ));
    }

    #[test]
    fn auto_delivery_steers_when_steerable() {
        assert!(matches!(
            delivery_from_runtime("tools", true),
            Delivery::Steer
        ));
    }

    #[test]
    fn auto_delivery_queues_when_busy_and_not_steerable() {
        assert!(matches!(
            delivery_from_runtime("closing", false),
            Delivery::Queue
        ));
    }
}
