# Quick add

Quick add lets a user capture a title from the board's one-line `+` input without a form takeover, then see the new task selected on the list.

## Sub-features

- `quick-add-open` opens the title line with `+`.
- `quick-add-save` saves with Enter and closes the line.
- `quick-add-cancel` discards with Esc.

## How to get to it (user POV)

- On the board, press `+` (or click `+ add` in the footer).
- Type a title and press Enter to save, or Esc to cancel.
- Herdr prefix+a / `tsk capture` opens the expanded capture page; that path is adjacent, not this recipe.

## Driving it with control-tsk

Preconditions:

- `control-tsk doctor` reports `"ok": true`.
- Board is running: `control-tsk board start`.
- No desk task is titled `Verify quick add`.

- **Open line.** Run `control-tsk board keys +`. Then `control-tsk board wait "add to desk"`. Dump: the list is still visible; chrome includes `title…`, `add to desk`, and `enter save · tab details · esc close`.
- **Type title.** Run `control-tsk board keys 'text:Verify quick add'` (one quoted argv token).
- **Save.** Run `control-tsk board keys enter`.
- **Proof.** Run `control-tsk cli -- list --desk --json`. Exit `0` and stdout contain `"title":"Verify quick add"`. Dump: the title is on the list with `▸`. There is no toast (`Title required` / `saved` / `captured` absent).
- **Cancel path.** Open `+` again, type `'text:Discard me'`, press `esc`. `list --desk --json` must not gain `Discard me`.

## Gotchas

- Success has no status message. The new row (and selection) is the feedback.
- Quote `text:` when the title has spaces. Unquoted `text:Verify quick add` sends only `Verify` and dies on `quick`.
- Capture tokens `!p` / `!t` consume arguments from the title. Avoid them unless proving tokens.
- Expanded Tab capture owns the whole frame; do not expect a side-by-side board at wide widths. Do not press Tab on this recipe.
- Esc is labeled `close` on the verb bar.
