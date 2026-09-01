# Context and scope

**Responsibility.** Turn host invocation JSON + process cwd into an
`InvocationSnapshot` (default capture scope, capsule, passive agent meta), and
resolve project tokens the same way for board `!p` and CLI `--project`.

**Public surface.** `context::{CONTEXT_JSON_ENV, RawHostContext, RawWorktree,
InvocationSnapshot, resolve_repo_root, build_snapshot, snapshot_from_env}`.
`scope::{resolve_flag_scope, resolve_project_path}`.

## How it works

### Snapshot

`HERDR_PLUGIN_CONTEXT_JSON` deserializes into `RawHostContext`. Unknown keys ignored.
Missing or invalid JSON → `Default` (all none). `from_json` is the pure parser.

Effective cwd: first non-empty of `focused_pane_cwd`, `workspace_cwd`, `cwd`, else
the process cwd passed into `build_snapshot`.

`resolve_repo_root` walks ancestors for a `.git` **file or directory**. No git
library. Unresolved repo → `TaskScope::Global` (desk).

Capsule fields are filled only from known values (`text::non_empty` trims). Branch
is taken from the host hint, never invented by running git. Completely empty
capsule / agent meta become `None`.

Selected text: non-empty → `title_prefill` and `ProvenanceOrigin::Selection`; else
no prefill and `Capture`.

`this_repo` is the resolved project root; the board uses it as “this repo” for
scope pickers and as the default when a project-scoped board opens capture.

### Project tokens

`resolve_project_path(token, domain, snapshot)`:

- Token contains `/` → verbatim path.
- Else unique ASCII-case-insensitive **basename** match against project paths on
  existing tasks plus the snapshot's default / `this_repo`.
- Missing or ambiguous → verbatim (the token is stored as the path; `tsk list --all`
  is the recovery hatch).

`resolve_flag_scope(project, global, domain, snapshot)`: `--project` wins, else
`--desk` (`global: true`) → `TaskScope::Global`, else snapshot default. CLI flag
`--desk` is this boolean; there is no `--global` flag.

Board capture tokens (`!p` / `!t`) are parsed in the quick-add reducer with the
same rules: bare `!p` is desk, `!p name` is basename, `!p /path` is verbatim; bare
`!t` unthreads; `!t name` runs `normalize_thread`.

## Invariants

- Never invent capsule or agent fields the host did not provide.
- Agent lifecycle is never a human-status authority.
- Board and headless add share `resolve_project_path` so a basename that works in
  one works in the other.
- Desk display vs `global` storage: [invariants](invariants.md) §11.

## Error paths

Context parse cannot fail the board: malformed JSON is empty context. Invalid
thread tokens refuse at the capture/CLI boundary (`ThreadError` / usage exit 2)
before domain create. Ambiguous project basename is not an error — it stores the
token verbatim.

## Extension points

New host fields: add optional keys to `RawHostContext`, map them in
`build_snapshot` only if they should persist on the capsule. Do not start shelling
out to git for branch; the code is explicit that branch is a host hint.
