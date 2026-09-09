import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { getCollection } from "astro:content";

import {
  formatTwin,
  newestCommitIso,
  SIDEBAR_SLUGS,
  slugFromDocsId,
  sourceFileName,
  stripFrontmatter,
} from "../lib/agent-docs.mjs";

export async function GET() {
  const docs = await getCollection("docs");
  const bySlug = new Map(docs.map((entry) => [slugFromDocsId(entry.id), entry]));
  const parts = [];
  for (const slug of SIDEBAR_SLUGS) {
    const entry = bySlug.get(slug);
    if (!entry?.filePath) continue;
    const source = await readFile(join(process.cwd(), entry.filePath), "utf8");
    parts.push(
      formatTwin({
        title: entry.data.title,
        slug,
        fileName: sourceFileName(entry.filePath),
        updatedIso: newestCommitIso(entry.filePath),
        body: stripFrontmatter(source),
      }).trimEnd(),
    );
  }
  return new Response(`${parts.join("\n\n")}\n`, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
}
