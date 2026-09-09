import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { getCollection } from "astro:content";

import {
  formatTwin,
  newestCommitIso,
  slugFromDocsId,
  sourceFileName,
  stripFrontmatter,
} from "../../lib/agent-docs.mjs";

export async function getStaticPaths() {
  const docs = await getCollection("docs");
  return docs.map((entry) => ({
    params: { slug: slugFromDocsId(entry.id) },
    props: { entry },
  }));
}

export async function GET({
  props,
}: {
  props: { entry: { id: string; filePath?: string; data: { title: string } } };
}) {
  const { entry } = props;
  if (!entry.filePath) {
    throw new Error(`docs entry ${entry.id} has no filePath`);
  }
  const source = await readFile(join(process.cwd(), entry.filePath), "utf8");
  const twin = formatTwin({
    title: entry.data.title,
    slug: slugFromDocsId(entry.id),
    fileName: sourceFileName(entry.filePath),
    updatedIso: newestCommitIso(entry.filePath),
    body: stripFrontmatter(source),
  });
  return new Response(twin, {
    headers: { "Content-Type": "text/markdown; charset=utf-8" },
  });
}
