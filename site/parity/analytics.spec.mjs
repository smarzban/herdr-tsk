import { test, expect } from "@playwright/test";

test("the standalone parity fixture does not load analytics", async ({
  page,
}) => {
  const errors = [];
  const analyticsRequests = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (request.url().includes("/_vercel/insights/")) {
      analyticsRequests.push(request.url());
    }
  });
  await page.goto("/");
  await expect(page.locator("[data-task]").first()).toBeVisible();
  await expect(page.locator("vercel-analytics")).toHaveCount(0);
  expect(analyticsRequests).toEqual([]);
  expect(errors).toEqual([]);
});

for (const path of ["/", "/docs/board/"]) {
  test(`production ${path} includes analytics once without page errors`, async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    // Vercel serves this endpoint in deployment. Never load its tracker in tests.
    await page.route("**/_vercel/insights/**", (route) => route.abort());
    const response = await page.goto(`http://127.0.0.1:4180${path}`);
    const html = await response.text();
    const head = html.slice(0, html.indexOf("</head>"));
    expect(head.match(/<vercel-analytics\b/g)).toHaveLength(1);
    await expect(page.locator("vercel-analytics")).toHaveCount(1);
    await expect(
      page.locator('head script[src="/_vercel/insights/script.js"]'),
    ).toHaveCount(1);
    await page.waitForLoadState("networkidle");
    expect(errors).toEqual([]);
  });
}
