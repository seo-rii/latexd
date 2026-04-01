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
    const pageCard = root?.querySelector(".page-card");
    const placeholder = root?.querySelector("#placeholder");
    const sidebarHeading = root?.querySelector("h1");
    return {
      layoutWidth: layout instanceof HTMLElement ? layout.clientWidth : null,
      sidebarWidth: aside instanceof HTMLElement ? aside.clientWidth : null,
      panelBackground: host.parentElement instanceof HTMLElement
        ? getComputedStyle(host.parentElement).backgroundColor
        : null,
      frameClientHeight: frame instanceof HTMLElement ? frame.clientHeight : null,
      frameScrollHeight: frame instanceof HTMLElement ? frame.scrollHeight : null,
      previewStackOffsetTop: stack instanceof HTMLElement ? stack.offsetTop : null,
      hostBackground: host instanceof HTMLElement ? getComputedStyle(host).backgroundColor : null,
      sidebarBackground: aside instanceof HTMLElement ? getComputedStyle(aside).backgroundColor : null,
      frameBackground: frame instanceof HTMLElement ? getComputedStyle(frame).backgroundColor : null,
      frameShadow: frame instanceof HTMLElement ? getComputedStyle(frame).boxShadow : null,
      pageCardBackground: pageCard instanceof HTMLElement ? getComputedStyle(pageCard).backgroundColor : null,
      pageCardShadow: pageCard instanceof HTMLElement ? getComputedStyle(pageCard).boxShadow : null,
      placeholderHidden: placeholder instanceof HTMLElement ? placeholder.hidden : null,
      placeholderDisplay: placeholder instanceof HTMLElement ? getComputedStyle(placeholder).display : null,
      hasLegacyHeading: sidebarHeading instanceof HTMLElement
    };
  });
  expect(viewerGeometry.panelBackground).toBe("rgb(255, 255, 255)");
  expect(viewerGeometry.hostBackground).toBe("rgb(255, 255, 255)");
  expect(viewerGeometry.sidebarBackground).toBe("rgb(255, 255, 255)");
  expect(viewerGeometry.frameBackground).toBe("rgb(255, 255, 255)");
  expect(viewerGeometry.frameShadow).toBe("none");
  expect(viewerGeometry.pageCardBackground).toBe("rgb(255, 255, 255)");
  expect(viewerGeometry.pageCardShadow).toBe("none");
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

test("viewer page controls stay in sync with frame scrolling", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("./", { waitUntil: "networkidle" });

  const viewer = page.locator(".viewer-frame");

  const readPageLabel = () =>
    viewer.evaluate((host) => {
      const label = host.shadowRoot?.querySelector("#page-label");
      return label instanceof HTMLElement ? label.textContent : null;
    });
  const readPageCount = () =>
    viewer.evaluate((host) => {
      const pageCards = host.shadowRoot?.querySelectorAll(".page-card");
      return pageCards?.length ?? 0;
    });

  const clickViewerButton = (buttonId: string) =>
    viewer.evaluate((host, id) => {
      const button = host.shadowRoot?.querySelector(`#${id}`);
      if (!(button instanceof HTMLButtonElement)) {
        return false;
      }
      button.click();
      return true;
    }, buttonId);

  await expect.poll(readPageCount).toBeGreaterThan(0);
  const pageCount = await readPageCount();
  await expect.poll(readPageLabel).toBe(`1 / ${pageCount}`);
  expect(await clickViewerButton("next-page")).toBe(true);
  await expect.poll(readPageLabel).toBe(pageCount > 1 ? `2 / ${pageCount}` : `1 / ${pageCount}`);
  expect(await clickViewerButton("prev-page")).toBe(true);
  await expect.poll(readPageLabel).toBe(`1 / ${pageCount}`);

  const secondPageScrollWorked = await viewer.evaluate((host) => {
    const root = host.shadowRoot;
    const frame = root?.querySelector("#frame");
    const pageCards = root?.querySelectorAll(".page-card");
    if (!(frame instanceof HTMLElement) || !pageCards || pageCards.length < 2) {
      return false;
    }
    frame.scrollTop = pageCards[1].offsetTop;
    frame.dispatchEvent(new Event("scroll"));
    return true;
  });
  if (!secondPageScrollWorked) {
    return;
  }
  await expect.poll(readPageLabel).toBe(`2 / ${pageCount}`);

  const thirdPageScrollWorked = await viewer.evaluate((host) => {
    const root = host.shadowRoot;
    const frame = root?.querySelector("#frame");
    const pageCards = root?.querySelectorAll(".page-card");
    if (!(frame instanceof HTMLElement) || !pageCards || pageCards.length < 3) {
      return false;
    }
    frame.scrollTop = pageCards[2].offsetTop;
    frame.dispatchEvent(new Event("scroll"));
    return true;
  });
  if (thirdPageScrollWorked) {
    await expect.poll(readPageLabel).toBe(`3 / ${pageCount}`);
  }
});
