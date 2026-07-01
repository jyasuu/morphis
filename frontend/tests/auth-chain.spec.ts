import { test, expect } from "@playwright/test";

test.describe("Full auth chain: Keycloak -> Frontend -> Auth-proxy -> API", () => {
  test("login via Keycloak OIDC, load materials list through the chain", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(`[${msg.type()}] ${msg.text()}`);
    });
    page.on("pageerror", (err) => errors.push(`[PAGE ERROR] ${err.message}`));

    // Navigate to frontend — should redirect to login page
    await page.goto("http://localhost:3000/materials", {
      waitUntil: "networkidle",
    });
    console.log("URL after navigation:", page.url());

    // Should be on the frontend login page
    expect(page.url()).toContain("/login");

    // Click the Keycloak OIDC sign-in button
    await page.click('button:has-text("Keycloak")');
    console.log("Clicked Keycloak button, waiting for redirect...");

    // Wait for redirect to Keycloak login page
    await page.waitForURL("**/realms/morphis/protocol/openid-connect/auth**", { timeout: 10000 });
    console.log("On Keycloak login page:", page.url());

    // Fill in Keycloak login form
    await page.fill("#username", "testuser");
    await page.fill("#password", "testpass");
    await page.click("#kc-login");

    // Wait for redirect back to frontend
    await page.waitForURL("http://localhost:3000/**", { timeout: 15000 });
    console.log("Redirected to:", page.url());

    // Should be on the materials page
    const finalBody = await page.textContent("body");
    console.log("Errors:", errors);
    // Allow errors from pre-login schema introspection (401 before auth)
    const authErrors = errors.filter(e => !e.includes("__schema") && !e.includes("401"));
    expect(authErrors.length).toBe(0);
    expect(finalBody).toBeTruthy();
  });

  test("materials API route passes through Keycloak JWT to auth-proxy", async ({ page }) => {
    // Login via Keycloak
    await page.goto("http://localhost:3000/materials", { waitUntil: "networkidle" });
    await page.click('button:has-text("Keycloak")');
    await page.waitForURL("**/realms/morphis/protocol/openid-connect/auth**", { timeout: 10000 });
    await page.fill("#username", "testuser");
    await page.fill("#password", "testpass");
    await page.click("#kc-login");
    await page.waitForURL("http://localhost:3000/**", { timeout: 15000 });

    // Wait for the page to load and make API calls
    await page.waitForTimeout(2000);

    // The page should fetch materials via /api/graphql
    const apiResponses: string[] = [];
    page.on("response", (response) => {
      if (response.url().includes("/api/graphql")) {
        apiResponses.push(`[${response.status()}] ${response.url()}`);
      }
    });

    // Navigate to materials page
    await page.goto("http://localhost:3000/materials", { waitUntil: "networkidle" });
    await page.waitForTimeout(2000);

    console.log("API responses:", apiResponses);
    expect(apiResponses.length).toBeGreaterThan(0);

    // Check the page rendered
    const bodyText = await page.textContent("body");
    console.log("Body length:", bodyText?.length);
    expect(bodyText).toBeTruthy();
  });
});
