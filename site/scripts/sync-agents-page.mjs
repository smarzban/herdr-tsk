#!/usr/bin/env node
// Generate the gitignored /docs/agents/ source from skills/tsk-cli/SKILL.md.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { DEFINITION_SENTENCE, stripFrontmatter } from "../src/lib/agent-docs.mjs";

const siteRoot = dirname(fileURLToPath(new URL("../package.json", import.meta.url)));
const repoRoot = dirname(siteRoot);
const destDir = join(siteRoot, "src/content/docs/docs");
const dest = join(destDir, "agents.md");
const skill = readFileSync(join(repoRoot, "skills/tsk-cli/SKILL.md"), "utf8");
const body = stripFrontmatter(skill).replace(/^\uFEFF/, "").replace(/\s+$/, "");

mkdirSync(destDir, { recursive: true });
writeFileSync(
  dest,
  [
    "---",
    "title: tsk for agents",
    "description: The agent workflow printed by tsk guide, including what to do if tsk is not installed.",
    "---",
    "",
    DEFINITION_SENTENCE,
    "",
    body,
    "",
  ].join("\n"),
);
