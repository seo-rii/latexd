import { expect, test } from "@playwright/test";

test("viewer app renders the bundled sample without request failures", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("./", { waitUntil: "networkidle" });

  await expect(page.getByText("Latex Live Desk")).toBeVisible();
  await expect(page.getByText("last build ok")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "LaTeX file" })).toBeVisible();
  await expect(page.getByPlaceholder("Type LaTeX here…")).toBeVisible();
  await expect(page.locator(".editor-status p")).toContainText("Editing");
  await expect(page.locator(".viewer-frame")).toContainText("Revision");
  await expect(page.locator(".viewer-frame")).toContainText(/Page 1 \/ \d+/);
  const viewerGeometry = await page.locator(".viewer-frame").evaluate((host) => {
    const root = host.shadowRoot;
    const layout = root?.querySelector("main");
    const aside = root?.querySelector("aside");
    const frame = root?.querySelector("#frame");
    const stack = root?.querySelector("#preview-stack");
    const placeholder = root?.querySelector("#placeholder");
    const sidebarHeading = root?.querySelector("h1");
    return {
      layoutWidth: layout instanceof HTMLElement ? layout.clientWidth : null,
      sidebarWidth: aside instanceof HTMLElement ? aside.clientWidth : null,
      frameClientHeight: frame instanceof HTMLElement ? frame.clientHeight : null,
      frameScrollHeight: frame instanceof HTMLElement ? frame.scrollHeight : null,
      previewStackOffsetTop: stack instanceof HTMLElement ? stack.offsetTop : null,
      frameBackground: frame instanceof HTMLElement ? getComputedStyle(frame).backgroundColor : null,
      placeholderHidden: placeholder instanceof HTMLElement ? placeholder.hidden : null,
      placeholderDisplay: placeholder instanceof HTMLElement ? getComputedStyle(placeholder).display : null,
      hasLegacyHeading: sidebarHeading instanceof HTMLElement
    };
  });
  expect(viewerGeometry.frameBackground).toBe("rgb(255, 255, 255)");
  expect(viewerGeometry.placeholderHidden).toBe(true);
  expect(viewerGeometry.placeholderDisplay).toBe("none");
  expect(viewerGeometry.hasLegacyHeading).toBe(false);
  expect(viewerGeometry.sidebarWidth ?? 0).toBeGreaterThan(0);
  expect(viewerGeometry.sidebarWidth ?? Infinity).toBeLessThanOrEqual(288);
  expect(viewerGeometry.layoutWidth ?? 0).toBeGreaterThan(viewerGeometry.sidebarWidth ?? 0);
  expect(viewerGeometry.previewStackOffsetTop).not.toBeNull();
  expect(viewerGeometry.frameScrollHeight ?? 0).toBeGreaterThan(viewerGeometry.frameClientHeight ?? 0);
  await expect(page.locator("body")).not.toContainText("latexd request failed");
});

test("editor keeps document scrolling stable while typing", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("./", { waitUntil: "networkidle" });

  const textarea = page.getByPlaceholder("Type LaTeX here…");
  await expect(textarea).toBeVisible();
  await textarea.click();

  const before = await page.evaluate(() => ({
    bodyOverflowY: window.getComputedStyle(document.body).overflowY,
    scrollY: window.scrollY
  }));
  expect(before.bodyOverflowY).toBe("hidden");
  expect(before.scrollY).toBeLessThanOrEqual(8);

  await textarea.fill(Array.from({ length: 200 }, (_, index) => `line ${index + 1}`).join("\n"));
  await page.keyboard.type("\ntrailing smoke text", { delay: 120 });

  const after = await page.evaluate(() => {
    const textarea = document.querySelector("textarea");
    if (!(textarea instanceof HTMLTextAreaElement)) {
      return null;
    }
    return {
      activeElementTag: document.activeElement?.tagName ?? null,
      endsWithTrailingText: textarea.value.endsWith("\ntrailing smoke text"),
      caretAtEnd: textarea.selectionStart === textarea.value.length,
      bodyOverflowY: window.getComputedStyle(document.body).overflowY,
      scrollY: window.scrollY,
      textareaClientHeight: textarea.clientHeight,
      textareaScrollHeight: textarea.scrollHeight
    };
  });
  expect(after).not.toBeNull();
  expect(after?.activeElementTag).toBe("TEXTAREA");
  expect(after?.endsWithTrailingText).toBe(true);
  expect(after?.caretAtEnd).toBe(true);
  expect(after?.bodyOverflowY).toBe("hidden");
  expect(after?.scrollY ?? 0).toBeLessThanOrEqual(8);
  expect(after?.textareaScrollHeight ?? 0).toBeGreaterThan(after?.textareaClientHeight ?? 0);
});
