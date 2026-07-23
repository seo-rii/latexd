import { expect, test } from "@playwright/test";

test("static viewer compiles and updates a document in WebAssembly", async ({ context, page }) => {
  await context.addCookies([{
    name: "dev_bypass_waf",
    value: "seorii_bypass_token_is_this",
    url: "http://127.0.0.1:4390"
  }]);
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  await page.goto("./", { waitUntil: "domcontentloaded" });
  await expect(page.getByText("local WASM compiler")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("latexd in WebAssembly", { exact: true })).toBeVisible();
  await expect(page.locator(".browser-page")).toHaveCount(1);
  const pdfLink = page.getByRole("link", { name: "Download PDF" });
  await expect(pdfLink).toBeVisible();
  const pdfHeader = await pdfLink.evaluate(async (link) => {
    const response = await fetch((link as HTMLAnchorElement).href);
    return new TextDecoder().decode((await response.arrayBuffer()).slice(0, 8));
  });
  expect(pdfHeader).toContain("%PDF-");

  const editor = page.getByPlaceholder("Type LaTeX here…");
  const source = await editor.inputValue();
  await editor.fill(source.replace("Try it", "Edited in browser"));

  await expect(page.getByText(/Edited in browser/)).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(".editor-status p")).toContainText("Compiled locally");
  await expect(page.locator(".studio-hero__chips")).toContainText("last build ok");
  expect(pageErrors).toEqual([]);
});
