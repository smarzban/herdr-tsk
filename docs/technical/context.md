# Context and scope

**Responsibility.** Turn host invocation JSON and process cwd into an
`InvocationSnapshot` containing capture scope, resolved repository, title prefill,
and provenance. It also resolves project tokens for board `!p` and CLI `--project`.

**Public surface.** `context::{CONTEXT_JSON_ENV, RawHostContext, InvocationSnapshot,
resolve_repo_root, build_snapshot, snapshot_from_env}` and
`scope::{resolve_flag_scope, resolve_project_path}`.

## Snapshot

`HERDR_PLUGIN_CONTEXT_JSON` deserializes into `RawHostContext`. Unknown keys are
ignored; missing or malformed JSON becomes empty context. Effective cwd is the first
non-empty focused-pane cwd, workspace cwd, or explicit cwd, else the process cwd.

`resolve_repo_root` walks cwd ancestors for a `.git` file or directory. A resolved
root becomes the project scope; otherwise the scope is desk. Selected text prefills
the title and sets selection provenance, otherwise provenance is capture.

The snapshot deliberately does not retain or persist host pane, agent, worktree,
branch, file, or line metadata.

## Project tokens

`resolve_project_path` accepts slash-containing tokens verbatim. Other tokens match
a unique ASCII-case-insensitive project basename from existing tasks or the snapshot.
Missing and ambiguous names are kept verbatim.

`resolve_flag_scope` gives `--project` priority, then `--desk`, then the snapshot
default. `TaskScope::Global` serializes as `global` and displays as desk.
