import { test, expect } from "@playwright/test";

test.describe("Morphis Admin", () => {
  test("home page loads and shows entity picker", async ({ page }) => {
    // Capture console messages for debugging
    const consoleLogs: string[] = [];
    page.on("console", (msg) => consoleLogs.push(`[${msg.type()}] ${msg.text()}`));
    page.on("pageerror", (err) => consoleLogs.push(`[PAGE ERROR] ${err.message}`));

    await page.goto("http://localhost:3000", { waitUntil: "networkidle" });

    // Wait a bit for React to hydrate and fetches to complete
    await page.waitForTimeout(3000);

    console.log("=== Page title ===");
    console.log(await page.title());

    console.log("=== Console logs ===");
    for (const log of consoleLogs) {
      console.log(log);
    }

    // Check page content
    const bodyText = await page.textContent("body");
    console.log("=== Body text ===");
    console.log(bodyText?.substring(0, 2000));

    // Check if entities loaded
    const entityLinks = page.locator("a[href^='/']");
    const count = await entityLinks.count();
    console.log(`\n=== Entity links found: ${count} ===`);
    for (let i = 0; i < count; i++) {
      console.log(`  ${await entityLinks.nth(i).getAttribute("href")}`);
    }
  });

  test("can navigate to materials list", async ({ page }) => {
    const consoleLogs: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleLogs.push(`[${msg.type()}] ${msg.text()}`);
    });
    page.on("pageerror", (err) => consoleLogs.push(`[PAGE ERROR] ${err.message}`));

    await page.goto("http://localhost:3000/materials", { waitUntil: "networkidle" });
    await page.waitForTimeout(3000);

    console.log("=== Console errors ===");
    for (const log of consoleLogs) console.log(log);

    const bodyText = await page.textContent("body");
    console.log("=== Body text ===");
    console.log(bodyText?.substring(0, 2000));
  });
});
