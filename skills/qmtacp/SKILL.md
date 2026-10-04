---
name: qmtacp
description: "JSON CLI for a running QueryMT ACP WebSocket server (qmtcode --acp-ws). Find/create sessions, prompt, follow, steer, queue, configure, cancel, and inspect live protocol behavior. Use for operating live sessions and for testing/developing qmtcode."
license: MIT
compatibility: "Requires the qmacp binary on PATH and a running qmtcode --acp-ws. Uses the shell."
allowed-tools: shell
metadata:
  version: "0.1.0"
  tags: "querymt acp sessions automation orchestration qmtcode"
---

# qmacp

Each invocation connects, does one job, and exits. Default URL: `ws://127.0.0.1:3030/acp/ws`.
The server owns session state. Keep the returned `sessionId`.

- Do not invent session, profile, model, mode, effort, or URL values. Discover them (`caps`, `profiles`, `models`, `find`/`sessions`) or take them from JSON.
- Do not close a session unless the user asked to end it, or a qmtcode test explicitly requires `close`.
- Do not use qmacp to edit or test this repo; use ordinary tools. Use it against a running `qmtcode --acp-ws`.
- If connection is refused, check that `qmtcode --acp-ws` is running. Do not retry blindly.

## Install

If `qmacp` is not on `PATH`, install it with the method that fits the current system. Do not treat a missing binary as a server failure.

Prefer the Nix profile when `nix` is available, including on NixOS. Otherwise use Cargo.

```sh
nix profile install github:querymt/qmacp
cargo install --git https://github.com/querymt/qmacp
```

`--version` / `-V` and `--help` / `-h` do not connect.

## Globals

All of these come *before* the subcommand:

```sh
qmacp [--url URL] [--allow-insecure] [--pretty] [--permission allow-once|reject-once|cancel] [--quiet] [--version] [--help] <command>
```

- `--url` / `-u`: `ws://`/`wss://` URL, or `host[:port][/path]`. Default `ws://127.0.0.1:3030/acp/ws`.
- `--allow-insecure`: required for plaintext `ws://` to non-loopback hosts.
- `--pretty`: pretty JSON. Do not use on NDJSON streams.
- `--quiet`: hide connection logs on stderr.
- `--version` / `-V`: print `qmacp <crate-version> (<short-git-sha>)`.
- `--help` / `-h`: print CLI help.

## Output

- Most commands: one JSON object on stdout (`ok` is set).
- `prompt`, `follow`, and `exec`: compact NDJSON. `watch` also emits `runtime` events, then a final JSON object.
- Success requires exit 0 and JSON `ok` not false. For `prompt`/`follow`, also require a `"type":"done"` event.
- Exit: 0 ok, 1 usage, 2 connection, 3 RPC, 4 timeout/cancel.

Streamed event types: `session`, `text`, `tool`, `mode`, `plan`, `input_state`, `permission`, `elicitation`, `runtime`, `update`, `done`.
The terminal prompt/follow event is `"type":"done"`; `text` is the assembled assistant response.

## Default path

New work:

```sh
qmacp --quiet prompt --new --cwd "$PWD" --timeout 900 'do the task and report'
```

Continue:

```sh
qmacp --quiet prompt --cwd "$PWD" --timeout 900 "$SESSION_ID" 'next step'
```

Long prompts: stdin (`-`), not nested quoting. Make prompts self-contained; elicitation is always cancelled.

`--delivery auto` (default) on an existing session: idle → prompt, busy+steerable → steer, else queue.
Do not `steer`, `queue`, `discard-queued`, or `cancel` just because a timeout fired. Run `runtime` first.

## Permissions

`--permission` is global and must come before the subcommand.

- Default is `allow-once` (tools run). Passing it is redundant.
- `--permission reject-once`: reject tool execution once.
- `--permission cancel`: fail-closed; do not select an option.
- If the wanted option is missing, `allow-once`/`reject-once` may select the first advertised option, which can be `allowAlways`.

## Commands

Discover:

```sh
qmacp --quiet caps
qmacp --quiet profiles
qmacp --quiet models
qmacp --quiet models --query '<term>' --provider '<provider>'
qmacp --quiet models --refresh
```

`caps.querymt == null` means ACP initialize succeeded but QueryMT extension discovery did not.

Sessions:

```sh
qmacp --quiet new --cwd "$PWD" --profile '<profile>' --mode build --model '<model-id>' --effort high
qmacp --quiet sessions --cwd "$PWD" --limit 20
qmacp --quiet sessions --cwd "$PWD" --all
qmacp --quiet find --cwd "$PWD" --query '<title-or-id-fragment>' --phase idle --limit 50
```

