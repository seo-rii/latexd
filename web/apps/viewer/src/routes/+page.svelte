<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { ViewerEvent } from "@latexd/viewer-core";
  import {
    createLatexdApiClient,
    createLatexdViewerRealtime,
    mountLatexdViewerHost,
    type LatexdServerMessage
  } from "$lib";
  import {
    compileInBrowser,
    compileProjectInBrowser,
    type BrowserCompilePage
  } from "$lib/browser-compiler";

  type EditorStatus = "idle" | "loading" | "ready" | "dirty" | "saving" | "saved" | "error";
  type EditorFocus = {
    file: string;
    line: number;
    column: number;
  };

  let viewerController: ReturnType<typeof mountLatexdViewerHost> | null = null;
  let apiClient: ReturnType<typeof createLatexdApiClient> | null = null;
  let editorNode = $state<HTMLTextAreaElement | null>(null);
  let previewState = $state({
    currentRev: 0,
    lastAppliedRev: 0,
    building: false,
    lastBuildSucceeded: null as boolean | null,
    changedFiles: [] as string[],
    editorBridgeEnabled: false,
    socketPhase: "idle" as "idle" | "connecting" | "open" | "closed"
  });
  let availableFiles = $state<string[]>([]);
  let activeFile = $state("");
  let editorText = $state("");
  let editorStatus = $state<EditorStatus>("idle");
  let editorMessage = $state("Waiting for latexd to expose a source file.");
  let currentFocus = $state<EditorFocus | null>(null);
  let lastSavedContent = $state("");
  let loadSerial = 0;
  let saveSerial = 0;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let previewSyncTimer: ReturnType<typeof setTimeout> | null = null;
  let lastEditorSyncKey = "";
  let suppressEditorSyncUntil = 0;
  const browserOnly = import.meta.env.VITE_LATEXD_BROWSER_ONLY === "true";
  let browserMode = $state(browserOnly);
  let browserPages = $state<BrowserCompilePage[]>([]);
  let browserDiagnostics = $state<string[]>([]);
  let browserEventCount = $state(0);
  let browserCompileTimer: ReturnType<typeof setTimeout> | null = null;
  let browserCompileSerial = 0;
  let browserFiles = $state<Record<string, Uint8Array>>({});
  let browserTextFiles = $state<Record<string, string>>({});
  let browserPdfUrl = $state("");

  const browserStarterSource = `\\documentclass{article}
\\title{latexd in WebAssembly}
\\author{Browser preview}
\\date{}

\\begin{document}
\\maketitle

\\begin{abstract}
This document is compiled locally in your browser. No daemon is required.
\\end{abstract}

\\section{Try it}
Edit this source and the preview will rebuild with latexd's TeX VM.

Inline math such as $x^2 + y^2 = z^2$ and citations \\cite{demo} are preserved.

\\begin{thebibliography}{1}
\\bibitem{demo} The latexd browser compiler.
\\end{thebibliography}
\\end{document}`;

  onMount(() => {
    if (browserOnly) {
      void activateBrowserMode();
    }
  });

  function focusKeyForPosition(file: string, line: number, column: number) {
    return `${file}:${Math.max(1, Math.round(line))}:${Math.max(1, Math.round(column))}`;
  }

  function preferredInitialFile(files: string[]) {
    return previewState.changedFiles.find((file) => files.includes(file) && file.endsWith(".tex"))
      ?? files.find((file) => file.endsWith(".tex"))
      ?? files[0]
      ?? "";
  }

  const mountWorkspace = (node: HTMLDivElement) => {
    apiClient = createLatexdApiClient();
    const realtime = createLatexdViewerRealtime();
    const unsubscribeSocketStatus = realtime.status.subscribe((status) => {
      previewState.socketPhase = status.phase;
    });
    const unsubscribeSocketMessages = realtime.messages.subscribe((message) => {
      if (!message) {
        return;
      }
      void syncFromRealtimeMessage(message).catch(() => {
        // Ignore malformed realtime payloads without creating an unhandled rejection.
      });
    });
    viewerController = mountLatexdViewerHost(node, {
      realtime,
      onEvent(event) {
        void syncFromViewer(event);
      }
    });
    const browserFallbackTimer = setTimeout(() => {
      if (previewState.socketPhase !== "open" && !activeFile) {
        void activateBrowserMode();
      }
    }, 900);
    return () => {
      clearTimeout(browserFallbackTimer);
      if (saveTimer) {
        clearTimeout(saveTimer);
        saveTimer = null;
      }
      if (previewSyncTimer) {
        clearTimeout(previewSyncTimer);
        previewSyncTimer = null;
      }
      if (browserCompileTimer) {
        clearTimeout(browserCompileTimer);
        browserCompileTimer = null;
      }
      if (browserPdfUrl) {
        URL.revokeObjectURL(browserPdfUrl);
        browserPdfUrl = "";
      }
      unsubscribeSocketMessages();
      unsubscribeSocketStatus();
      viewerController?.destroy();
      realtime.destroy();
      viewerController = null;
      apiClient = null;
    };
  };

  async function activateBrowserMode() {
    if (browserMode && activeFile) {
      return;
    }
    browserMode = true;
    availableFiles = ["main.tex"];
    activeFile = "main.tex";
    editorText = browserStarterSource;
    browserFiles = { "main.tex": new TextEncoder().encode(browserStarterSource) };
    browserTextFiles = { "main.tex": browserStarterSource };
    lastSavedContent = browserStarterSource;
    editorStatus = "loading";
    editorMessage = "Loading the local WebAssembly compiler…";
    await compileBrowserSource(browserStarterSource);
  }

  async function compileBrowserSource(source: string) {
    const requestId = ++browserCompileSerial;
    previewState.building = true;
    editorStatus = "saving";
    editorMessage = "Compiling main.tex locally with WebAssembly…";
    try {
      const encoder = new TextEncoder();
      const projectFiles = {
        ...browserFiles,
        [activeFile || "main.tex"]: encoder.encode(source)
      };
      const entry = projectFiles["main.tex"] ? "main.tex" : activeFile;
      const result = Object.keys(projectFiles).length === 1 && entry === "main.tex"
        ? await compileInBrowser(source)
        : await compileProjectInBrowser(projectFiles, entry);
      if (requestId !== browserCompileSerial) {
        return;
      }
      browserPages = result.pages;
      browserDiagnostics = result.diagnostics;
      browserEventCount = result.event_count;
      browserFiles = projectFiles;
      browserTextFiles = { ...browserTextFiles, [activeFile]: source };
      if (browserPdfUrl) {
        URL.revokeObjectURL(browserPdfUrl);
      }
      browserPdfUrl = URL.createObjectURL(new Blob([result.pdf as BlobPart], {
        type: "application/pdf"
      }));
      previewState.currentRev += 1;
      previewState.lastAppliedRev = previewState.currentRev;
      previewState.lastBuildSucceeded = true;
      lastSavedContent = source;
      editorStatus = "saved";
      editorMessage = `Compiled locally: ${result.pages.length} page(s), ${result.event_count} render events.`;
    } catch (error) {
      if (requestId !== browserCompileSerial) {
        return;
      }
      previewState.lastBuildSucceeded = false;
      editorStatus = "error";
      editorMessage = error instanceof Error ? error.message : "WebAssembly compilation failed.";
    } finally {
      if (requestId === browserCompileSerial) {
        previewState.building = false;
      }
    }
  }

  function queueBrowserCompile() {
    if (browserCompileTimer) {
      clearTimeout(browserCompileTimer);
    }
    browserCompileTimer = setTimeout(() => {
      browserCompileTimer = null;
      void compileBrowserSource(editorText);
    }, 250);
  }

  async function syncFromViewer(event: ViewerEvent) {
    if (event.type === "state-changed") {
      const previousRev = previewState.lastAppliedRev;
      previewState.currentRev = event.state.currentRev;
      previewState.lastAppliedRev = event.state.lastAppliedRev;
      previewState.building = event.state.building;
      previewState.lastBuildSucceeded = event.state.lastBuildSucceeded;
      previewState.changedFiles = Array.isArray(event.state.changedFiles)
        ? event.state.changedFiles.slice()
        : [];
      previewState.editorBridgeEnabled = event.state.editorBridgeEnabled === true;
      const snapshotFiles = Object.keys((event.state.sourceFiles ?? {}) as Record<string, unknown>)
        .sort((left, right) => left.localeCompare(right));
      if (snapshotFiles.length > 0) {
        availableFiles = Array.from(new Set([
          ...snapshotFiles,
          ...(activeFile ? [activeFile] : [])
        ])).sort((left, right) => left.localeCompare(right));
        const initialFile = preferredInitialFile(availableFiles);
        if (!activeFile && initialFile) {
          await openFile(initialFile);
          return;
        }
      } else if (availableFiles.length === 0) {
        await refreshAvailableFiles(event.state.lastAppliedRev);
      }
      if (event.state.lastAppliedRev !== previousRev && activeFile) {
        lastEditorSyncKey = "";
        queuePreviewSyncFromEditor();
      }
      return;
    }
    if (
      event.type !== "source-selected"
      && event.type !== "source-jump-resolved"
      && event.type !== "open-source-resolved"
    ) {
      return;
    }
    const detail = event.detail as any;
    const file =
      detail?.source?.file
      ?? detail?.item?.file
      ?? detail?.response?.file
      ?? "";
    if (!file) {
      return;
    }
    const line =
      typeof detail?.source?.startLine === "number"
        ? detail.source.startLine
        : typeof detail?.line === "number"
          ? detail.line
          : typeof detail?.item?.start_line === "number"
            ? detail.item.start_line
            : 1;
    const column =
      typeof detail?.source?.startColumn === "number"
        ? detail.source.startColumn
        : typeof detail?.column === "number"
          ? detail.column
          : 1;
    const focus = {
      file,
      line: Math.max(1, Math.round(line)),
      column: Math.max(1, Math.round(column))
    };
    currentFocus = focus;
    if (editorNode === document.activeElement) {
      return;
    }
    if (activeFile === file && editorText.length > 0) {
      await focusEditorLine(focus.line, focus.column);
      return;
    }
    await openFile(file, focus);
  }

  async function syncFromRealtimeMessage(message: LatexdServerMessage) {
    if (message.type === "build_started") {
      previewState.currentRev = Math.max(previewState.currentRev, message.rev);
      previewState.building = true;
      previewState.changedFiles = Array.isArray(message.changed_files)
        ? message.changed_files.slice()
        : [];
      return;
    }
    if (message.type === "full_pdf_ready") {
      previewState.currentRev = Math.max(previewState.currentRev, message.rev);
      previewState.lastAppliedRev = Math.max(previewState.lastAppliedRev, message.rev);
      return;
    }
    if (message.type === "source_snapshot") {
      if (!Array.isArray(message.files)) {
        return;
      }
      const snapshotFiles = message.files
        .map((entry) => entry.file)
        .filter((file) => typeof file === "string" && file.length > 0)
        .sort((left, right) => left.localeCompare(right));
      if (snapshotFiles.length === 0) {
        return;
      }
      availableFiles = Array.from(new Set([
        ...snapshotFiles,
        ...(activeFile ? [activeFile] : [])
      ])).sort((left, right) => left.localeCompare(right));
      return;
    }
    if (message.type === "build_finished") {
      previewState.currentRev = Math.max(previewState.currentRev, message.rev);
      previewState.building = false;
      previewState.lastBuildSucceeded = message.success;
    }
  }

  async function refreshAvailableFiles(rev: number) {
    if (!apiClient) {
      return;
    }
    try {
      const response = await apiClient.fetchSourceFiles({ rev });
      availableFiles = Array.from(new Set([
        ...response.files,
        ...availableFiles,
        ...(activeFile ? [activeFile] : [])
      ])).sort((left, right) => left.localeCompare(right));
      const initialFile = preferredInitialFile(availableFiles);
      if (!activeFile && initialFile) {
        await openFile(initialFile);
      }
    } catch (error) {
      if (availableFiles.length === 0) {
        editorStatus = "error";
        editorMessage = error instanceof Error
          ? error.message
          : "Failed to discover editable source files.";
      }
    }
  }

  async function openFile(file: string, focus: EditorFocus | null = null) {
    if (browserMode) {
      if (!file || file === activeFile) {
        return;
      }
      if (activeFile) {
        browserTextFiles = { ...browserTextFiles, [activeFile]: editorText };
        browserFiles = {
          ...browserFiles,
          [activeFile]: new TextEncoder().encode(editorText)
        };
      }
      activeFile = file;
      editorText = browserTextFiles[file] ?? "";
      lastSavedContent = editorText;
      editorStatus = "ready";
      editorMessage = `Editing ${file} in browser memfs`;
      return;
    }
    if (!apiClient || !file) {
      return;
    }
    if (activeFile && activeFile !== file) {
      await flushSave();
    }
    const requestId = ++loadSerial;
    editorStatus = "loading";
    editorMessage = `Loading ${file}…`;
    const currentRev = previewState.lastAppliedRev;
    const cachedSourceFiles = viewerController?.getState()?.sourceFiles as
      | Record<string, { rev: number; content: string }>
      | undefined;
    const cached = cachedSourceFiles?.[file];
    try {
      const content = cached && cached.rev === currentRev
        ? cached.content
        : (await apiClient.fetchSourceFile({ rev: currentRev, file })).content;
      if (requestId !== loadSerial) {
        return;
      }
      availableFiles = Array.from(new Set([
        ...availableFiles,
        file
      ])).sort((left, right) => left.localeCompare(right));
      activeFile = file;
      editorText = content;
      lastSavedContent = content;
      lastEditorSyncKey = "";
      editorStatus = "ready";
      editorMessage = `Editing ${file}`;
      const nextFocus = focus ?? (currentFocus?.file === file ? currentFocus : null);
      if (nextFocus) {
        currentFocus = nextFocus;
        await tick();
        await focusEditorLine(nextFocus.line, nextFocus.column);
      } else {
        await tick();
        queuePreviewSyncFromEditor();
      }
    } catch (error) {
      if (requestId !== loadSerial) {
        return;
      }
      editorStatus = "error";
      editorMessage = error instanceof Error
        ? error.message
        : `Failed to load ${file}.`;
    }
  }

  function queueSave() {
    if (saveTimer) {
      clearTimeout(saveTimer);
    }
    saveTimer = setTimeout(() => {
      void flushSave();
    }, 250);
  }

  async function flushSave() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (browserMode) {
      if (editorText !== lastSavedContent) {
        await compileBrowserSource(editorText);
      }
      return;
    }
    if (!apiClient || !activeFile || editorText === lastSavedContent) {
      if (activeFile && editorStatus === "dirty") {
        editorStatus = "ready";
        editorMessage = `Editing ${activeFile}`;
      }
      return;
    }
    const requestId = ++saveSerial;
    const sentContent = editorText;
    editorStatus = "saving";
    editorMessage = `Saving ${activeFile}…`;
    try {
      const response = await apiClient.updateSourceFile({
        file: activeFile,
        content: sentContent
      });
      if (requestId !== saveSerial) {
        return;
      }
      lastSavedContent = sentContent;
      if (editorText !== sentContent) {
        editorStatus = "dirty";
        editorMessage = `Unsaved edits in ${activeFile}`;
        return;
      }
      editorStatus = "saved";
      editorMessage = `Saved ${response.file}. Waiting for the next preview revision…`;
    } catch (error) {
      if (requestId !== saveSerial) {
        return;
      }
      editorStatus = "error";
      editorMessage = error instanceof Error
        ? error.message
        : `Failed to save ${activeFile}.`;
    }
  }

  function handleEditorInput() {
    if (!activeFile) {
      return;
    }
    if (editorText === lastSavedContent) {
      editorStatus = "ready";
      editorMessage = `Editing ${activeFile}`;
      return;
    }
    editorStatus = "dirty";
    editorMessage = `Unsaved edits in ${activeFile}`;
    if (browserMode) {
      queueBrowserCompile();
      return;
    }
    queueSave();
    queuePreviewSyncFromEditor();
  }

  async function loadBrowserProject(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const selected = Array.from(input.files ?? []);
    if (selected.length === 0) {
      return;
    }
    editorStatus = "loading";
    editorMessage = `Loading ${selected.length} project files into memfs…`;
    const nextFiles: Record<string, Uint8Array> = {};
    const nextTextFiles: Record<string, string> = {};
    for (const file of selected) {
      const relativePath = (file as File & { webkitRelativePath?: string }).webkitRelativePath
        || file.name;
      const path = relativePath.split("/").slice(relativePath.includes("/") ? 1 : 0).join("/");
      const bytes = new Uint8Array(await file.arrayBuffer());
      nextFiles[path] = bytes;
      if (/\.(tex|sty|cls|cfg|def|bbl)$/i.test(path)) {
        nextTextFiles[path] = new TextDecoder().decode(bytes);
      }
    }
    const sourceFiles = Object.keys(nextTextFiles).sort((left, right) => left.localeCompare(right));
    const entry = sourceFiles.includes("main.tex")
      ? "main.tex"
      : sourceFiles.find((file) => file.endsWith(".tex"));
    if (!entry) {
      editorStatus = "error";
      editorMessage = "The selected project does not contain a TeX entry file.";
      return;
    }
    browserFiles = nextFiles;
    browserTextFiles = nextTextFiles;
    availableFiles = sourceFiles;
    activeFile = entry;
    editorText = nextTextFiles[entry];
    lastSavedContent = editorText;
    await compileBrowserSource(editorText);
    input.value = "";
  }

  async function focusEditorLine(line: number, column = 1) {
    if (!editorNode) {
      return;
    }
    const targetLine = Math.max(1, Math.round(line));
    const targetColumn = Math.max(1, Math.round(column));
    const lines = editorText.split("\n");
    const clampedLine = Math.min(targetLine, Math.max(1, lines.length));
    let caret = 0;
    for (let index = 0; index < clampedLine - 1; index += 1) {
      caret += lines[index].length + 1;
    }
    caret += Math.min(lines[clampedLine - 1]?.length ?? 0, targetColumn - 1);
    suppressEditorSyncUntil = Date.now() + 200;
    lastEditorSyncKey = focusKeyForPosition(activeFile, clampedLine, targetColumn);
    const pageScrollX = window.scrollX;
    const pageScrollY = window.scrollY;
    editorNode.focus({ preventScroll: true });
    editorNode.setSelectionRange(caret, caret);
    const computedLineHeight = Number.parseFloat(window.getComputedStyle(editorNode).lineHeight);
    const lineHeight = Number.isFinite(computedLineHeight) ? computedLineHeight : 22;
    editorNode.scrollTop = Math.max(0, (clampedLine - 2) * lineHeight);
    window.scrollTo(pageScrollX, pageScrollY);
  }

  function handleEditorCaretActivity() {
    queuePreviewSyncFromEditor();
  }

  function queuePreviewSyncFromEditor() {
    if (previewSyncTimer) {
      clearTimeout(previewSyncTimer);
    }
    previewSyncTimer = setTimeout(() => {
      void syncEditorSelectionToPreview();
    }, 90);
  }

  async function syncEditorSelectionToPreview() {
    if (!viewerController || !editorNode || !activeFile) {
      return;
    }
    if (Date.now() < suppressEditorSyncUntil) {
      return;
    }
    const caret = Math.max(
      0,
      Math.min(editorText.length, editorNode.selectionStart ?? 0)
    );
    let line = 1;
    let column = 1;
    for (let index = 0; index < caret; index += 1) {
      if (editorText[index] === "\n") {
        line += 1;
        column = 1;
      } else {
        column += 1;
      }
    }
    const nextSyncKey = focusKeyForPosition(activeFile, line, column);
    if (nextSyncKey === lastEditorSyncKey) {
      return;
    }
    lastEditorSyncKey = nextSyncKey;
    currentFocus = { file: activeFile, line, column };
    try {
      await viewerController.jumpToSource({
        file: activeFile,
        line,
        column
      });
    } catch {
      // Keep typing responsive even when the preview revision is temporarily behind.
    }
  }
