import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { extname, join } from "node:path";
import test from "node:test";

import { parseCapture } from "../public/capture.js";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("no tracked file still points at the old preview domain", async () => {
  const repoRoot = new URL("../../", import.meta.url).pathname;
  // docs/specs is intentionally out of scope; the rest are untracked build or
  // local-only trees whose contents never ship.
  const skipDirs = new Set([
    ".git", ".review-panel", ".astro", "node_modules", "target", "dist",
    ".vercel", "specs", "technical",
  ]);
  const skipExt = new Set([".png", ".svg", ".ico", ".jpg", ".lock"]);
  // Assembled so this file itself does not contain the literal domain.
  const oldDomain = ["tsk-gules", "vercel", "app"].join(".");
  const offenders = [];
  const walk = async (dir) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        if (!skipDirs.has(entry.name)) await walk(join(dir, entry.name));
      } else if (!skipExt.has(extname(entry.name))) {
        const text = await readFile(join(dir, entry.name), "utf8").catch(() => "");
        if (text.includes(oldDomain)) offenders.push(join(dir, entry.name));
      }
    }
  };
  await walk(repoRoot);
  assert.deepEqual(offenders, [], "stale preview-domain references remain");
});

test("crate, plugin, lockfile, and site share one release version", async () => {
  const [cargo, plugin, lockfile, siteVersion] = await Promise.all([
    read("../../Cargo.toml"),
    read("../../herdr-plugin.toml"),
    read("../../Cargo.lock"),
    read("../src/version.mjs"),
  ]);
  const cargoVersion = cargo.match(/^version = "([^"]+)"/m)?.[1];
  const pluginVersion = plugin.match(/^version = "([^"]+)"/m)?.[1];
  const lockVersion = lockfile.match(/\[\[package\]\]\nname = "tsk-tui"\nversion = "([^"]+)"/)?.[1];
  const renderedVersion = siteVersion.match(/VERSION = '([^']+)'/)?.[1];
  assert.equal(cargoVersion, "0.5.0");
  assert.deepEqual([pluginVersion, lockVersion, renderedVersion], [cargoVersion, cargoVersion, cargoVersion]);
});

test("the Astro site and sitemap build on the canonical URL", async () => {
  const config = await read("../astro.config.mjs");
  assert.match(config, /site: 'https:\/\/gettsk\.sh'/);
});

test("Vercel ignores unchanged files relative to the site root", async () => {
  const config = JSON.parse(await read("../vercel.json"));
  assert.equal(config.ignoreCommand, "git diff --quiet HEAD^ HEAD -- .");
});

test("demo capture keeps an absolute project path verbatim", () => {
  assert.deepEqual(parseCapture("Ship it !p /workspace/herdr-tsk !t Site-Docs", null), {
    title: "Ship it",
    project: "/workspace/herdr-tsk",
    thread: "site-docs",
  });
});

