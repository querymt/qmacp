# CLI setup and I/O

Use this reference for installation, endpoint selection, global flags, permissions,
output handling, and batching. For work in a session, see [live control](control.md).

## Install

If `qmacp` is not on `PATH`, install it with the method that fits the current
system. A missing binary is not a server failure. Prefer Nix when available,
including on NixOS; otherwise use Cargo. Choose one:

```sh
nix profile install github:querymt/qmacp
cargo install --git https://github.com/querymt/qmacp
```

`--version` / `-V` and `--help` / `-h` do not connect. Version output is
`qmacp <crate-version> (<short-git-sha>)`.

## Globals and endpoints

All global flags come before the subcommand:

```sh
qmacp [--url URL] [--allow-insecure] [--pretty] [--permission allow-once|reject-once|cancel] [--quiet] <command>
```

- `--url` / `-u`: `ws://`/`wss://` URL, or `host[:port][/path]`.
  Default: `ws://127.0.0.1:3030/acp/ws`.
- `--allow-insecure`: required for plaintext `ws://` to non-loopback hosts.
  It does not authenticate the endpoint. Use a trusted tunnel or an authenticated
  deployment with `wss://` for remote access; do not expose public agent control.
- `--pretty`: pretty JSON. Do not use on NDJSON streams.
- `--quiet`: hide connection logs on stderr.

## Output

- Most commands: one JSON object on stdout (`ok` is set).
- `prompt`, `follow`, `exec`, and opt-in `new --stream`: NDJSON.
  `watch` emits `runtime` events, then a final JSON object.
  Connection logs go to stderr.
- Success requires exit 0 and JSON `ok` not false. `prompt`, `follow`, and
  `new --stream` also require a terminal `"type":"done"`. `exec` emits individual
  command results, not a global `done`.
- Exit codes: 0 success, 1 usage, 2 connection, 3 RPC, 4 timeout/cancel.

Stream event types: `session`, `submitted`, `text`, `tool`, `mode`, `plan`,
`input_state`, `permission`, `elicitation`, `runtime`, `update`, `done`.
Do not claim a cancel/discard/close/config change succeeded without exit 0 and
JSON `ok: true`. Inspect current state after uncertain delivery rather than retrying
blindly; see [live control](control.md).

`new --stream`/`prompt --new` emit `sessionId` before configuration. Default
`new` still returns one object. Queue/steer prompts emit `submitted` with
`clientInputId` and `receipt` before waiting; `done` retains them.

On failure, `recovery` preserves known IDs, the failing `stage`
(`create|configure|submit|observe`), and creation/delivery state. `not_submitted`
or `rejected` differs from `unknown`; `accepted` means a receipt was received,
not that the task completed. Inspect state before retrying an uncertain or
accepted submission. A lost creation response cannot provide a session ID.

`observationState` is `not_waited`, `idle`, or `prompt_returned`, not proof of
task success. `text` contains only assistant text observed on this connection.

## Permissions

`--permission` is global and must precede the subcommand:

- Default: `allow-once` (tools run). Passing it is redundant.
- `reject-once`: reject tool execution once.
- `cancel`: fail closed; do not select an option.
- If the wanted option is missing, `allow-once`/`reject-once` may select the first
  advertised option, which can be `allowAlways`. Do not assume they fail closed.

Elicitation is always cancelled and emitted as `"type":"elicitation"` with
`cancelled: true`. Make unattended prompts self-contained.

## Batch commands

`exec` runs multiple commands over one connection and streams NDJSON:

```sh
qmacp --quiet exec <<EOF
status --cwd "$PWD" "$SESSION_ID"
runtime "$SESSION_ID"
get-lifecycle "$SESSION_ID"
set-lifecycle "$SESSION_ID" --resolved false
sessions --resolution open --archive excluded
inspect --cwd "$PWD" --messages 20 --tools false "$SESSION_ID"
EOF
```

This is not a shell: no escaping, no using an ID returned by an earlier line,
and no stdin payloads inside a stdin batch (including queue/steer). Such reads
are rejected before any batch command runs. Discover IDs before the batch.
See [session management](sessions.md) for lifecycle and capability requirements.
