import { readReference } from "./reference.mjs";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { test, expect } from "@playwright/test";
test.beforeAll(async ({ browser }) => {
  const root = new URL("../../", import.meta.url);
  const files = [
    "site/public/board-demo.js",
    "site/public/board-wrap.js",
    "site/public/task-steps.js",
    "site/public/landing.js",
    "site/src/styles/landing.css",
    "tests/fixtures/demo-parity/store.json",
  ];
  const hashes = Object.fromEntries(
    await Promise.all(
      files.map(async (path) => [
        path,
        createHash("sha256")
          .update(await readFile(new URL(path, root)))
          .digest("hex"),
      ]),
    ),
  );
  await writeFile(
    new URL("../parity-reference/browser-provenance.json", import.meta.url),
    JSON.stringify(
      {
        kind: "actual Chrome screenshots cropped to board root; not approved baselines",
        browser: browser.version(),
        head: execFileSync("git", ["rev-parse", "HEAD"], {
          encoding: "utf8",
        }).trim(),
        diff: execFileSync("git", ["diff", "--stat"], { encoding: "utf8" }),
        hashes,
      },
      null,
      2,
    ),
  );
});
const id = (n) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
const row = (page, n) => page.locator(`[data-task="${id(n)}"]`);
async function open(page, width) {
  page.on("pageerror", (error) => console.error(error));
  await page.goto("/");
  await page
    .locator("#tsk-demo")
    .evaluate((el, w) => (el.style.width = `${w}ch`), width);
  await expect(row(page, 12)).toBeVisible();
  await page.locator("#board-demo").focus();
}
async function capture(page, testInfo, name) {
  await page.locator("#tsk-demo").screenshot({
    path: testInfo.outputPath(`${name}.png`),
    animations: "disabled",
  });
}
for (const width of [40, 78, 109, 110]) {
  test(`desk start open back and wrapping at ${width} columns`, async ({
    page,
  }, info) => {
    await open(page, width);
    await expect(page.locator(".tsk-attribution")).toHaveCount(0);
    await expect(page.locator(".tsk-sec")).not.toContainText(["IN MOTION"]);
    await capture(page, info, "initial");
    await page.keyboard.press("ArrowDown");
    await expect(row(page, 13)).toHaveClass(/is-sel/);
    const lines = await row(page, 13)
      .locator(".tsk-title-line")
      .allTextContents();
    expect(lines.length).toBeGreaterThan(1);
    const appLines = await readReference(`title-${width}`);
    expect(lines.map((line) => line.trimEnd())).toEqual(appLines);
    expect(lines.join("")).toBe(
      "Read every word of this long title before starting the task so the continuation must align beneath its title and retain the final boundary marker END",
    );
    const bounds = await row(page, 13).evaluate((el) => {
      const r = el.getBoundingClientRect();
      return [...el.querySelectorAll(".tsk-title-line")].map((line) => {
        const range = document.createRange();
        range.selectNodeContents(line);
        const b = range.getBoundingClientRect();
        return { left: b.left - r.left, right: r.right - b.right };
      });
    });
    expect(
      Math.max(...bounds.map((b) => b.left)) -
        Math.min(...bounds.map((b) => b.left)),
    ).toBeLessThan(1);
    expect(Math.min(...bounds.map((b) => b.right))).toBeGreaterThanOrEqual(15);
    await page.keyboard.press("s");
    await expect(row(page, 13).locator(".tsk-row-glyph")).toHaveText("●");
    await expect(row(page, 13)).toHaveClass(/is-sel/);
    await capture(page, info, "started");
    await page.keyboard.press("Enter");
    await expect(page.locator(".tsk-task-column")).toHaveAttribute(
      "data-status",
      "started",
    );
    await expect(page.locator(".tsk-page-notes")).toHaveText(
      "A plain note for the first matched task-page flow.",
    );
    await expect(page.locator(".tsk-list")).toHaveCount(0);
    await expect(page.locator(".tsk-foot")).toHaveCount(1);
    await capture(page, info, "page");
    await page.keyboard.press("Escape");
    await expect(row(page, 13)).toHaveClass(/is-sel/);
    await capture(page, info, "back");
  });
}
for (const width of [40, 78, 109])
  test(`peek attribution at ${width}`, async ({ page }, info) => {
    await open(page, width);
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".tsk-attribution")).toHaveText(
      "    └─ tsk-parity",
    );
    await capture(page, info, "project-peek");
    await page.keyboard.press("Escape");
    await expect(page.locator(".tsk-attribution")).toHaveCount(0);
    await page.keyboard.press("2");
    await expect(page.locator(".tsk-tab")).toHaveCount(3);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".tsk-attribution")).toHaveText(
      "    └─ #release",
    );
    await capture(page, info, "thread-peek");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".tsk-attribution")).toHaveCount(0);
    await expect(page.locator(".tsk-peek")).toContainText(["no notes yet"]);
    await capture(page, info, "unlabeled-peek");
  });
test("110-column boundary and rail mouse return", async ({ page }, info) => {
  await open(page, 109);
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".tsk-attribution")).toBeVisible();
  await page.locator("#tsk-demo").evaluate((el) => (el.style.width = "110ch"));
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".tsk-wide-split.is-split")).toBeVisible();
  await expect(page.locator(".tsk-peek")).toHaveCount(0);
  await capture(page, info, "split-110");
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".tsk-wide-split.is-rail")).toBeVisible();
  await row(page, 13).click();
  await expect(page.locator(".tsk-wide-split.is-split")).toBeVisible();
  await expect(row(page, 13)).toHaveClass(/is-sel/);
});

for (const width of [110, 130]) {
  test(`row double-click survives reflow at ${width} columns`, async ({
    page,
  }) => {
    await open(page, width);
    const result = await row(page, 13).evaluate((el) => {
      const rect = el.getBoundingClientRect();
      const x = Math.min(rect.right - 4, window.innerWidth - 8);
      const y = rect.top + 4;
      const click = () =>
        document.elementFromPoint(x, y).dispatchEvent(
          new MouseEvent("click", {
            bubbles: true,
            clientX: x,
            clientY: y,
            detail: 1,
          }),
        );
      click();
      const split = !!document.querySelector(".tsk-wide-split.is-split");
      click();
      return { split, full: !document.querySelector(".tsk-list") };
    });
    expect(result).toEqual({ split: true, full: true });
    await expect(page.locator(".tsk-task-column")).toContainText("END");
  });
}

test("landing column readout excludes board padding", async ({ page }) => {
  await open(page, 78);
  await page.evaluate(() => {
    const frame = document.getElementById("board-demo");
    const split = document.createElement("div");
    split.dataset.split = "";
    split.style.width = "1200px";
    frame.before(split);
    split.append(frame);
    const readout = document.createElement("span");
    readout.dataset.cols = "";
    split.append(readout);
    document.getElementById("tsk-demo").style.padding = "20px";
  });
  await page.addScriptTag({ url: "/public/landing.js" });
  await expect(page.locator("[data-cols] b")).toHaveText("78");
});
