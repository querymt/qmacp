# Session management

Use this reference to discover real IDs, select a session, inspect/configure it,
and manage lifecycle or pins. Commands require a running `qmtcode --acp-ws`;
see [CLI setup](cli.md) for endpoints, permissions, and output handling.

## Discover capabilities, profiles, and models

```sh
qmacp --quiet caps
qmacp --quiet profiles
qmacp --quiet models
qmacp --quiet models --query '<term>' --provider '<provider>'
qmacp --quiet models --refresh
```

Do not invent profile/model IDs. Refresh only when a fresh provider scan is needed.
`caps.querymt == null` means ACP initialize succeeded but QueryMT extension
capability discovery did not.

## Create or find a session

```sh
qmacp --quiet new --cwd "$PWD" --profile '<profile>' --mode build --model '<model-id>' --effort high
qmacp --quiet sessions --cwd "$PWD" --limit 20
qmacp --quiet sessions --cwd "$PWD" --all
qmacp --quiet sessions --cwd "$PWD" --resolution open --archive excluded
qmacp --quiet find --cwd "$PWD" --query '<title-or-id-fragment>' --phase idle --limit 50
qmacp --quiet find --cwd "$PWD" --resolution resolved --archive only
```

`new` creates only. Prefer `prompt --new` when the next step is a prompt; see
[live control](control.md). Keep the returned `sessionId`. If `find` returns
multiple matches, inspect or ask rather than picking by list order.

## Inspect and configure

Use the selected session's ID as `SESSION_ID` and run from its workspace:

```sh
qmacp --quiet status --cwd "$PWD" "$SESSION_ID"
qmacp --quiet runtime "$SESSION_ID"
qmacp --quiet inspect --cwd "$PWD" --messages 20 --tools false "$SESSION_ID"
qmacp --quiet inspect --cwd "$PWD" --full "$SESSION_ID"
qmacp --quiet set-mode "$SESSION_ID" plan      # build|plan|review
qmacp --quiet set-model "$SESSION_ID" '<model-id>'
qmacp --quiet set-effort "$SESSION_ID" high    # auto|low|medium|high|max
```

`status` is mode/model/profile/effort; `runtime` is phase/queue/steerability.
`inspect --tools` defaults true; use `--tools false` for compact history.
`--full` dumps the raw snapshot (large). Do not treat earlier output as current
state; query again before acting. Require exit 0 and JSON `ok: true` for changes.

## Lifecycle and pins

```sh
qmacp --quiet get-lifecycle "$SESSION_ID"
qmacp --quiet set-lifecycle "$SESSION_ID" --resolved true
qmacp --quiet set-lifecycle "$SESSION_ID" --resolved false --archived true
qmacp --quiet pin "$SESSION_ID"
qmacp --quiet unpin "$SESSION_ID"
qmacp --quiet sessions --pinned
qmacp --quiet find --query 'build' --pinned=false
```

`set-lifecycle` takes explicit desired values, not toggles. At least one of
`--resolved` or `--archived` is required; omitted fields stay unchanged.
`sessions` and `find` accept `--resolution any|open|resolved`,
`--archive any|excluded|only`, and `--pinned` (equivalent to `--pinned=true`) or
`--pinned=false`. Omitted dimensions preserve legacy requests; filters go to the
server for every page.

`caps` reports `features.session_lifecycle`, `features.session_pins`, and supported
methods. Unsupported operations/filters fail rather than inventing state.
Session summaries preserve available `_meta.querymt.lifecycle` and
`_meta.querymt.pin` data, omitting each when unavailable.
