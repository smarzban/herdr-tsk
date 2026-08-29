export function normalizeThread(raw) {
  const normalized = String(raw)
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);
  if (!normalized || !/^[a-z0-9]/.test(normalized)) return null;
  return normalized;
}

export function parseCapture(raw, fallbackProject) {
  const parts = raw.trim().split(/\s+/).filter(Boolean);
  let project = fallbackProject;
  let thread;
  const title = [];
  for (let i = 0; i < parts.length; i += 1) {
    const part = parts[i];
    if (part === "!p") {
      const argument = parts[i + 1];
      if (!argument || argument.startsWith("!")) {
        project = null;
      } else {
        i += 1;
        project = argument;
      }
      continue;
    }
    if (part === "!t") {
      const argument = parts[i + 1];
      if (!argument || argument.startsWith("!")) {
        thread = null;
      } else {
        i += 1;
        thread = normalizeThread(argument);
      }
      continue;
    }
    title.push(part);
  }
  return { title: title.join(" "), project, thread };
}
