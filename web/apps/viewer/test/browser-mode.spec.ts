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
  const browserPreview = page.locator('[aria-label="WebAssembly document preview"]');
  const pdfPreview = page.getByTitle("Compiled PDF preview");
  await expect(pdfPreview).toBeVisible({ timeout: 15_000 });
  await expect(browserPreview).toHaveAttribute("data-build-revision", "1");
  await expect(browserPreview).toHaveAttribute("data-compile-mode", "one_shot");
  await expect(browserPreview).toHaveAttribute("data-page-ids", /^[0-9a-f]{64}$/);
  await expect(browserPreview).toHaveAttribute("data-page-hashes", /^[0-9a-f]{64}$/);
  await expect(browserPreview).toHaveAttribute("data-page-sizes", /^\d+(?:\.\d+)?x\d+(?:\.\d+)?$/);
  await expect(page.locator(".browser-page")).toHaveCount(0);
  await expect(page.getByText("latexd in WebAssembly", { exact: true })).toHaveCount(0);
  const pdfLink = page.getByRole("link", { name: "Download PDF" });
  await expect(pdfLink).toBeVisible();
  const initialPdfUrl = await pdfLink.getAttribute("href");
  const initialPageIds = await browserPreview.getAttribute("data-page-ids");
  expect(initialPdfUrl).toBeTruthy();
  expect(initialPageIds).toBeTruthy();
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
  await expect(browserPreview).toHaveAttribute("data-build-revision", "2");
  await expect.poll(() => browserPreview.getAttribute("data-page-ids")).not.toBe(initialPageIds);
  await expect.poll(() => pdfPreview.getAttribute("src")).not.toBe(initialPdfUrl);
  const updatedPageIds = await browserPreview.getAttribute("data-page-ids");
  const updatedPdfUrl = await pdfPreview.getAttribute("src");
  expect(updatedPageIds).toBeTruthy();
  expect(updatedPdfUrl).toBeTruthy();
  await expect(pdfLink).toHaveAttribute("href", updatedPdfUrl!);

  await editor.fill("\\errmessage{expected browser compile failure}");

  await expect(page.locator(".editor-status__badge")).toHaveText("error", { timeout: 15_000 });
  await expect(browserPreview).toHaveAttribute("data-build-revision", "2");
  await expect(browserPreview).toHaveAttribute("data-page-ids", updatedPageIds!);
  await expect(pdfPreview).toHaveAttribute("src", updatedPdfUrl!);
  await expect(pdfLink).toHaveAttribute("href", updatedPdfUrl!);
  expect(pageErrors).toEqual([]);
});
