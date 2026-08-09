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
  const heading = displayPage.getByText("Try it", { exact: true });
  const editor = page.getByPlaceholder("Type LaTeX here…");
  const outlinedTitle = displayPage.getByRole("img", {
    name: "latexd in WebAssembly",
    exact: true
  });
  await expect(outlinedTitle).toBeVisible();
  await expect(outlinedTitle).toHaveAttribute("data-text-rendering", "positioned-outline");
  await heading.hover();
  await expect(page.locator("[data-browser-source-hover]")).toContainText("main.tex:");
  await heading.click();
  await expect(editor).toBeFocused();
  await expect.poll(() => editor.evaluate((input) => {
    const textarea = input as HTMLTextAreaElement;
    return textarea.value.slice(textarea.selectionStart, textarea.selectionEnd);
  })).toBe("Try it");
  await expect(displayPage.locator("svg")).toContainText("x^2");
  const fontBackedRun = displayPage.locator('[data-resolved-font-face="cmr10"]').first();
  await expect(fontBackedRun).toBeVisible();
  await expect(fontBackedRun).toHaveAttribute("data-font-content-hash", /^blake3:[0-9a-f]{64}$/);
  await expect(fontBackedRun).toHaveAttribute("data-positioned-glyph-count", /^[1-9]\d*$/);
  await expect(fontBackedRun).toHaveAttribute("data-text-rendering", "positioned-outline");
  await expect(fontBackedRun.locator("path")).not.toHaveCount(0);
  const cssFallbackCount = await displayPage.locator('[data-text-rendering="css-fallback"]').count();
  expect(cssFallbackCount).toBeGreaterThan(0);
  await expect(browserPreview).toContainText(`${cssFallbackCount} CSS text fallback(s)`);
  const initialPageWidth = (await displayPage.boundingBox())?.width ?? 0;
  const zoomIn = page.getByRole("button", { name: "Zoom in" });
  await zoomIn.click();
  await expect(browserPreview).toHaveAttribute("data-zoom-percent", "110");
  await expect.poll(async () => (await displayPage.boundingBox())?.width ?? 0)
    .toBeGreaterThan(initialPageWidth * 1.05);
  for (let step = 0; step < 9; step += 1) {
    await zoomIn.click();
  }
  await expect(browserPreview).toHaveAttribute("data-zoom-percent", "200");
  const zoomedPageBox = await displayPage.boundingBox();
  const previewBox = await browserPreview.boundingBox();
  expect(zoomedPageBox).not.toBeNull();
  expect(previewBox).not.toBeNull();
  expect(zoomedPageBox!.x).toBeGreaterThanOrEqual(previewBox!.x - 1);
  await expect.poll(() => browserPreview.evaluate((container) => (
    container.scrollWidth > container.clientWidth
  ))).toBe(true);
  await page.getByRole("button", { name: "Reset zoom" }).click();
  await expect(browserPreview).toHaveAttribute("data-zoom-percent", "100");
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
  const pdfText = await pdfPreview.evaluate(async (frame) => {
    const response = await fetch((frame as HTMLIFrameElement).src);
    return new TextDecoder().decode(await response.arrayBuffer());
  });
  expect(pdfText.slice(0, 8)).toContain("%PDF-");
  expect(pdfText).toContain("/BaseFont /CMR10");
  expect(pdfText).toContain("/FontFile");
  await page.getByRole("button", { name: "Fast preview" }).click();
  await expect(displayPage).toBeVisible();
  await expect(pdfPreview).toHaveCount(0);

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

test("unchanged display-list pages retain DOM identity and scroll position", async ({ context, page }) => {
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
  const editor = page.getByPlaceholder("Type LaTeX here…");
  const source = String.raw`\documentclass{article}
\begin{document}
Page one stable.
\newpage
Page two before.
\newpage
Page three stable.
\end{document}`;
  await editor.fill(source);
  await expect(page.locator(".editor-status p")).toContainText("Compiled locally", {
    timeout: 15_000
  });

  const pages = page.locator(".display-list-page");
  await expect(pages).toHaveCount(3);
  const originalIds = await pages.evaluateAll((nodes) => nodes.map((node, index) => {
    (node as HTMLElement).dataset.domInstance = `original-${index}`;
    return (node as HTMLElement).dataset.pageId ?? "";
  }));
  const originalAnchorOffset = await browserPreview.evaluate((container) => {
    const pages = container.querySelectorAll<HTMLElement>(".display-list-page");
    const anchor = pages[2];
    container.scrollTop = Math.max(0, anchor.offsetTop - 48);
    return anchor.getBoundingClientRect().top - container.getBoundingClientRect().top;
  });
  await expect.poll(() => browserPreview.evaluate((container) => container.scrollTop))
    .toBeGreaterThan(0);

  await editor.fill(source.replace("Page two before.", "Page two edited."));
  await expect(page.locator(".editor-status p")).toContainText("Compiled locally", {
    timeout: 15_000
  });
  await expect(pages).toHaveCount(3);
  const updatedIds = await pages.evaluateAll((nodes) => nodes.map((node) => (
    (node as HTMLElement).dataset.pageId ?? ""
  )));
  expect(updatedIds[0]).toBe(originalIds[0]);
  expect(updatedIds[1]).not.toBe(originalIds[1]);
  expect(updatedIds[2]).toBe(originalIds[2]);
  await expect(pages.nth(0)).toHaveAttribute("data-dom-instance", "original-0");
  await expect(pages.nth(2)).toHaveAttribute("data-dom-instance", "original-2");
  const updatedAnchorOffset = await browserPreview.evaluate((container) => {
    const anchor = container.querySelectorAll<HTMLElement>(".display-list-page")[2];
    return anchor.getBoundingClientRect().top - container.getBoundingClientRect().top;
  });
  expect(Math.abs(updatedAnchorOffset - originalAnchorOffset)).toBeLessThan(2);
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