`new` creates only. Prefer `prompt --new` when the next step is a prompt.
If `find` returns multiple matches, do not pick by list order; inspect or ask.

Inspect / configure:

```sh
qmacp --quiet status --cwd "$PWD" "$SESSION_ID"
qmacp --quiet runtime "$SESSION_ID"
qmacp --quiet inspect --cwd "$PWD" --messages 20 --tools false "$SESSION_ID"
qmacp --quiet inspect --cwd "$PWD" --full "$SESSION_ID"
qmacp --quiet set-mode "$SESSION_ID" plan      # build|plan|review
qmacp --quiet set-model "$SESSION_ID" '<model-id>'
qmacp --quiet set-effort "$SESSION_ID" high    # auto|low|medium|high|max
```

`status` is mode/model/profile/effort. `runtime` is phase/queue/steerability.
`inspect --tools` defaults true; pass `--tools false` for compact history. `--full` dumps the raw snapshot (large).

Prompt:

```sh
qmacp --quiet prompt --new --cwd "$PWD" --profile '<profile>' --mode build --model '<id>' --effort high --timeout 900 'text'
qmacp --quiet prompt --cwd "$PWD" --delivery auto --timeout 900 "$SESSION_ID" 'text'
qmacp --quiet prompt --cwd "$PWD" --delivery prompt|steer|queue --until-idle false "$SESSION_ID" 'text'
qmacp --quiet --permission allow-once prompt --new --cwd "$PWD" --timeout 900 - < instructions.txt
```

`--until-idle` defaults true and waits through queued turns. Default timeout if omitted is 120s.

Live control:

```sh
qmacp --quiet follow "$SESSION_ID" --timeout 900 --interval 1
qmacp --quiet watch "$SESSION_ID" --timeout 900 --interval 1 --idle true
qmacp --quiet steer "$SESSION_ID" --run-id "$RUN_ID" 'adjust the active work'
qmacp --quiet queue "$SESSION_ID" 'do this after the current turn'
qmacp --quiet discard-queued "$SESSION_ID" "$INPUT_ID"
qmacp --quiet cancel "$SESSION_ID"
qmacp --quiet close "$SESSION_ID"
```

`follow` streams session events until idle and loads cwd from the current process; run it from the session workspace.
`watch` polls `runtime` (`"type":"runtime"`) until idle unless `--idle false`. Default `follow`/`watch` timeout is 120s.
`steer --run-id` is optional; if omitted, qmacp uses `runtime.active_run_id`.
Queued submit returns `clientInputId`. `input_state` events expose `inputId`; pass that exact value to `discard-queued`.

Batch:

```sh
qmacp --quiet exec <<EOF
status --cwd "$PWD" "$SESSION_ID"
runtime "$SESSION_ID"
inspect --cwd "$PWD" --messages 20 --tools false "$SESSION_ID"
EOF
```

`exec` is one connection, not a shell: no escaping, no using an id returned by an earlier line, no stdin prompts inside a stdin batch. It streams NDJSON.

## Testing and developing qmtcode

qmacp is a protocol client for `qmtcode --acp-ws`. Use it as a harness while changing qmtcode: start the server, hit it with qmacp, and treat stdout JSON/NDJSON as the assertion surface.

Typical checks:

- Server up and handshake: `caps` (protocol version, ACP session features, QueryMT extensions).
- Discovery: `profiles`, `models` (`--refresh` only when a fresh provider scan is required).
- Session lifecycle: `new` → `status`/`runtime`/`inspect` → `prompt` → `follow`/`watch` → `cancel`/`close`.
- Busy-session behavior: prompt a running session with `--delivery auto|steer|queue`; confirm `input_state` and `clientInputId`.
- Config RPCs: `set-mode` / `set-model` / `set-effort`, then `status` to verify the server accepted the change.
- Permission path: `--permission allow-once|reject-once|cancel` and inspect `permission` events. Do not assume fail-closed unless `cancel`.
- Elicitation: qmacp always cancels; expect `"type":"elicitation"` with `cancelled: true`.
- Load/list features: if `caps.acp.loadSession`/`list`/`close` are false, do not keep calling `inspect`/`sessions`/`close`.

Do not treat a previous command’s output as current runtime state; query `runtime` or `status` again.
Do not claim a cancel/discard/close/config change succeeded unless the command exited 0 and JSON `ok` is true.
For qmtcode work, pass `--cwd` to the qmtcode checkout (or the fixture workspace), not to this qmacp repo unless that is the session under test.
