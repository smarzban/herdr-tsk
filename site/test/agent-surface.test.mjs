import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile, readdir } from "node:fs/promises";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

import {
  formatTwin,
  htmlUrlForSlug,
  slugFromDocsId,
  sourceFileName,
  stripFrontmatter,
  twinUrlForSlug,
} from "../src/lib/agent-docs.mjs";

const siteRoot = dirname(fileURLToPath(new URL("../package.json", import.meta.url)));
const repoRoot = dirname(siteRoot);
const docsDir = join(siteRoot, "src/content/docs/docs");
const distDir = join(siteRoot, "dist");

const read = (path) => readFile(path, "utf8");

function ensureDist() {
  if (existsSync(join(distDir, "index.html"))) return;
  const result = spawnSync("npm", ["run", "build"], {
    cwd: siteRoot,
    encoding: "utf8",
    stdio: "pipe",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
}

async function docsFiles() {
  const names = await readdir(docsDir);
  return names.filter((name) => name.endsWith(".md") || name.endsWith(".mdx"));
}

test("markdown_twin_formatter_prefixes_title_html_source_and_updated", () => {
  const rendered = formatTwin({
    title: "CLI",
    slug: "cli",
    fileName: "cli.md",
    updatedIso: "2026-09-09T00:00:00.000Z",
    body: "The same store backs the board.\n",
  });
  assert.equal(
    rendered,
    [
      "# CLI",
      "",
      "- html: https://gettsk.sh/docs/cli/",
      "- source: https://github.com/smarzban/herdr-tsk/blob/main/site/src/content/docs/docs/cli.md",
      "- updated: 2026-09-09T00:00:00.000Z",
      "",
      "The same store backs the board.",
      "",
    ].join("\n"),
  );
  assert.equal(htmlUrlForSlug("index"), "https://gettsk.sh/docs/");
  assert.equal(twinUrlForSlug("index"), "https://gettsk.sh/docs/index.md");
  assert.equal(slugFromDocsId("docs/cli"), "cli");
  assert.equal(slugFromDocsId("docs/index"), "index");
  assert.equal(sourceFileName("src/content/docs/docs/index.mdx"), "index.mdx");
});

test("dist_has_a_markdown_twin_for_every_docs_entry_with_matching_body", async () => {
  ensureDist();
  const files = await docsFiles();
  assert.ok(files.length >= 8, "docs collection should have the current pages");
  for (const fileName of files) {
    const slug = fileName.replace(/\.(md|mdx)$/, "");
    const twinPath = join(distDir, "docs", `${slug}.md`);
    assert.equal(existsSync(twinPath), true, `missing twin ${twinPath}`);
    const twin = await read(twinPath);
    const source = await read(join(docsDir, fileName));
    const body = stripFrontmatter(source).replace(/\s+$/, "");
    const afterHeader = twin.replace(/^# .*\n\n(?:- .*\n){3}\n/, "").replace(/\s+$/, "");
    assert.equal(afterHeader, body, `${fileName} twin body mismatch`);
    const title = source.match(/^title:\s*(.+)$/m)?.[1]?.replace(/^"|"$/g, "");
    assert.match(twin, new RegExp(`^# ${title}\\n`));
    assert.match(twin, /- html: https:\/\/gettsk\.sh\/docs\//);
    assert.match(
      twin,
      new RegExp(
        `- source: https://github.com/smarzban/herdr-tsk/blob/main/site/src/content/docs/docs/${fileName}`,
      ),
    );
    assert.match(twin, /- updated: \d{4}-\d{2}-\d{2}T/);
  }
});

test("sitemap_xml_contains_no_md_urls", async () => {
  ensureDist();
  const names = await readdir(distDir);
  const sitemaps = names.filter((name) => name.startsWith("sitemap") && name.endsWith(".xml"));
  assert.ok(sitemaps.length > 0, "expected a built sitemap");
  for (const name of sitemaps) {
    const xml = await read(join(distDir, name));
    assert.doesNotMatch(xml, /\.md</);
    assert.doesNotMatch(xml, /\.md</);
    assert.doesNotMatch(xml, /docs\/[^<]+\.md/);
  }
});

test("docs_html_has_rel_alternate_to_the_twin_once_and_landing_has_none", async () => {
  ensureDist();
  const files = await docsFiles();
  for (const fileName of files) {
    const slug = fileName.replace(/\.(md|mdx)$/, "");
    const htmlPath =
      slug === "index"
        ? join(distDir, "docs", "index.html")
        : join(distDir, "docs", slug, "index.html");
    const html = await read(htmlPath);
    const href = twinUrlForSlug(slug);
    const tag = `<link rel="alternate" type="text/markdown" href="${href}">`;
    const matches = html.split(tag).length - 1;
    assert.equal(matches, 1, `${slug} should contain the alternate link once`);
  }
  const landing = await read(join(distDir, "index.html"));
  assert.doesNotMatch(landing, /rel="alternate" type="text\/markdown"/);
});
