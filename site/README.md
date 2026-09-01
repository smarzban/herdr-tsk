# tsk site

Marketing page and **user guide** for tsk. Lives in this repo as `site/` — same
split as [herdr's `website/`](https://github.com/herdrdev/herdr/tree/master/website).
Astro + Starlight; the homepage is a custom page, not the Starlight index.
How-to pages are `src/content/docs/docs/`. Maintainer internals stay in
`docs/technical/` at the repo root.

This directory is **not** part of the `tsk` binary. `cargo build` ignores it.

## Local

```bash
cd site
npm install
npm run dev
```

## Build

```bash
cd site
npm run build
```

Output is `site/dist/`.

## Deploy

Vercel should use this repository with **Root Directory** `site`.
`vercel.json` pins install / build / output. Production is
https://tsk-gules.vercel.app.

Ignore builds that do not touch `site/**` so a Rust-only commit does not
rebuild the landing page.
