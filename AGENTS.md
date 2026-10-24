# AGENTS.md

## Working Rules

- Keep changes minimal and focused; avoid drive-by refactors and unrelated cleanup.
- Preserve existing staged, unstaged and untracked work. Do not stage, unstage,
  commit or push unless the user explicitly requests that specific action.
  Editing an already-staged file does not authorize updating the index.
- Keep this file limited to durable, repository-wide guidance. Do not automatically
  append task-specific corrections.
- Never write to, migrate, replace or delete the user's existing session database.
  Test state-changing operations against mock ACP servers or a separate disposable
  QueryMT instance with a fresh test database, not the user's running server.
- Never stop or restart user-run QueryMT services or pre-existing `qmtcode` processes.
  Stop only processes created for the current test.

## Repository Overview

- `qmacp` is one Rust package with a CLI and an exported native ACP client library.
- `src/main.rs` and `src/commands.rs` implement CLI parsing and command execution.
- `src/client.rs` implements transport; `events`, `session`, `output`, `error`,
  `policy` and `url` contain shared helpers. `src/lib.rs` exposes the library API.
- `tests/` contains integration tests; `skills/qmacp/` contains the agent skill
  and focused Markdown references. Keep `README.md` an overview, not a duplicate
  of those references.

## Development Environment

- Detect Nix first. When available, use the project devShell (`nix develop` or
  `nix develop -c <command>`); an active project direnv shell is sufficient.
- Without Nix, use the supported native toolchain and dependencies. Check
  `flake.nix` and `rust-toolchain.toml`; diagnose setup failures rather than
  silently switching toolchains or environments.
- Run the commands below from the repository root inside the selected environment.

## Validation

- After Rust edits, run `cargo check -p qmacp` (include `--tests` for test edits).
  Run `cargo fmt` only after that check passes.
- Use focused red/green tests where applicable. Each added case must protect a
  distinct required behavior or real regression; prefer existing tests and fixtures.
- Do not add tests solely to prove deleted functionality stays deleted, repeat
  implementation logic or constants, or multiply equivalent input combinations.
  Avoid brittle assertions on incidental values and unrelated lifecycle checks.
- Keep test scaffolding proportional to the change. Moving excess into another
  file is not simplification. Trim redundancy without weakening real regressions.
- Start with focused checks, then expand as needed; consult
  `.github/workflows/ci.yml` for final CI commands:
  ```sh
  cargo check -p qmacp --tests
  cargo test -p qmacp --test agent_workflows
  cargo fmt -p qmacp -- --check
  cargo clippy -p qmacp --all-targets -- -D warnings
  ```
- Live checks must be authorized and limited to read-only operations. Session
  load/resume can restore runtime state; do not treat them as read-only history
  access. Creation, prompting, queueing and configuration tests belong in mocks
  or an isolated instance.
- Report checks actually run, failures and remaining gaps. Documentation-only
  edits do not require a Rust build.

## Code Conventions

- Follow surrounding code, rustfmt and existing error conventions. Preserve
  intentional JSON/NDJSON on stdout and diagnostics on stderr.
- Do not add speculative compatibility layers. If a change requires a backward
  compatibility decision, ask the user before implementing it.
- Backend session/runtime fixes belong in QueryMT; UI fixes belong in
  `querymt-desktop`. Do not expand a qmacp task into sibling repositories without
  authorization.
