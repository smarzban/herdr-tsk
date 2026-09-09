// Serve the actual production build; keep the process owned by Playwright.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname } from "node:path";
const root = resolve(import.meta.dirname, "../dist");
const mime = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".json": "application/json",
  ".md": "text/markdown; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
};
createServer(async (req, res) => {
  try {
    let name = decodeURIComponent(
      new URL(req.url, "http://localhost").pathname,
    );
    if (name.endsWith("/")) name += "index.html";
    const path = resolve(root, "." + name);
    if (!path.startsWith(root + "/")) {
      res.writeHead(403).end();
      return;
    }
    const body = await readFile(path);
    res.setHeader(
      "Content-Type",
      mime[extname(path)] || "application/octet-stream",
    );
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(4180, "127.0.0.1");
