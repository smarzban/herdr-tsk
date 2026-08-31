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

test("demo rows paint bare task numbers", async () => {
  const demo = await read("../public/board-demo.js");
  assert.match(demo, /let n = 12;/);
  assert.match(demo, /number: n\+\+,/);
  assert.match(demo, /bits\.push\(String\(task\.number\)\)/);
  assert.doesNotMatch(demo, /#\$\{task\.number\}/);
  assert.doesNotMatch(demo, /T\$\{task\.number\}/);
});
