import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { parseCapture } from "../public/capture.js";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("Vercel ignores unchanged files relative to the site root", async () => {
  const config = JSON.parse(await read("../vercel.json"));
  assert.equal(config.ignoreCommand, "git diff --quiet HEAD^ HEAD -- .");
});

test("demo capture keeps an absolute project path verbatim", () => {
  assert.deepEqual(parseCapture("Ship it !p /Users/saeed/Workspace/herdr-tasks !t Site-Docs", null), {
    title: "Ship it",
    project: "/Users/saeed/Workspace/herdr-tasks",
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

test("docs and demo describe wide surface focus and threshold", async () => {
  const board = await read("../src/content/docs/docs/board.md");
  const keys = await read("../src/content/docs/docs/keys.md");
  const taskPage = await read("../src/content/docs/docs/task-page.md");
  const demo = await read("../public/board-demo.js");
  const styles = await read("../src/styles/landing.css");

  assert.match(board, /110 usable columns/);
  assert.match(board, /`Enter` or `→`[\s\S]{0,40}task focus/);
  assert.match(board, /`Esc` or `←`[\s\S]{0,40}board focus/);
  assert.match(keys, /wide split/i);
  assert.match(taskPage, /110 usable columns/);
  assert.match(demo, /const WIDE_SPLIT_MIN_COLUMNS = 110;/);
  assert.match(demo, /surfaceFocus: "board"/);
  assert.match(demo, /state\.surfaceFocus = "task"/);
  assert.match(demo, /state\.surfaceFocus = "board"/);
  assert.match(styles, /\.tsk-wide-split/);
  assert.doesNotMatch(demo, /data-panel-title="board"/);
  assert.match(demo, /data-panel-title="T\$\{task\.number\} · task"/);
  assert.doesNotMatch(demo, /tsk-wide-divider/);
  assert.match(styles, /grid-template-columns: minmax\(0, 1fr\) minmax\(0, 1fr\)/);
  assert.match(styles, /\.tsk-panel\.is-focused-surface[\s\S]{0,80}var\(--terminal-focus\)/);
});

test("demo matches the quick-add, peek, and group-toggle contracts", async () => {
  const demo = await read("../public/board-demo.js");
  assert.match(demo, /if \(e\.key === "Enter" && !e\.ctrlKey && !e\.altKey && !e\.metaKey\)/);
  assert.match(demo, /saveDraft\(e\.shiftKey\)/);
  assert.match(demo, /enter save · shift\+enter stay/);
  assert.doesNotMatch(demo, /saveDraft\(e\.ctrlKey \|\| e\.metaKey\)/);
  assert.match(demo, /return notes\.split\(\/\\n\/\)\.slice\(0, 5\);/);
  assert.doesNotMatch(demo, /thread #\$\{task\.thread\}|scope \$\{projectName\(task\)\}|created \$\{age\(/);
  assert.match(demo, /<div class="tsk-peek dim">\$\{indent\}    └<\/div>/);
  assert.match(demo, /function toggleAllGroups\(\)/);
  assert.match(demo, /id: "groups", label: "toggle groups"/);
  assert.match(demo, /if \(e\.key === "g" && !e\.altKey && !e\.ctrlKey && !e\.metaKey\)/);
});
