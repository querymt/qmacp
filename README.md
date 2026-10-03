# qmtacp

CLI for a running QueryMT ACP WebSocket server (`qmtcode --acp-ws`).
Default endpoint is `ws://127.0.0.1:3030/acp/ws`.

Each invocation connects, initializes, does one job, and exits. Session ids
are the only state. JSON goes to stdout; connection logs go to stderr.

## Install

Cargo:

```sh
cargo install --path .
cargo install --git https://github.com/querymt/qmtacp
```

Nix profile:

```sh
nix profile install .
nix profile install github:querymt/qmtacp
```

```sh
qmtacp --version
qmtacp --help
```

`--version` prints `qmtacp <crate-version> (<short-git-sha>)` and does not connect.

```sh
cargo run -- caps
cargo run -- new --cwd . --profile default --mode build
cargo run -- sessions --cwd . --limit 20
cargo run -- models --query grok --provider xai
cargo run -- --quiet prompt --new --cwd . "fix the build"
cargo run -- --quiet prompt SESSION_ID "continue"
cargo run -- runtime SESSION_ID
cargo run -- follow SESSION_ID
cargo run -- steer SESSION_ID --run-id RUN "stop and summarize"
cargo run -- queue SESSION_ID "next: run tests"
cargo run -- discard-queued SESSION_ID INPUT_ID
cargo run -- inspect SESSION_ID --messages 20
cargo run -- set-mode SESSION_ID plan
cargo run -- cancel SESSION_ID
```

`prompt` streams compact NDJSON (`text`, `tool`, `mode`, `plan`, `input_state`,
`done`) and waits until the session is idle, including queued turns. `done`
includes assembled assistant `text`. For an existing busy session,
`--delivery auto` (default) steers if possible, otherwise queues. The
`INPUT_ID` passed to `discard-queued` is the `inputId` from an `input_state`
event, which matches the `clientInputId` returned when the input is submitted.
`follow SESSION` streams updates until idle. `exec` runs multiple commands on
one connection. `--quiet` hides connection logs.

`--permission allow-once` is the default so coder tools can run. Elicitation
requests are cancelled and emitted as NDJSON events during `prompt`.
