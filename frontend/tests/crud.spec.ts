import { test, expect } from "@playwright/test";

test.describe("Full CRUD flow", () => {
  test("create, view, edit, and delete a material", async ({ page }) => {
    const logs: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") logs.push(`[${msg.type()}] ${msg.text()}`);
    });
    page.on("pageerror", (err) => logs.push(`[PAGE ERROR] ${err.message}`));

    const testMatNo = `E2E_${Date.now()}`;

    // 1. Navigate to /materials/new
    await page.goto("http://localhost:3000/materials/new", {
      waitUntil: "networkidle",
    });
    await page.waitForTimeout(1500);
    console.log("=== Navigate to new ===");
    console.log("URL:", page.url());
    console.log("Errors:", logs);
    logs.length = 0;

    // Check the page loaded
    const body = await page.textContent("body");
    expect(body).toContain("New materials");

    // 2. Fill in the create form
    // Find inputs by their labels
    await page.fill('input[name="mat_no"]', testMatNo);
    await page.fill('input[name="name"]', "E2E Test Material");
    await page.selectOption('select[name="status"]', "active");

    // 3. Submit
    await page.click('button[type="submit"]');
    await page.waitForTimeout(2000);

    // Check for errors
    console.log("=== After create submit ===");
    console.log("URL:", page.url());
    console.log("Errors:", logs);
    logs.length = 0;

    // Should redirect to /materials list
    expect(page.url()).toContain("/materials");

    // 4. Verify the new material appears in the list
    const listBody = await page.textContent("body");
    expect(listBody).toContain(testMatNo);
    expect(listBody).toContain("E2E Test Material");

    // 5. Click Edit on the new material
    await page.goto(
      `http://localhost:3000/materials/${encodeURIComponent(testMatNo)}`,
      { waitUntil: "networkidle" }
    );
    await page.waitForTimeout(1500);

    console.log("=== Navigate to edit ===");
    console.log("URL:", page.url());
    console.log("Errors:", logs);
    logs.length = 0;

    const editBody = await page.textContent("body");
    expect(editBody).toContain(testMatNo);

    // 6. Update the name
    await page.fill('input[name="name"]', "E2E Updated");
    await page.click('button[type="submit"]');
    await page.waitForTimeout(2000);

    console.log("=== After update submit ===");
    console.log("URL:", page.url());
    console.log("Errors:", logs);
    logs.length = 0;

    expect(page.url()).toContain("/materials");

    // 7. Verify the update in the list
    const updatedBody = await page.textContent("body");
    expect(updatedBody).toContain("E2E Updated");

    // 8. Delete the test material
    // Click Delete button for our test row
    await page.goto(`http://localhost:3000/materials`, {
      waitUntil: "networkidle",
    });
    await page.waitForTimeout(1500);

    // page.on('dialog') doesn't work great with confirm, let's use the API directly
    const deleteRes = await page.evaluate(async (matNo) => {
      const res = await fetch("http://localhost:4000/graphql", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          query: `mutation { deleteMaterials(id: "${matNo}") { mat_no } }`,
        }),
      });
      return res.json();
    }, testMatNo);

    console.log("=== Delete result ===");
    console.log(JSON.stringify(deleteRes));

    await page.reload({ waitUntil: "networkidle" });
    await page.waitForTimeout(1000);

    const finalBody = await page.textContent("body");
    expect(finalBody).not.toContain("E2E Updated");
  });
});
