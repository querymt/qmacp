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
- `prompt`, `follow`, and `exec`: compact NDJSON. `watch` emits `runtime` events,
  then a final JSON object. Connection logs go to stderr.
- Success requires exit 0 and JSON `ok` not false. For `prompt`/`follow`, also
  require a terminal `"type":"done"` event; its `text` is the assembled response.
- Exit codes: 0 success, 1 usage, 2 connection, 3 RPC, 4 timeout/cancel.

Stream event types: `session`, `text`, `tool`, `mode`, `plan`, `input_state`,
`permission`, `elicitation`, `runtime`, `update`, `done`.
Do not claim a cancel/discard/close/config change succeeded without exit 0 and
JSON `ok: true`. Inspect current state after uncertain delivery rather than retrying
blindly; see [live control](control.md).

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
and no stdin prompts inside a stdin batch. Discover IDs before the batch.
See [session management](sessions.md) for lifecycle and capability requirements.
