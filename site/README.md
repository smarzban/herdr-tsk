# tsk site

Marketing page and docs for tsk. Lives in this repo as `site/` — same split as
[herdr's `website/`](https://github.com/herdrdev/herdr/tree/master/website).
Astro + Starlight; the homepage is a custom page, not the Starlight index.

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

Production is https://tsk-gules.vercel.app.

The Vercel project must be connected to **`smarzban/herdr-tsk`**, production
branch **`main`**. Leave **Root Directory empty**. A repo-root `vercel.json`
runs `npm ci` / `npm run build` in `site/` and publishes `site/dist`. Setting
Root Directory to `site` fails if Vercel is still reading `tsk-site` (that
repo has no `site/` folder).

`site/vercel.json` is only used if you later set Root Directory to `site`.
It has no `ignoreCommand`: a docs-only `main` tip would cancel a new
project's first deploy.
