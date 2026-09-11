---
title: Storage
description: Task data, backups, deleted tasks, and update checks.
---

The board, CLI, and Herdr plugin share `~/.tsk/tsk.json`.

## Location

| Setting | Default | Purpose |
| --- | --- | --- |
| `TSK_STATE_DIR` | `~/.tsk` | Task data, backups, trash, and release-check cache |
| `TSK_CONFIG_DIR` | `~/.tsk` | Walkthrough preferences |
| `--state-dir <dir>` | State directory | Override storage for a data command |

Use a local disk. NFS and synced folders such as Dropbox or iCloud Drive are unsupported. Directory roots must be real directories, not symlinks.

Herdr's plugin-specific state/config directories do not override these locations. Removing tsk leaves its task data intact.

## Backups

| File | Contains |
| --- | --- |
| `tsk.json` | Current tasks and archived-project records |
| `tsk.json.1` | Previous valid task document |
| `tsk.json.v<N>` | Backup made when migrating an older store format |
| `walkthrough.json` | Whether onboarding was dismissed |
| `delivery.json` | Which starter guides this install has received or dismissed, and the newest release note it has seen |

An older binary refuses a newer or unversioned store instead of rewriting it. Use a compatible tsk version to open it.

Archived tasks stay in the task document with their existing status. [Archive and restore](/docs/board/#archive).

## Starter guides and release notes

The starter guides (`N1`… on your desk) are seeded once per state directory on the first board open. Mark one done, archive it, or delete it and it never returns. Deleting `delivery.json` seeds any guide the task document no longer holds.

After an upgrade, the first board open adds one `What's new in tsk` row to your desk when the new version bundles release notes you have not seen. That includes upgrading from a build that only had starter guides. Every missed release lands in that one row, newest first, with its changelog link in the notes. The notes ship inside the binary; a blank first install records them as seen and shows none. Clear the row like any guide.

## Deleted tasks

Deleted tasks move to `trash.jsonl` once undo can no longer restore them, or after seven days. They are purged 30 days after deletion.

```sh
tsk list --deleted --all
tsk trash restore T12
```

The list includes both recently deleted tasks still in the main store and tasks in trash. Restore reads trash; use board undo for a recent deletion that has not moved there yet.

## Update check

On launch, tsk checks for a newer release if its cached check is older than 24 hours. A newer version appears on the board's idle status row.

| Setting or file | Purpose |
| --- | --- |
| `TSK_NO_UPDATE_CHECK` | Set to disable the check and notice |
| `update.json` | Cached release check in the state directory |

The check requests the latest release tag from GitHub. It does not upload task data. Failures are silent.

[Upgrade tsk](/docs/install/#upgrade).