test("the shared theme script is loaded once from the Starlight head", async () => {
  const config = await read("../astro.config.mjs");
  const themeSelect = await read("../src/components/ThemeSelect.astro");
  assert.match(config, /attrs: \{ src: ['"]\/theme\.js['"] \}/);
  assert.doesNotMatch(themeSelect, /theme\.js/);
});

test("demo rows lead with copyable T task identifiers", async () => {
  const demo = await read("../public/board-demo.js");
  assert.match(demo, /let n = 12;/);
  assert.match(demo, /number: n\+\+,/);
  assert.doesNotMatch(demo, /bits\.push\(String\(task\.number\)\)/);
  assert.match(demo, /data-copy-task=/);
  assert.match(demo, /T\$\{task\.number\}/);
  assert.doesNotMatch(demo, /#\$\{task\.number\}/);
});

test("docs and demo describe the wide stage slider and threshold", async () => {
  const board = await read("../src/content/docs/docs/board.md");
  const keys = await read("../src/content/docs/docs/keys.md");
  const taskPage = await read("../src/content/docs/docs/task-page.md");
  const demo = await read("../public/board-demo.js");
  const styles = await read("../src/styles/landing.css");

  assert.match(board, /110 usable columns/);
  assert.match(board, /four-stage slider/);
  assert.match(board, /\| G \| dim rail, 32 columns \| task page \| task \|/);
  assert.doesNotMatch(board, /bordered|cyan/);
  assert.match(keys, /Wide stage slider/);
  assert.match(keys, /\| F \| nothing \| → G \|/);
  assert.doesNotMatch(keys, /cyan/);
  assert.match(taskPage, /110 usable columns/);
  assert.match(taskPage, /▸ T12 title … started · tsk/);
  assert.doesNotMatch(taskPage, /bordered panel/);
  assert.match(demo, /const WIDE_SPLIT_MIN_COLUMNS = 110;/);
  assert.match(demo, /stage: "board",/);
  assert.match(demo, /function stageRight\(\)/);
  assert.match(demo, /function stageLeft\(\)/);
  assert.match(demo, /state\.stageOrigin = state\.stage;/);
  assert.match(demo, /class="tsk-wide-split is-rail"/);
  assert.match(demo, /tsk-rule-column/);
  assert.match(demo, /board ▸ task    → task · ← close · enter open/);
  assert.match(styles, /\.tsk-wide-split\.is-split \{\s*grid-template-columns: minmax\(0, 2fr\) 1px minmax\(0, 3fr\);/);
  assert.match(styles, /\.tsk-wide-split\.is-rail \{\s*grid-template-columns: 32ch 1px/);
});

test("demo keeps project navigation, attribution, and search contracts", async () => {
  const demo = await read("../public/board-demo.js");
  assert.match(demo, /selectedProject: "tsk",/);
  assert.match(demo, /need: open/);
  assert.doesNotMatch(demo, /!t\.project && \(t\.status === "blocked" \|\| t\.status === "review"\)/);
  assert.match(demo, /state\.selectedProject = name;/);
  assert.match(demo, /const text = tab === "project" \? state\.selectedProject : label/);
  assert.match(demo, /state\.tasks\.filter\(\(t\) => t\.project && !t\.archived\)/);
  assert.match(demo, /if \(a === "tsk"\) return -1/);
  assert.match(demo, /id: `project:\${name}`/);
  assert.match(demo, /id: "nav:archived"/);
  assert.match(demo, /const row = selectedRow\(\);/);
  assert.match(demo, /row\?\.kind === "project"/);
  assert.match(demo, /selectedRow\(\)\?\.kind !== "task"/);
  assert.match(demo, /id="tsk-project-search"/);
  assert.ok(demo.includes('e.key === "/"'));
  assert.match(demo, /state\.projectQuery/);
  assert.match(demo, /data-project-row=/);
});

test("demo matches the quick-add, peek, and group-toggle contracts", async () => {
  const demo = await read("../public/board-demo.js");
  assert.match(demo, /if \(e\.key === "Enter" && !e\.ctrlKey && !e\.altKey && !e\.metaKey\)/);
  assert.match(demo, /saveDraft\(e\.shiftKey\)/);
  assert.match(demo, /enter save · tab details · esc close/);
  assert.match(demo, /z or D drawer \(app: d\)/);
  assert.match(demo, /e\.key === "z" \|\| e\.key === "D"/);
  assert.doesNotMatch(demo, /saveDraft\(e\.ctrlKey \|\| e\.metaKey\)/);
  assert.match(demo, /return notes\.split\(\/\\n\/\)\.slice\(0, 5\);/);
  assert.doesNotMatch(demo, /thread #\$\{task\.thread\}|scope \$\{projectName\(task\)\}|created \$\{age\(/);
  assert.match(demo, /<div class="tsk-peek dim">\$\{indent\}    └<\/div>/);
  assert.match(demo, /function toggleAllGroups\(\)/);
  assert.match(demo, /id: "groups", label: "toggle groups"/);
  assert.match(demo, /if \(e\.key === "g" && !e\.altKey && !e\.ctrlKey && !e\.metaKey\)/);
});

test("the saved theme survives a visit to the docs", async () => {
  const config = await read("../astro.config.mjs");
  const init = config.indexOf("var k='tsk-theme'");
  const loader = config.indexOf("attrs: { src: '/theme.js' }");
  assert.ok(init > 0 && loader > init, "the inline theme init must run before theme.js");
  const theme = await read("../public/theme.js");
  assert.match(theme, /saved = localStorage\.getItem\(KEY\)/);
});

test("docs paint keys as keycaps and leave flags as code", async () => {
  const { isKeyName } = await import("../src/plugins/rehype-kbd.mjs");
  for (const key of ["ctrl+s", "Shift+Enter", "Enter", "Esc", "→", "j", "P", "+", ":", "?"]) {
    assert.ok(isKeyName(key), `${key} is a key`);
  }
  for (const code of ["--json", "-", "tsk add", "~/.tsk", "T30", "T", "i", "n", "global", "ctrl+"]) {
    assert.ok(!isKeyName(code), `${code} is not a key`);
  }
});

test("docs open on the two-party board and let agents set status", async () => {
  const overview = await read("../src/content/docs/docs/index.mdx");
  assert.match(overview, /a task board for you and your agents/);
  assert.match(overview, /tsk status/);
  assert.doesNotMatch(overview, /Done is a human verb/);
  const cli = await read("../src/content/docs/docs/cli.md");
  assert.match(cli, /## For agents/);
  assert.match(cli, /tsk status <task> done/);
});

test("docs keep the phone layout until the right TOC fits", async () => {
  const css = await read("../src/styles/starlight.css");
  assert.match(css, /@media \(min-width: 50rem\) and \(max-width: 71\.99rem\)/);
  const links = await read("../src/components/DocsLinks.astro");
  assert.doesNotMatch(links, /install/);
});

test("attribution is peek-only in demo and static anatomy", async () => {
  const demo = await read("../public/board-demo.js");
  const page = await read("../src/pages/index.astro");
  const styles = await read("../src/styles/landing.css");
  const guide = await read("../src/content/docs/docs/board.md");
  assert.ok(demo.includes('state.peekId === task.id && metaFor(task) ? `<div class="tsk-attribution dim">'));
  assert.ok(demo.includes('└─ ${esc(metaFor(task))}'));
  assert.doesNotMatch(demo, /<span class="meta">/);
  assert.match(styles, /\.tsk-attribution\s*\{[^}]*white-space: pre-wrap;[^}]*overflow-wrap: anywhere;/);
  assert.match(styles, /\.tsk-row-main\s*\{[^}]*padding-right: 2ch;/);
  assert.doesNotMatch(page, /class="r meta"/);
  assert.match(page, /└─ tsk/);
  assert.doesNotMatch(guide, /section headers, row meta/);
});
