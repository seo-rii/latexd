import path from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

const testDirectory = path.dirname(fileURLToPath(import.meta.url));

test("static viewer renders display-list pages and retains PDF comparison output", async ({ context, page }) => {
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
  await expect(browserPreview).toHaveAttribute("data-build-revision", "1");
  await expect(browserPreview).toHaveAttribute("data-compile-mode", "one_shot");
  await expect(browserPreview).toHaveAttribute("data-page-ids", /^[0-9a-f]{64}$/);
  await expect(browserPreview).toHaveAttribute("data-page-hashes", /^[0-9a-f]{64}$/);
  await expect(browserPreview).toHaveAttribute("data-page-sizes", /^\d+(?:\.\d+)?x\d+(?:\.\d+)?$/);
  const displayPage = page.locator(".display-list-page");
  await expect(displayPage).toHaveCount(1);
  await expect(displayPage).toHaveAttribute("data-page-id", /^[0-9a-f]{64}$/);
  await expect(displayPage.locator("svg")).toHaveAttribute("viewBox", /^0 0 \d+(?:\.\d+)? \d+(?:\.\d+)?$/);
  await expect(displayPage.getByText("latexd in WebAssembly", { exact: true })).toBeVisible();
  await expect(displayPage.locator("svg")).toContainText("x^2");
  await expect(displayPage.locator('[data-text-rendering="css-fallback"]').first()).toBeVisible();
  await expect(pdfPreview).toHaveCount(0);
  await expect(page.locator(".browser-page")).toHaveCount(0);
  const pdfLink = page.getByRole("link", { name: "Download PDF" });
  await expect(pdfLink).toBeVisible();
  const initialPdfUrl = await pdfLink.getAttribute("href");
  const initialPageIds = await browserPreview.getAttribute("data-page-ids");
  expect(initialPdfUrl).toBeTruthy();
  expect(initialPageIds).toBeTruthy();
  await page.getByRole("button", { name: "PDF output" }).click();
  await expect(pdfPreview).toBeVisible();
  await expect(pdfPreview).toHaveAttribute("src", initialPdfUrl!);
  const pdfHeader = await pdfPreview.evaluate(async (frame) => {
    const response = await fetch((frame as HTMLIFrameElement).src);
    return new TextDecoder().decode((await response.arrayBuffer()).slice(0, 8));
  });
  expect(pdfHeader).toContain("%PDF-");
  await page.getByRole("button", { name: "Fast preview" }).click();
  await expect(displayPage).toBeVisible();
  await expect(pdfPreview).toHaveCount(0);

  const editor = page.getByPlaceholder("Type LaTeX here…");
  const source = await editor.inputValue();
  await editor.fill(source.replace("Try it", "Edited in browser"));

  await expect(page.locator(".editor-status p")).toContainText("Compiled locally");
  await expect(page.locator(".studio-hero__chips")).toContainText("last build ok");
  await expect(displayPage.getByText("Edited in browser", { exact: true })).toBeVisible();
  await expect(browserPreview).toHaveAttribute("data-build-revision", "2");
  await expect.poll(() => browserPreview.getAttribute("data-page-ids")).not.toBe(initialPageIds);
  const updatedPageIds = await browserPreview.getAttribute("data-page-ids");
  const updatedPdfUrl = await pdfLink.getAttribute("href");
  expect(updatedPageIds).toBeTruthy();
  expect(updatedPdfUrl).toBeTruthy();
  expect(updatedPdfUrl).not.toBe(initialPdfUrl);
  await expect(pdfLink).toHaveAttribute("href", updatedPdfUrl!);
  await page.getByRole("button", { name: "PDF output" }).click();
  await expect(pdfPreview).toHaveAttribute("src", updatedPdfUrl!);

  await editor.fill("\\errmessage{expected browser compile failure}");

  await expect(page.locator(".editor-status__badge")).toHaveText("error", { timeout: 15_000 });
  await expect(browserPreview).toHaveAttribute("data-build-revision", "2");
  await expect(browserPreview).toHaveAttribute("data-page-ids", updatedPageIds!);
  await expect(pdfPreview).toHaveAttribute("src", updatedPdfUrl!);
  await expect(pdfLink).toHaveAttribute("href", updatedPdfUrl!);
  expect(pageErrors).toEqual([]);
});

test("browser project images survive failed builds and release replaced URLs", async ({ context, page }) => {
  await context.addCookies([{
    name: "dev_bypass_waf",
    value: "seorii_bypass_token_is_this",
    url: "http://127.0.0.1:4390"
  }]);
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  await page.goto("./", { waitUntil: "domcontentloaded" });
  const browserPreview = page.locator('[aria-label="WebAssembly document preview"]');
  await expect(browserPreview).toHaveAttribute("data-build-revision", "1", { timeout: 15_000 });

  await page.locator('input[type="file"][webkitdirectory]').setInputFiles(
    path.join(testDirectory, "fixtures/browser-image-project")
  );

  await expect(page.locator(".editor-status p")).toContainText("Compiled locally", {
    timeout: 15_000
  });
  const image = page.locator(".display-list-page image");
  await expect(image).toHaveCount(1);
  const imageUrl = await image.getAttribute("href");
  expect(imageUrl).toMatch(/^blob:/);
  await expect.poll(() => page.evaluate(async (url) => {
    try {
      return (await fetch(url)).ok;
    } catch {
      return false;
    }
  }, imageUrl!)).toBe(true);

  const editor = page.getByPlaceholder("Type LaTeX here…");
  await editor.fill("\\errmessage{expected asset build failure}");
  await expect(page.locator(".editor-status__badge")).toHaveText("error", { timeout: 15_000 });
  await expect(image).toHaveAttribute("href", imageUrl!);
  await expect.poll(() => page.evaluate(async (url) => {
    try {
      return (await fetch(url)).ok;
    } catch {
      return false;
    }
  }, imageUrl!)).toBe(true);

  await editor.fill(String.raw`\documentclass{article}
\begin{document}
Image removed.
\end{document}`);
  await expect(page.locator(".editor-status p")).toContainText("Compiled locally", {
    timeout: 15_000
  });
  await expect(image).toHaveCount(0);
  await expect.poll(() => page.evaluate(async (url) => {
    try {
      return (await fetch(url)).ok;
    } catch {
      return false;
    }
  }, imageUrl!)).toBe(false);
  expect(pageErrors).toEqual([]);
});
