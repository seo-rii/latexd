import { expect, test } from "@playwright/test";

test("static viewer previews the generated PDF and retains the last good build", async ({ context, page }) => {
  await context.addCookies([{
    name: "dev_bypass_waf",
    value: "seorii_bypass_token_is_this",
    url: "http://127.0.0.1:4390"
  }]);
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  await page.goto("./", { waitUntil: "domcontentloaded" });
  await expect(page.getByText("local WASM compiler")).toBeVisible({ timeout: 15_000 });
  const pdfPreview = page.getByTitle("Compiled PDF preview");
  await expect(pdfPreview).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(".browser-page")).toHaveCount(0);
  await expect(page.getByText("latexd in WebAssembly", { exact: true })).toHaveCount(0);
  const pdfLink = page.getByRole("link", { name: "Download PDF" });
  await expect(pdfLink).toBeVisible();
  const initialPdfUrl = await pdfLink.getAttribute("href");
  expect(initialPdfUrl).toBeTruthy();
  await expect(pdfPreview).toHaveAttribute("src", initialPdfUrl!);
  const pdfHeader = await pdfPreview.evaluate(async (frame) => {
    const response = await fetch((frame as HTMLIFrameElement).src);
    return new TextDecoder().decode((await response.arrayBuffer()).slice(0, 8));
  });
  expect(pdfHeader).toContain("%PDF-");

  const editor = page.getByPlaceholder("Type LaTeX here…");
  const source = await editor.inputValue();
  await editor.fill(source.replace("Try it", "Edited in browser"));

  await expect(page.locator(".editor-status p")).toContainText("Compiled locally");
  await expect(page.locator(".studio-hero__chips")).toContainText("last build ok");
  await expect.poll(() => pdfPreview.getAttribute("src")).not.toBe(initialPdfUrl);
  const updatedPdfUrl = await pdfPreview.getAttribute("src");
  expect(updatedPdfUrl).toBeTruthy();
  await expect(pdfLink).toHaveAttribute("href", updatedPdfUrl!);

  await editor.fill("\\errmessage{expected browser compile failure}");

  await expect(page.locator(".editor-status__badge")).toHaveText("error", { timeout: 15_000 });
  await expect(pdfPreview).toHaveAttribute("src", updatedPdfUrl!);
  await expect(pdfLink).toHaveAttribute("href", updatedPdfUrl!);
  expect(pageErrors).toEqual([]);
});
