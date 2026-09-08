import { readFile } from "node:fs/promises";

export async function readReference(name) {
  const path = new URL(`../parity-reference/${name}.json`, import.meta.url);
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (error.code === "ENOENT") {
      throw new Error(
        `Missing parity reference ${name}.json. Run npm run parity:reference from site/ first.`,
      );
    }
    throw error;
  }
}
