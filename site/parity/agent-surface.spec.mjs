import { test, expect } from "@playwright/test";

test.use({
  permissions: ["clipboard-read", "clipboard-write"],
});

test("docs_cli_shows_markdown_link_and_copy_for_agent_writes_read_url", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.goto("http://127.0.0.1:4180/docs/cli/");
  const row = page.locator(".docs-agent-row");
  await expect(row).toContainText("markdown");
  await expect(row).toContainText("copy for agent");
  await expect(row.locator("a", { hasText: "markdown" })).toHaveAttribute(
    "href",
    "/docs/cli.md",
  );
  await row.getByRole("button", { name: "copy for agent" }).click();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toBe("Read https://gettsk.sh/docs/cli.md and follow it.");
  const colors = await row.evaluate((el) => {
    const probe = document.createElement("span");
    probe.style.color = "var(--sl-color-gray-3)";
    document.body.appendChild(probe);
    const expected = getComputedStyle(probe).color;
    probe.remove();
    return { actual: getComputedStyle(el).color, expected };
  });
  expect(colors.actual).toBe(colors.expected);
  expect(colors.actual).not.toMatch(/#/);
});

const BOOTSTRAP = [
  "Use tsk for my task board. Run `tsk guide` and follow it;",
  "if tsk is not installed, read https://gettsk.sh/docs/agents/",
  "and ask me before installing.",
].join("\n");

test("landing_agents_band_after_cli_copies_bootstrap_and_stays_on_palette", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.goto("http://127.0.0.1:4180/");
  const ids = await page.evaluate(() =>
    [...document.querySelectorAll("section[id]")].map((section) => section.id),
  );
  expect(ids.indexOf("cli")).toBeGreaterThanOrEqual(0);
  expect(ids.indexOf("agents")).toBeGreaterThan(ids.indexOf("cli"));
  expect(ids.indexOf("install")).toBeGreaterThan(ids.indexOf("agents"));
  const band = page.locator("#agents");
  await expect(band.getByRole("heading", { name: "Hand it to your agent." })).toBeVisible();
  await expect(band.locator(".index")).toContainText("06");
  const paletteCheck = await page.evaluate(() => {
    const used = new Set();
    document.querySelectorAll("body *").forEach((el) => {
      if (el.closest("#agents")) return;
      const style = getComputedStyle(el);
      used.add(style.color);
      used.add(style.backgroundColor);
    });
    const outsiders = [];
    document.querySelectorAll("#agents, #agents *").forEach((el) => {
      const style = getComputedStyle(el);
      for (const value of [style.color, style.backgroundColor]) {
        if (!used.has(value)) outsiders.push(value);
      }
    });
    return outsiders;
  });
  expect(paletteCheck).toEqual([]);
  await band.locator("[data-copy]").click();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toBe(BOOTSTRAP);
});