</script>

<svelte:head>
  <title>latexd studio</title>
</svelte:head>

<div class="studio-shell">
  <section class="studio-hero">
    <div class="studio-hero__intro">
      <p class="studio-hero__eyebrow">Latex Live Desk</p>
      <p class="studio-hero__summary">
        Live editor and preview for the current `latexd` workspace.
      </p>
    </div>
    <div class="studio-hero__chips">
      <span class="chip">rev {previewState.currentRev}</span>
      <span class="chip">applied {previewState.lastAppliedRev}</span>
      <span class:chip-active={previewState.building} class="chip">
        {previewState.building ? "building" : "idle"}
      </span>
      <span
        class:chip-good={previewState.lastBuildSucceeded === true}
        class:chip-bad={previewState.lastBuildSucceeded === false}
        class="chip"
      >
        {previewState.lastBuildSucceeded === null
          ? "waiting"
          : previewState.lastBuildSucceeded
            ? "last build ok"
            : "last build failed"}
      </span>
      <span class="chip">
        {browserMode
          ? "local WASM compiler"
          : previewState.editorBridgeEnabled
            ? "external editor bridge on"
            : "in-app editor active"}
      </span>
      <span
        class:chip-good={previewState.socketPhase === "open"}
        class:chip-bad={previewState.socketPhase === "closed"}
        class="chip"
      >
        socket {previewState.socketPhase}
      </span>
    </div>
  </section>

  <div class="workspace-grid">
    <section class="editor-panel">
      <div class="panel-header">
        <div>
          <p class="panel-header__label">Workspace editor</p>
          <h2>{activeFile || "No source file selected yet"}</h2>
        </div>
        <button
          type="button"
          class="save-button"
          disabled={!activeFile || editorStatus === "saving" || editorText === lastSavedContent}
          onclick={() => void flushSave()}
        >
          Save now
        </button>
      </div>

      <div class="editor-toolbar">
        {#if browserMode}
          <label class="project-button">
            Open project directory
            <input type="file" multiple webkitdirectory onchange={loadBrowserProject} />
          </label>
        {/if}
        <label class="field">
          <span>LaTeX file</span>
          <select
            value={activeFile}
            disabled={availableFiles.length === 0}
            onchange={(event) => void openFile((event.currentTarget as HTMLSelectElement).value)}
          >
            {#if availableFiles.length === 0}
              <option value="">Waiting for files…</option>
            {/if}
            {#each availableFiles as file (file)}
              <option value={file}>{file}</option>
            {/each}
          </select>
        </label>
        <div class="editor-status">
          <span class={`editor-status__badge editor-status__badge--${editorStatus}`}>{editorStatus}</span>
          <p>{editorMessage}</p>
        </div>
      </div>

      {#if previewState.changedFiles.length > 0}
        <div class="changed-files">
          <p class="changed-files__label">Latest rebuild input set</p>
          <ul>
            {#each previewState.changedFiles as file (file)}
              <li>{file}</li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if activeFile}
        <div class="editor-paper">
          <textarea
            bind:this={editorNode}
            bind:value={editorText}
            class="editor-textarea"
            autocapitalize="off"
            autocomplete="off"
            placeholder="Type LaTeX here…"
            spellcheck="false"
            oninput={handleEditorInput}
            onmouseup={handleEditorCaretActivity}
            onselect={handleEditorCaretActivity}
          ></textarea>
        </div>
      {:else}
        <div class="editor-empty">
          <p>latexd has not exposed an editable source file yet.</p>
          <p>The editor will attach as soon as the first file list arrives.</p>
        </div>
      {/if}
    </section>

    <section class="preview-panel">
      <div class="panel-header">
        <div>
          <p class="panel-header__label">Preview workspace</p>
          <h2>Incremental browser viewer</h2>
        </div>
        {#if currentFocus}
          <p class="preview-focus">
            Following {currentFocus.file}:{currentFocus.line}
          </p>
        {/if}
      </div>

      {#if browserMode}
        <div class="browser-preview" aria-label="WebAssembly document preview">
          <div class="browser-preview__meta">
            <span>{browserPages.length} page(s)</span>
            <span>{browserEventCount} events</span>
            <span>{browserDiagnostics.length} diagnostics</span>
            {#if browserPdfUrl}
              <a href={browserPdfUrl} download="latexd-output.pdf">Download PDF</a>
            {/if}
          </div>
          {#each browserPages as page, index (page.page_id)}
            <article
              class="browser-page"
              aria-label={`Page ${index + 1}`}
              style={`aspect-ratio: ${page.width_pt} / ${page.height_pt}`}
            >
              {#each page.lines as line}
                <p class:browser-page__blank={line.length === 0}>{line || "\u00a0"}</p>
              {/each}
              <span class="browser-page__number">{index + 1}</span>
            </article>
          {/each}
          {#if browserDiagnostics.length > 0}
            <details class="browser-diagnostics">
              <summary>Compiler diagnostics</summary>
              {#each browserDiagnostics as diagnostic}
                <p>{diagnostic}</p>
              {/each}
            </details>
          {/if}
        </div>
      {:else}
        <div class="viewer-frame" {@attach mountWorkspace}></div>
      {/if}
    </section>
  </div>
</div>

<style>
  :global(html),
  :global(body) {
    height: 100%;
    margin: 0;
  }

  :global(body) {
    color: #f8f2e5;
  }

  .studio-shell {
    box-sizing: border-box;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 1.25rem;
    min-height: 100dvh;
    padding: 2rem;
    background:
      radial-gradient(circle at top left, rgba(249, 203, 92, 0.16), transparent 32%),
      radial-gradient(circle at top right, rgba(132, 191, 255, 0.14), transparent 28%),
      linear-gradient(180deg, #14213d 0%, #0e172a 48%, #111827 100%);
  }

  .studio-hero {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    max-width: 72rem;
    width: 100%;
    margin: 0 auto;
  }

  .studio-hero__intro {
    display: grid;
    gap: 0.35rem;
  }

  .studio-hero__eyebrow,
  .panel-header__label,
  .changed-files__label {
    margin: 0 0 0.45rem;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: #f7c873;
  }

  h2 {
    margin: 0;
    font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
  }

  .studio-hero__summary {
    margin: 0;
    font-size: 0.95rem;
    line-height: 1.5;
    color: rgba(248, 242, 229, 0.82);
  }

  .studio-hero__chips {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.65rem;
  }

  .chip {
    padding: 0.55rem 0.9rem;
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 999px;
    background: rgba(8, 15, 29, 0.46);
    color: rgba(248, 242, 229, 0.92);
    font-size: 0.86rem;
  }

  .chip-active {
    border-color: rgba(247, 200, 115, 0.7);
    box-shadow: 0 0 0 1px rgba(247, 200, 115, 0.2) inset;
  }

  .chip-good {
    border-color: rgba(74, 222, 128, 0.7);
  }

  .chip-bad {
    border-color: rgba(248, 113, 113, 0.7);
  }

  .workspace-grid {
    display: grid;
    grid-template-columns: minmax(22rem, 28rem) minmax(0, 1fr);
    gap: 1.25rem;
    max-width: 92rem;
    margin: 0 auto;
    width: 100%;
    min-height: 0;
  }

  .editor-panel,
  .preview-panel {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    overflow: hidden;
    min-height: 0;
    padding: 1.1rem;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 1.5rem;
    background: rgba(7, 12, 24, 0.62);
    backdrop-filter: blur(18px);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.28);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: flex-start;
  }

  .panel-header h2 {
    font-size: 1.55rem;
    line-height: 1.05;
  }

  .save-button {
    border: 0;
    border-radius: 999px;
    padding: 0.75rem 1rem;
    background: linear-gradient(135deg, #f7c873, #f59e0b);
    color: #1b2438;
    font-weight: 700;
    cursor: pointer;
  }

  .save-button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .editor-toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0.9rem;
  }

  .field {
    display: grid;
    gap: 0.45rem;
  }

  .project-button {
    display: inline-flex;
    justify-content: center;
    padding: 0.72rem 0.9rem;
    border: 1px solid rgba(247, 200, 115, 0.5);
    border-radius: 0.9rem;
    color: #f7c873;
    cursor: pointer;
    font-size: 0.83rem;
  }

  .project-button input {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .field span {
    font-size: 0.83rem;
    color: rgba(248, 242, 229, 0.72);
  }

  .field select {
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 0.9rem;
    padding: 0.8rem 0.9rem;
    background: rgba(255, 255, 255, 0.06);
    color: inherit;
    font: inherit;
  }

  .editor-status {
    display: grid;
    gap: 0.35rem;
    padding: 0.8rem 0.95rem;
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.05);
  }

  .editor-status p,
  .preview-focus,
  .editor-empty p {
    margin: 0;
    color: rgba(248, 242, 229, 0.8);
  }

  .editor-status__badge {
    justify-self: start;
    padding: 0.25rem 0.55rem;
    border-radius: 999px;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  .editor-status__badge--ready,
  .editor-status__badge--saved {
    border-color: rgba(74, 222, 128, 0.55);
    color: #bbf7d0;
  }

  .editor-status__badge--dirty,
  .editor-status__badge--saving,
  .editor-status__badge--loading {
    border-color: rgba(247, 200, 115, 0.6);
    color: #fde68a;
  }

  .editor-status__badge--error {
    border-color: rgba(248, 113, 113, 0.6);
    color: #fecaca;
  }

  .editor-status__badge--idle {
    color: rgba(248, 242, 229, 0.7);
  }

  .changed-files {
    padding: 0.9rem 1rem;
    border-radius: 1rem;
    background: rgba(15, 23, 42, 0.52);
  }

  .changed-files ul {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .changed-files li {
    padding: 0.35rem 0.65rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    font-size: 0.83rem;
    color: rgba(248, 242, 229, 0.85);
  }

  .editor-paper {
    box-sizing: border-box;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 1rem;
    border-radius: 1.4rem;
    background:
      linear-gradient(180deg, rgba(255, 250, 240, 0.98), rgba(247, 241, 228, 0.96)),
      repeating-linear-gradient(
        180deg,
        rgba(181, 163, 134, 0.08) 0,
        rgba(181, 163, 134, 0.08) 1px,
        transparent 1px,
        transparent 2rem
      );
  }

  .editor-textarea {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    min-height: 0;
    border: 0;
    background: transparent;
    color: #312317;
    overflow: auto;
    resize: none;
    font: 1rem/1.7 "Cascadia Code", "JetBrains Mono", "SFMono-Regular", Menlo, Consolas, monospace;
    outline: none;
    tab-size: 2;
  }

  .editor-empty {
    display: grid;
    place-items: center;
    min-height: 24rem;
    border: 1px dashed rgba(255, 255, 255, 0.16);
    border-radius: 1.4rem;
    padding: 1rem;
    text-align: center;
  }

  .preview-panel {
    min-height: 0;
    border-color: rgba(15, 23, 42, 0.08);
    background: white;
    box-shadow: 0 18px 48px rgba(15, 23, 42, 0.1);
    color: #111827;
  }

  .viewer-frame {
    box-sizing: border-box;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border-radius: 1.25rem;
    background: white;
  }

  .browser-preview {
    display: grid;
    justify-items: center;
    gap: 1.25rem;
    overflow: auto;
    min-height: 0;
    padding: 1rem;
    border-radius: 1rem;
    background: #cbd1d8;
  }

  .browser-preview__meta {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    gap: 0.75rem;
    padding: 0.55rem 0.8rem;
    border-radius: 999px;
    background: rgba(15, 23, 42, 0.88);
    color: #f8f2e5;
    font-size: 0.75rem;
    backdrop-filter: blur(12px);
  }

  .browser-preview__meta a {
    color: #f7c873;
  }

  .browser-page {
    position: relative;
    box-sizing: border-box;
    width: min(100%, 46rem);
    min-height: 48rem;
    margin: 0;
    padding: 4.75rem 4.5rem;
    background: #fffef9;
    color: #161616;
    box-shadow: 0 12px 36px rgba(15, 23, 42, 0.24);
    font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
    font-size: 0.98rem;
    line-height: 1.48;
  }

  .browser-page p {
    min-height: 1.48em;
    margin: 0;
    white-space: pre-wrap;
  }

  .browser-page__blank {
    height: 0.7em;
  }

  .browser-page__number {
    position: absolute;
    bottom: 2rem;
    left: 50%;
    transform: translateX(-50%);
    color: #6b7280;
    font-size: 0.8rem;
  }

  .browser-diagnostics {
    width: min(100%, 46rem);
    color: #111827;
  }

  .preview-panel .preview-focus {
    color: #64748b;
  }

  @media (min-width: 981px) {
    :global(body) {
      overflow: hidden;
    }

    .studio-shell {
      height: 100dvh;
    }

    .workspace-grid {
      height: 100%;
    }

    .editor-panel,
    .preview-panel {
      height: 100%;
    }
  }

  @media (max-width: 980px) {
    .studio-shell {
      padding: 1rem;
      grid-template-rows: auto;
    }

    .studio-hero {
      flex-direction: column;
    }

    .studio-hero__chips {
      justify-content: flex-start;
    }

    .workspace-grid {
      grid-template-columns: 1fr;
      height: auto;
    }

    .editor-panel,
    .preview-panel {
      min-height: auto;
    }

    .viewer-frame {
      min-height: 30rem;
    }

    .editor-paper {
      min-height: 24rem;
    }
  }
</style>
