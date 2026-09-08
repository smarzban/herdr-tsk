import { test, expect } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

test("agent transcript matches the current CLI output contract", async ({
  page,
}) => {
  const scratch = mkdtempSync(join(tmpdir(), "tsk-cli-copy-"));
  const project = join(scratch, "launchpad");
  mkdirSync(project);
  const binary = resolve("../target/release/tsk");
  const env = {
    ...process.env,
    TSK_STATE_DIR: join(scratch, "state"),
    TSK_CONFIG_DIR: join(scratch, "config"),
  };
  try {
    await page.goto("http://127.0.0.1:4180/");
    const shown = JSON.parse(
      await page.locator("[data-cli-add-output]").textContent(),
    );
    const actual = JSON.parse(
      execFileSync(
        binary,
        [
          "add",
          "-t",
          shown.title,
          "-p",
          "launchpad",
          "--thread",
          "api",
          "--json",
        ],
        { env, encoding: "utf8" },
      ),
    );
    expect(Object.keys(shown).sort()).toEqual(Object.keys(actual).sort());
    expect(shown.outcome).toBe(actual.outcome);
    expect(shown.title).toBe(actual.title);
    expect(shown.project).toBe(actual.project);
    expect(shown.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(typeof shown.number).toBe(typeof actual.number);
    const stepText = "Reproduce duplicate delivery";
    const output = execFileSync(
      binary,
      ["steps", `T${actual.number}`, "add", stepText],
      { env, encoding: "utf8" },
    ).trim();
    const shownStep = (
      await page.locator("[data-cli-step-output]").textContent()
    ).trim();
    expect(output.replace(/^added \S+ /, "added <id> ")).toBe(
      shownStep.replace(/^added \S+ /, "added <id> "),
    );
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
