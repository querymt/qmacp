# qmacp

JSON CLI for a running QueryMT ACP WebSocket server (`qmtcode --acp-ws`).

Each invocation connects, does one job, and exits. The server owns session state. Keep the returned `sessionId`. JSON goes to stdout. Connection logs go to stderr.

Default endpoint: `ws://127.0.0.1:3030/acp/ws`.

## Install

Skill, for agents:

```sh
npx skills add querymt/qmacp
bunx skills add querymt/qmacp
```

Binary. Prefer Nix when `nix` is available. Otherwise use Cargo.

```sh
nix profile install github:querymt/qmacp
cargo install --git https://github.com/querymt/qmacp
```

From a checkout:

```sh
cargo install --path .
nix profile install .
```

`--version` and `--help` do not connect. `--version` prints `qmacp <crate-version> (<short-git-sha>)`.

## Use

```sh
qmacp --quiet caps
qmacp --quiet prompt --new --cwd . --timeout 900 'fix the build'
qmacp --quiet prompt --cwd . --timeout 900 "$SESSION_ID" 'run the tests'
qmacp --quiet runtime "$SESSION_ID"
qmacp --quiet follow "$SESSION_ID"
```

Globals come before the subcommand: `--url`, `--allow-insecure`, `--pretty`, `--permission`, `--quiet`.

`--permission` defaults to `allow-once`. `reject-once` rejects one tool call. `cancel` selects nothing.

## Commands

Discover: `caps`, `profiles`, `models`

Sessions: `new`, `sessions`, `find`, `inspect`, `status`, `close`

Lifecycle: `get-lifecycle`, `set-lifecycle`; `--resolution any|open|resolved` and `--archive any|excluded|only` filter `sessions` and `find` on the server before pagination. Omitting both filters preserves the existing listing behavior.

Configure: `set-mode` (`build|plan|review`), `set-model`, `set-effort` (`auto|low|medium|high|max`)

Work: `prompt`, `follow`, `watch`, `steer`, `queue`, `discard-queued`, `exec`, `cancel`, `runtime`

`prompt --new TEXT` creates a session. `prompt SESSION_ID TEXT` continues one. `-` reads the prompt from stdin. `--delivery auto` prompts when idle, steers when the turn is steerable, otherwise queues. It waits until idle, including queued turns.

`discard-queued` takes the `inputId` from an `input_state` event. That is the same value as `clientInputId` from the submit response.

Lifecycle updates are explicit desired states, not toggles. At least one field is required; omitted fields remain unchanged. `caps` reports lifecycle feature/method support, and lifecycle commands/filters fail explicitly when the server does not advertise support. Session summaries preserve available lifecycle data in `_meta.querymt.lifecycle` and omit it when unavailable.

```sh
qmacp --quiet get-lifecycle "$SESSION_ID"
qmacp --quiet set-lifecycle "$SESSION_ID" --resolved true
qmacp --quiet set-lifecycle "$SESSION_ID" --resolved false --archived true
qmacp --quiet sessions --resolution open --archive excluded
qmacp --quiet find --query 'build' --resolution resolved --archive only
```

`exec` runs several commands on one connection. It is not a shell. Lifecycle commands are supported in `exec` too:

```sh
qmacp --quiet exec <<EOF
get-lifecycle $SESSION_ID
set-lifecycle $SESSION_ID --resolved false
sessions --resolution open --archive excluded
EOF
```

## Output

Most commands print one JSON object. `prompt`, `follow`, and `exec` stream compact NDJSON. `watch` also emits `runtime` events.

Success is exit 0 and `ok` not false. Streamed commands also need a `"type":"done"` event. `done.text` is the assembled assistant response.

Exit codes: 0 ok, 1 usage, 2 connection, 3 RPC, 4 timeout or cancel.

Event types: `session`, `text`, `tool`, `mode`, `plan`, `input_state`, `permission`, `elicitation`, `runtime`, `update`, `done`.

Elicitation is cancelled and emitted as an event.
