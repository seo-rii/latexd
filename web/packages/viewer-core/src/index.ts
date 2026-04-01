import { VIEWER_STYLE, VIEWER_TEMPLATE } from "./template";

export type SourceJumpRequest = {
  file: string;
  offset: number | null;
  line: number | null;
  column?: number;
  source_hash?: string;
  launch?: boolean;
};

export type OpenSourceRequest = Partial<SourceJumpRequest> & {
  source_hash?: string;
  launch?: boolean;
};

export type SourceSelection = {
  file: string;
  pageId?: string;
  itemId?: string;
  startUtf8?: number;
  endUtf8?: number;
  outputStartUtf8?: number;
  outputEndUtf8?: number;
  pageSourceStartUtf8?: number;
  pageSourceEndUtf8?: number;
  pageOutputStartUtf8?: number;
  pageOutputEndUtf8?: number;
  startLine?: number | null;
  endLine?: number | null;
  startColumn?: number;
  leftPx?: number;
  rightPx?: number;
  topPx?: number;
  bottomPx?: number;
  sourceHash?: string;
};

export type ViewerEvent =
  | { type: "state-changed"; state: any }
  | { type: "source-hovered"; detail: { rev: number; source: SourceSelection | null } }
  | { type: "source-selected"; detail: { rev: number; source: SourceSelection | null } }
  | { type: "source-jump-resolved"; detail: any }
  | { type: "source-jump-failed"; detail: any }
  | { type: "source-hover-resolved"; detail: any }
  | { type: "source-hover-failed"; detail: any }
  | { type: "open-source"; detail: any }
  | { type: "open-source-resolved"; detail: any }
  | { type: "open-source-failed"; detail: any };

export interface ViewerMountOptions {
  window?: Window & typeof globalThis;
  document?: Document;
  onEvent?: (event: ViewerEvent) => void;
  transport: ViewerTransport;
}

export interface ViewerSocket extends EventTarget {
  readyState: number;
  send(data: string): void;
  close(): void;
}

export interface ViewerTransport {
  fetchState(): Promise<any>;
  fetchTileManifest(args: {
    height: number;
    left: number;
    pageId: string;
    rev: number;
    scale: number;
    tileSize: number;
    top: number;
    width: number;
  }): Promise<any>;
  fetchSyncMap(args: {
    pageId: string;
    rev: number;
  }): Promise<any>;
  fetchSourceFile(args: {
    file: string;
    rev: number;
  }): Promise<any>;
  jumpToSource(args: {
    request: SourceJumpRequest;
    rev: number;
  }): Promise<any>;
  openSource(args: {
    request: OpenSourceRequest;
    rev: number;
  }): Promise<any>;
  openWebSocket(): ViewerSocket;
}

export const initialState = {
  currentRev: 0,
  lastAppliedRev: 0,
  pdfUrl: null,
  pageIds: [],
  pages: [],
  syncMaps: {},
  sourceFiles: {},
  tileLayers: {},
  pendingPagePatchOps: [],
  diagnostics: [],
  changedFiles: [],
  building: false,
  lastBuildSucceeded: null,
  editorBridgeEnabled: false,
  currentPage: 1,
  zoom: 1,
  scrollTop: 0,
  hoveredSource: null,
  selectedSource: null
};

function sourceFilesFromSnapshot(entries) {
  const sourceFiles = {};
  for (const entry of entries ?? []) {
    if (typeof entry?.file !== "string" || entry.file.length === 0) {
      continue;
    }
    sourceFiles[entry.file] = {
      rev: entry.rev ?? null,
      content: typeof entry.content === "string" ? entry.content : "",
      lineCount: Number.isFinite(entry.line_count) ? entry.line_count : 1
    };
  }
  return sourceFiles;
}

export function zoomBucketForZoom(zoom) {
  return Math.round(Math.max(0.5, Math.min(3, zoom)) * 100);
}

function activePageForState(state) {
  if (state.pages.length === 0) {
    return null;
  }
  return state.pages[state.currentPage - 1] ?? null;
}

function pageExistsForState(state, pageId) {
  return state.pages.some((page) => page.pageId === pageId);
}

function retainPageEntries(previousPages, nextPages, entries, mapEntry = (entry) => entry) {
  const previousById = new Map<string, any>(previousPages.map((page) => [page.pageId, page]));
  const retained: Record<string, any> = {};
  for (const page of nextPages) {
    const previous = previousById.get(page.pageId);
    if (!previous) {
      continue;
    }
    if (previous.pdfUrl !== page.pdfUrl || previous.svgUrl !== page.svgUrl) {
      continue;
    }
    if (Object.prototype.hasOwnProperty.call(entries, page.pageId)) {
      retained[page.pageId] = mapEntry(entries[page.pageId]);
    }
  }
  return retained;
}

function sourceKey(source) {
  return source
    ? [
      source.itemId ?? "",
      source.pageId,
      source.file,
      source.startUtf8,
      source.endUtf8,
      source.startLine ?? "",
      source.endLine ?? "",
      source.sourceHash ?? ""
    ].join(":")
    : "";
}

function tileCacheKey(tile) {
  return `${tile.tile_x}:${tile.tile_y}`;
}

export function selectNearestSyncItem(syncMap, pageY, pageX = null) {
  if (!syncMap || !Array.isArray(syncMap.items) || syncMap.items.length === 0) {
    return null;
  }
  let closest = syncMap.items[0];
  let closestDistance = Number.POSITIVE_INFINITY;
  for (const item of syncMap.items) {
    const top = Math.min(item.top_px, item.bottom_px);
    const bottom = Math.max(item.top_px, item.bottom_px);
    const left = Math.min(item.left_px ?? 0, item.right_px ?? syncMap.page_width_px ?? 0);
    const right = Math.max(item.left_px ?? 0, item.right_px ?? syncMap.page_width_px ?? 0);
    let distance = 0;
    if (pageY < top) {
      distance = top - pageY;
    } else if (pageY > bottom) {
      distance = pageY - bottom;
    }
    if (typeof pageX === "number" && Number.isFinite(pageX) && right > left) {
      if (pageX < left) {
        distance += left - pageX;
      } else if (pageX > right) {
        distance += pageX - right;
      }
    }
    if (distance < closestDistance) {
      closest = item;
      closestDistance = distance;
    }
  }
  return closest;
}

export function normalizeSourceJumpRequest(fileOrRequest: any, offsetOrOptions?: any): SourceJumpRequest | null {
  if (typeof fileOrRequest === "string") {
    const parsedSourceHash = parseSourceHashRequest(fileOrRequest);
    if (parsedSourceHash) {
      return {
        ...parsedSourceHash,
        source_hash: fileOrRequest.startsWith("#") ? fileOrRequest : `#${fileOrRequest}`
      };
    }
    if (typeof offsetOrOptions === "number" && Number.isFinite(offsetOrOptions)) {
      return { file: fileOrRequest, offset: Math.max(0, Math.round(offsetOrOptions)), line: null };
    }
    if (
      offsetOrOptions
      && typeof offsetOrOptions === "object"
      && typeof offsetOrOptions.line === "number"
      && Number.isFinite(offsetOrOptions.line)
    ) {
      return {
        file: fileOrRequest,
        offset: null,
        line: Math.max(1, Math.round(offsetOrOptions.line)),
        ...(typeof offsetOrOptions.column === "number" && Number.isFinite(offsetOrOptions.column)
          ? { column: Math.max(1, Math.round(offsetOrOptions.column)) }
          : {})
      };
    }
    return null;
  }
  if (fileOrRequest && typeof fileOrRequest === "object" && typeof fileOrRequest.sourceHash === "string") {
    const parsedSourceHash = parseSourceHashRequest(fileOrRequest.sourceHash);
    return parsedSourceHash
      ? {
        ...parsedSourceHash,
        source_hash: fileOrRequest.sourceHash.startsWith("#")
          ? fileOrRequest.sourceHash
          : `#${fileOrRequest.sourceHash}`
      }
      : null;
  }
  if (fileOrRequest && typeof fileOrRequest === "object" && typeof fileOrRequest.source_hash === "string") {
    const parsedSourceHash = parseSourceHashRequest(fileOrRequest.source_hash);
    return parsedSourceHash
      ? {
        ...parsedSourceHash,
        source_hash: fileOrRequest.source_hash.startsWith("#")
          ? fileOrRequest.source_hash
          : `#${fileOrRequest.source_hash}`
      }
      : null;
  }
  if (!fileOrRequest || typeof fileOrRequest !== "object" || typeof fileOrRequest.file !== "string") {
    return null;
  }
  if (typeof fileOrRequest.offset === "number" && Number.isFinite(fileOrRequest.offset)) {
    return {
      file: fileOrRequest.file,
      offset: Math.max(0, Math.round(fileOrRequest.offset)),
      line: null
    };
  }
  if (typeof fileOrRequest.line === "number" && Number.isFinite(fileOrRequest.line)) {
    return {
      file: fileOrRequest.file,
      offset: null,
      line: Math.max(1, Math.round(fileOrRequest.line)),
      ...(typeof fileOrRequest.column === "number" && Number.isFinite(fileOrRequest.column)
        ? { column: Math.max(1, Math.round(fileOrRequest.column)) }
        : {})
    };
  }
  return null;
}

export function sourceRequestFromSelection(source: SourceSelection | null): SourceJumpRequest | null {
  if (!source || typeof source.file !== "string" || source.file.length === 0) {
    return null;
  }
  const canonicalHashRequest = typeof source.sourceHash === "string" && source.sourceHash.length > 0
    ? parseSourceHashRequest(source.sourceHash)
    : null;
  if (typeof source.startLine === "number" && Number.isFinite(source.startLine)) {
    return {
      file: source.file,
      offset: null,
      line: Math.max(1, Math.round(source.startLine)),
      ...(typeof source.startColumn === "number" && Number.isFinite(source.startColumn)
        ? { column: Math.max(1, Math.round(source.startColumn)) }
        : (
            canonicalHashRequest
            && canonicalHashRequest.file === source.file
            && canonicalHashRequest.line === Math.max(1, Math.round(source.startLine))
            && typeof canonicalHashRequest.column === "number"
          )
          ? { column: canonicalHashRequest.column }
        : {})
    };
  }
  if (typeof source.startUtf8 === "number" && Number.isFinite(source.startUtf8)) {
    return { file: source.file, offset: Math.max(0, Math.round(source.startUtf8)), line: null };
  }
  return null;
}

export function formatSourceSelectionHash(source: SourceSelection | null): string {
  if (typeof source?.sourceHash === "string" && source.sourceHash.length > 0) {
    return source.sourceHash;
  }
  return formatSourceRequestHash(sourceRequestFromSelection(source));
}

export function formatSourceRequestHash(request: Partial<SourceJumpRequest> | null): string {
  if (!request || typeof request.file !== "string" || request.file.length === 0) {
    return "";
  }
  const params = new URLSearchParams();
  params.set("src", request.file);
  if (typeof request.line === "number" && Number.isFinite(request.line)) {
    params.set("line", String(request.line));
    if (typeof request.column === "number" && Number.isFinite(request.column) && request.column > 1) {
      params.set("column", String(request.column));
    }
  } else if (typeof request.offset === "number" && Number.isFinite(request.offset)) {
    params.set("offset", String(request.offset));
  }
  return `#${params.toString()}`;
}

function sourceSelectionFromRequest(request: SourceJumpRequest | null): SourceSelection | null {
  if (!request || typeof request.file !== "string" || request.file.length === 0) {
    return null;
  }
  const source = request.line !== null
    ? {
      file: request.file,
      startLine: request.line,
      ...(typeof request.column === "number" && Number.isFinite(request.column)
        ? { startColumn: request.column }
        : {})
    }
    : request.offset !== null
      ? {
        file: request.file,
        startUtf8: request.offset
      }
      : {
        file: request.file
      };
  const sourceHash = typeof request.source_hash === "string" && request.source_hash.length > 0
    ? request.source_hash
    : formatSourceSelectionHash(source);
  return sourceHash
    ? {
      ...source,
      sourceHash
    }
    : source;
}

function syncItemId(pageId, item) {
  if (!pageId || !item || typeof item.file !== "string" || item.file.length === 0) {
    return "";
  }
  return [
    pageId,
    item.file,
    item.start_utf8 ?? item.startUtf8 ?? "",
    item.end_utf8 ?? item.endUtf8 ?? "",
    item.start_line ?? item.startLine ?? "",
    item.end_line ?? item.endLine ?? ""
  ].join(":");
}

function sourceSelectionItemId(source) {
  if (typeof source?.itemId === "string" && source.itemId.length > 0) {
    return source.itemId;
  }
  if (!source || typeof source.file !== "string" || source.file.length === 0) {
    return "";
  }
  return [
    source.pageId ?? "",
    source.file,
    source.startUtf8 ?? "",
    source.endUtf8 ?? "",
    source.startLine ?? "",
    source.endLine ?? ""
  ].join(":");
}

export function parseSourceHashRequest(hash: string): SourceJumpRequest | null {
  if (typeof hash !== "string" || hash.length === 0) {
    return null;
  }
  const params = new URLSearchParams(hash.startsWith("#") ? hash.slice(1) : hash);
  const file = params.get("src");
  if (!file) {
    return null;
  }
  if (params.has("offset")) {
    return normalizeSourceJumpRequest({
      file,
      offset: Number(params.get("offset"))
    });
  }
  if (params.has("line")) {
    return normalizeSourceJumpRequest({
      file,
      line: Number(params.get("line")),
      ...(params.has("column") ? { column: Number(params.get("column")) } : {})
    });
  }
  return null;
}

function sourceLineRangeFromSelection(content, source) {
  if (!content || !source) {
    return null;
  }
  if (typeof source.startLine === "number" && Number.isFinite(source.startLine)) {
    return {
      startLine: Math.max(1, Math.round(source.startLine)),
      endLine: typeof source.endLine === "number" && Number.isFinite(source.endLine)
        ? Math.max(Math.round(source.startLine), Math.round(source.endLine))
        : Math.max(1, Math.round(source.startLine))
    };
  }
  if (typeof source.startUtf8 !== "number" || !Number.isFinite(source.startUtf8)) {
    return null;
  }
  const startOffset = Math.max(0, Math.min(content.length, Math.round(source.startUtf8)));
  const endOffset = typeof source.endUtf8 === "number" && Number.isFinite(source.endUtf8)
    ? Math.max(startOffset, Math.min(content.length, Math.round(source.endUtf8)))
    : startOffset;
  const lineForOffset = (offset) => {
    let line = 1;
    for (let index = 0; index < offset; index += 1) {
      if (content.charCodeAt(index) === 10) {
        line += 1;
      }
    }
    return line;
  };
  return {
    startLine: lineForOffset(startOffset),
    endLine: lineForOffset(endOffset)
  };
}

function syncSelectionFromItem(pageId, syncMap, item) {
  if (!syncMap || !item) {
    return null;
  }
  return {
    itemId: typeof item.item_id === "string" && item.item_id.length > 0
      ? item.item_id
      : syncItemId(pageId, item),
    pageId,
    pageWidthPx: syncMap.page_width_px,
    pageHeightPx: syncMap.page_height_px,
    file: item.file,
    startUtf8: item.start_utf8,
    endUtf8: item.end_utf8,
    outputStartUtf8: item.output_start_utf8 ?? (syncMap.page_output_start_utf8 ?? 0),
    outputEndUtf8: item.output_end_utf8 ?? (syncMap.page_output_end_utf8 ?? 0),
    pageSourceStartUtf8: syncMap.page_source_start_utf8 ?? 0,
    pageSourceEndUtf8: syncMap.page_source_end_utf8 ?? 0,
    pageOutputStartUtf8: syncMap.page_output_start_utf8 ?? 0,
    pageOutputEndUtf8: syncMap.page_output_end_utf8 ?? 0,
    startLine: item.start_line ?? null,
    endLine: item.end_line ?? null,
    leftPx: item.left_px ?? 0,
    rightPx: item.right_px ?? syncMap.page_width_px,
    topPx: item.top_px,
    bottomPx: item.bottom_px
  };
}

export function syncSelectionFromJumpContext(context: any): SourceSelection | null {
  if (!context || typeof context.page_id !== "string" || !context.item) {
    return null;
  }
  const selection = syncSelectionFromItem(
    context.page_id,
    {
      page_width_px: context.page_width_px ?? 0,
      page_height_px: context.page_height_px ?? 0,
      page_source_start_utf8: context.page_source_start_utf8 ?? context.item.start_utf8 ?? 0,
      page_source_end_utf8: context.page_source_end_utf8 ?? context.item.end_utf8 ?? context.item.start_utf8 ?? 0,
      page_output_start_utf8: context.page_output_start_utf8 ?? context.item.output_start_utf8 ?? 0,
      page_output_end_utf8: context.page_output_end_utf8 ?? context.item.output_end_utf8 ?? context.item.output_start_utf8 ?? 0
    },
    context.item
  );
  if (!selection) {
    return null;
  }
  return typeof context.source_hash === "string" && context.source_hash.length > 0
    ? {
      ...selection,
      sourceHash: context.source_hash
    }
    : selection;
}

export function resolvedSourceRequestDetail(
  rev: number,
  request: (({ source_hash?: string; launch?: boolean }) & Partial<SourceJumpRequest>) | null,
  response: any,
  source: SourceSelection | null = null
) {
  const item = syncSelectionFromJumpContext(response);
  const previewOnly = request?.launch === false;
  return {
    rev,
    ...(source ? { source } : {}),
    request,
    response,
    item,
    absoluteFile: typeof response?.absolute_file === "string" ? response.absolute_file : "",
    fileUri: typeof response?.file_uri === "string" ? response.file_uri : "",
    line: Number.isInteger(response?.line) ? response.line : 1,
    line0: Number.isInteger(response?.line0) ? response.line0 : 0,
    column: Number.isInteger(response?.column) ? response.column : 1,
    column0: Number.isInteger(response?.column0) ? response.column0 : 0,
    editorCwd: typeof response?.editor_cwd === "string" ? response.editor_cwd : "",
    editorLaunchSupported: response?.editor_launch_supported === true,
    editorPreviewKind: typeof response?.editor_preview_kind === "string"
      ? response.editor_preview_kind
      : "none",
    editorProgram: typeof response?.editor_program === "string" ? response.editor_program : "",
    editorArgs: Array.isArray(response?.editor_args) ? response.editor_args.slice() : [],
    editorCommandLine: typeof response?.editor_command_line === "string" ? response.editor_command_line : "",
    editorUri: typeof response?.editor_uri === "string" ? response.editor_uri : "",
    launched: response?.launched === true,
    launchRequested: !previewOnly,
    previewOnly,
    sourceHash: typeof response?.source_hash === "string" && response.source_hash.length > 0
      ? response.source_hash
      : typeof request?.source_hash === "string" && request.source_hash.length > 0
        ? request.source_hash
        : formatSourceSelectionHash(item ?? source)
  };
}

function sourceSelectionFitsSyncWindow(source, syncMap) {
  if (!source || !syncMap) {
    return false;
  }
  const pageSourceStart = syncMap.page_source_start_utf8 ?? 0;
  const pageSourceEnd = syncMap.page_source_end_utf8 ?? pageSourceStart;
  const pageOutputStart = syncMap.page_output_start_utf8 ?? 0;
  const pageOutputEnd = syncMap.page_output_end_utf8 ?? pageOutputStart;
  if (typeof source.startUtf8 === "number" && Number.isFinite(source.startUtf8) && source.startUtf8 < pageSourceStart) {
    return false;
  }
  if (typeof source.endUtf8 === "number" && Number.isFinite(source.endUtf8) && source.endUtf8 > pageSourceEnd) {
    return false;
  }
  if (
    typeof source.outputStartUtf8 === "number"
    && Number.isFinite(source.outputStartUtf8)
    && source.outputStartUtf8 < pageOutputStart
  ) {
    return false;
  }
  if (
    typeof source.outputEndUtf8 === "number"
    && Number.isFinite(source.outputEndUtf8)
    && source.outputEndUtf8 > pageOutputEnd
  ) {
    return false;
  }
  if (typeof source.leftPx === "number" && Number.isFinite(source.leftPx) && source.leftPx > syncMap.page_width_px) {
    return false;
  }
  if (typeof source.rightPx === "number" && Number.isFinite(source.rightPx) && source.rightPx < 0) {
    return false;
  }
  if (typeof source.topPx === "number" && Number.isFinite(source.topPx) && source.topPx > syncMap.page_height_px) {
    return false;
  }
  if (typeof source.bottomPx === "number" && Number.isFinite(source.bottomPx) && source.bottomPx < 0) {
    return false;
  }
  return true;
}

function reanchorSourceSelection(source, pageId, syncMap) {
  if (!source || source.pageId !== pageId) {
    return source;
  }
  const desiredItemId = sourceSelectionItemId(source);
  if (desiredItemId) {
    const matchingItem = syncMap.items.find((item) => syncItemId(pageId, item) === desiredItemId);
    if (matchingItem) {
      const nextSelection = syncSelectionFromItem(pageId, syncMap, matchingItem);
      return typeof source.sourceHash === "string" && source.sourceHash.length > 0
        ? {
          ...nextSelection,
          sourceHash: source.sourceHash
        }
        : nextSelection;
    }
  }
  return sourceSelectionFitsSyncWindow(source, syncMap) ? source : null;
}

export function reduce(state, message) {
  switch (message.type) {
    case "build_started":
      if (message.rev < state.currentRev) {
        return state;
      }
      return {
        ...state,
        currentRev: message.rev,
        pendingPagePatchOps: [],
        building: true,
        changedFiles: message.changed_files
      };
    case "diagnostics":
      if (message.rev < state.currentRev) {
        return state;
      }
      return {
        ...state,
        currentRev: message.rev,
        diagnostics: message.items
      };
    case "full_pdf_ready":
      if (message.rev < state.currentRev) {
        return state;
      }
      {
        const pageArtifacts = message.page_artifacts ?? [];
        const pageIds = pageArtifacts.length > 0
          ? pageArtifacts.map((page) => page.page_id)
          : message.page_ids ?? state.pageIds;
        const pages = state.pendingPagePatchOps.length > 0
          ? reconcilePagesAfterPatch(state.pages, pageArtifacts)
          : pageArtifacts.map((page) => ({
            pageId: page.page_id,
            pdfUrl: page.pdf_url,
            svgUrl: page.svg_url ?? null
          }));
        const syncMaps = retainPageEntries(
          state.pages,
          pages,
          state.syncMaps,
          (entry) => ({ ...entry, rev: message.rev })
        );
        const tileLayers = retainPageEntries(state.pages, pages, state.tileLayers);
        const selectedSource = state.selectedSource?.pageId
          && Object.prototype.hasOwnProperty.call(syncMaps, state.selectedSource.pageId)
          ? state.selectedSource
          : null;
        const hoveredSource = state.hoveredSource?.pageId
          && Object.prototype.hasOwnProperty.call(syncMaps, state.hoveredSource.pageId)
          ? state.hoveredSource
          : null;
        return {
          ...state,
          currentRev: message.rev,
          lastAppliedRev: message.rev,
          pdfUrl: message.pdf_url,
          pageIds,
          pages,
          syncMaps,
          sourceFiles: {},
          tileLayers,
          pendingPagePatchOps: [],
          currentPage: pageIds.length > 0
            ? Math.min(state.currentPage, pageIds.length)
            : state.currentPage,
          hoveredSource,
          selectedSource,
          building: false,
          lastBuildSucceeded: true
        };
      }
    case "patch_pages":
      if (message.rev < state.currentRev) {
        return state;
      }
      {
        const pages = state.pages.length > 0
          ? state.pages.map((page) => ({ ...page }))
          : [];
        for (const op of message.ops) {
          if (op.op === "replace_page") {
            if (op.index >= 0 && op.index < pages.length) {
              pages[op.index] = { pageId: op.page_id, pdfUrl: op.pdf_url };
              pages[op.index].svgUrl = op.svg_url ?? null;
            }
          } else if (op.op === "insert_page") {
            if (op.index >= 0 && op.index <= pages.length) {
              pages.splice(op.index, 0, {
                pageId: op.page_id,
                pdfUrl: op.pdf_url,
                svgUrl: op.svg_url ?? null
              });
            }
          } else if (op.op === "delete_page") {
            if (op.index >= 0 && op.index < pages.length) {
              pages.splice(op.index, 1);
            }
          }
        }
        const pageIds = pages.map((page) => page.pageId);
        const syncMaps = retainPageEntries(
          state.pages,
          pages,
          state.syncMaps,
          (entry) => ({ ...entry, rev: message.rev })
        );
        const tileLayers = retainPageEntries(state.pages, pages, state.tileLayers);
        const selectedSource = state.selectedSource?.pageId
          && Object.prototype.hasOwnProperty.call(syncMaps, state.selectedSource.pageId)
          ? state.selectedSource
          : null;
        const hoveredSource = state.hoveredSource?.pageId
          && Object.prototype.hasOwnProperty.call(syncMaps, state.hoveredSource.pageId)
          ? state.hoveredSource
          : null;
        return {
          ...state,
          currentRev: message.rev,
          pageIds,
          pages,
          syncMaps,
          tileLayers,
          pendingPagePatchOps: [...state.pendingPagePatchOps, ...message.ops],
          currentPage: pageIds.length > 0
            ? Math.min(state.currentPage, pageIds.length)
            : state.currentPage,
          hoveredSource,
          selectedSource
        };
      }
    case "build_finished":
      if (message.rev < state.currentRev) {
        return state;
      }
      return {
        ...state,
        currentRev: message.rev,
        building: false,
        lastBuildSucceeded: message.success
      };
    case "ui_page_changed":
      {
        return {
          ...state,
          currentPage: state.pageIds.length > 0
            ? Math.max(1, Math.min(state.pageIds.length, message.page))
            : Math.max(1, message.page)
        };
      }
    case "ui_zoom_changed":
      return {
        ...state,
        zoom: Math.max(0.5, Math.min(3, message.zoom)),
        tileLayers: {}
      };
    case "ui_scroll_changed":
      return {
        ...state,
        scrollTop: Math.max(0, message.scrollTop)
      };
    case "ui_tiles_ready":
      if (message.rev < state.currentRev) {
        return state;
      }
      if (message.rev !== state.lastAppliedRev) {
        return state;
      }
      if (zoomBucketForZoom(state.zoom) !== message.zoom_bucket) {
        return state;
      }
      if (!pageExistsForState(state, message.page_id)) {
        return state;
      }
      {
        const previousLayer = state.tileLayers[message.page_id];
        const items = previousLayer
          && previousLayer.zoomBucket === message.zoom_bucket
          && previousLayer.tileSize === message.tile_size
          ? (() => {
            const merged = new Map<string, any>(
              previousLayer.items.map((item) => [tileCacheKey(item), item])
            );
            for (const item of message.items) {
              merged.set(tileCacheKey(item), item);
            }
            return [...merged.values()].sort((left, right) =>
              left.tile_y - right.tile_y || left.tile_x - right.tile_x
            );
          })()
          : message.items;
        return {
          ...state,
          tileLayers: {
            ...state.tileLayers,
            [message.page_id]: {
              zoomBucket: message.zoom_bucket,
              tileSize: message.tile_size,
              items
            }
          }
        };
      }
    case "ui_syncmap_ready":
      if (message.rev < state.currentRev) {
        return state;
      }
      if (message.rev !== state.lastAppliedRev) {
        return state;
      }
      if (!pageExistsForState(state, message.page_id)) {
        return state;
      }
      {
        const syncMap = {
          rev: message.rev,
          page_width_px: message.page_width_px,
          page_height_px: message.page_height_px,
          page_source_start_utf8: message.page_source_start_utf8 ?? 0,
          page_source_end_utf8: message.page_source_end_utf8 ?? 0,
          page_output_start_utf8: message.page_output_start_utf8 ?? 0,
          page_output_end_utf8: message.page_output_end_utf8 ?? 0,
          items: message.items
        };
        const selectedSource = reanchorSourceSelection(state.selectedSource, message.page_id, syncMap);
        const hoveredSource = reanchorSourceSelection(state.hoveredSource, message.page_id, syncMap);
        return {
          ...state,
          selectedSource,
          hoveredSource,
          syncMaps: {
            ...state.syncMaps,
            [message.page_id]: syncMap
          }
        };
      }
    case "ui_source_file_ready":
      if (message.rev < state.currentRev) {
        return state;
      }
      if (message.rev !== state.lastAppliedRev) {
        return state;
      }
      return {
        ...state,
        sourceFiles: {
          ...state.sourceFiles,
          [message.file]: {
            rev: message.rev,
            content: message.content,
            lineCount: message.line_count
          }
        }
      };
    case "source_snapshot":
      if (message.rev < state.currentRev) {
        return state;
      }
      if (message.rev !== state.lastAppliedRev) {
        return state;
      }
      {
        const sourceFiles = sourceFilesFromSnapshot(
          (message.files ?? []).map((entry) => ({
            ...entry,
            rev: message.rev
          }))
        );
        return {
          ...state,
          sourceFiles
        };
      }
    case "ui_sync_hovered":
      return {
        ...state,
        hoveredSource: message.item
      };
    case "ui_sync_selected":
      return {
        ...state,
        selectedSource: message.item
      };
    case "ui_source_jump_resolved":
      if (!pageExistsForState(state, message.page_id)) {
        return state;
      }
      return {
        ...state,
        currentPage: Math.max(1, Math.min(state.pageIds.length, message.page_index + 1)),
        hoveredSource: null,
        selectedSource: message.item
      };
    case "ui_open_source_resolved":
      if (!pageExistsForState(state, message.page_id)) {
        return state;
      }
      return {
        ...state,
        currentPage: Math.max(1, Math.min(state.pageIds.length, message.page_index + 1)),
        hoveredSource: null,
        selectedSource: message.item
      };
    case "ui_source_hover_resolved":
      if (!pageExistsForState(state, message.page_id)) {
        return state;
      }
      return {
        ...state,
        currentPage: Math.max(1, Math.min(state.pageIds.length, message.page_index + 1)),
        hoveredSource: message.item
      };
    default:
      return state;
  }
}

function reconcilePagesAfterPatch(previousPages, nextPageArtifacts) {
  const previousById = new Map(
    previousPages
      .filter((page) => page.pdfUrl)
      .map((page) => [page.pageId, page])
  );

  return nextPageArtifacts.map((page) => {
    const previous = previousById.get(page.page_id);
    if (previous) {
      return previous;
    }
    return {
      pageId: page.page_id,
      pdfUrl: page.pdf_url,
      svgUrl: page.svg_url ?? null
    };
  });
}

export function mountViewer(root: HTMLElement, options: ViewerMountOptions) {
  const viewerWindow = (options.window ?? window) as (Window & typeof globalThis & Record<string, any>);
  const viewerDocument = options.document ?? document;
  const emitEvent = (event: ViewerEvent) => {
    options.onEvent?.(event);
  };
  const transport = options.transport;
  const shadowRoot = root.shadowRoot ?? root.attachShadow({ mode: "open" });

  shadowRoot.innerHTML = `<style>${VIEWER_STYLE}</style>${VIEWER_TEMPLATE}`;

  const getRequiredElement = <T extends Element>(selector: string) => {
    const element = shadowRoot.querySelector(selector);
    if (!element) {
      throw new Error(`latexd viewer mount missing required element: ${selector}`);
    }
    return element as T;
  };
  const elements = {
    revision: getRequiredElement<HTMLElement>("#revision"),
    buildStatus: getRequiredElement<HTMLElement>("#build-status"),
    changedFiles: getRequiredElement<HTMLElement>("#changed-files"),
    diagnostics: getRequiredElement<HTMLElement>("#diagnostics"),
    sourceStatus: getRequiredElement<HTMLElement>("#source-status"),
    sourceFile: getRequiredElement<HTMLElement>("#source-file"),
    sourceSelection: getRequiredElement<HTMLElement>("#source-selection"),
    sourceViewer: getRequiredElement<HTMLElement>("#source-viewer"),
    sourceOpen: getRequiredElement<HTMLButtonElement>("#source-open"),
    sourceLink: getRequiredElement<HTMLAnchorElement>("#source-link"),
    pageLabel: getRequiredElement<HTMLElement>("#page-label"),
    zoomLabel: getRequiredElement<HTMLElement>("#zoom-label"),
    frame: getRequiredElement<HTMLElement>("#frame"),
    placeholder: getRequiredElement<HTMLElement>("#placeholder"),
    preview: getRequiredElement<HTMLIFrameElement>("#preview"),
    previewStack: getRequiredElement<HTMLElement>("#preview-stack"),
    prevPage: getRequiredElement<HTMLButtonElement>("#prev-page"),
    nextPage: getRequiredElement<HTMLButtonElement>("#next-page"),
    zoomOut: getRequiredElement<HTMLButtonElement>("#zoom-out"),
    zoomIn: getRequiredElement<HTMLButtonElement>("#zoom-in")
  };

  let state = initialState;
  const pageNodes = new Map();
  const pageMetrics = new Map();
  const TILE_SIZE = 256;
  const FALLBACK_PAGE_WIDTH = 612;
  const FALLBACK_PAGE_HEIGHT = 792;
  let tileRefreshHandle = 0;
  const tileRequestSerials = new Map();
  const tileRequestKeys = new Map();
  const syncMapInflight = new Map();
  const sourceFileInflight = new Map();
  const syncInteractionSerials = new Map();
  let lastViewportPayload = null;
  const scrollPageIntoFrame = (pageId) => {
    const pageNode = pageNodes.get(pageId);
    if (!pageNode) {
      return;
    }
    const frameRect = elements.frame.getBoundingClientRect();
    const pageRect = pageNode.getBoundingClientRect();
    if (frameRect.height <= 0 || pageRect.height <= 0) {
      return;
    }
    const targetTop =
      elements.frame.scrollTop
      + (pageRect.top - frameRect.top)
      - Math.max(0, (frameRect.height - pageRect.height) / 2);
    if (typeof elements.frame.scrollTo === "function") {
      elements.frame.scrollTo({
        top: Math.max(0, targetTop)
      });
      return;
    }
    elements.frame.scrollTop = Math.max(0, targetTop);
  };

  const queueTileRefresh = () => {
    if (tileRefreshHandle !== 0) {
      return;
    }
    tileRefreshHandle = viewerWindow.requestAnimationFrame(() => {
      tileRefreshHandle = 0;
      if (state.lastAppliedRev < 1) {
        return;
      }
      const frameRect = elements.frame.getBoundingClientRect();
      const zoomBucket = zoomBucketForZoom(state.zoom);
      const visiblePages = [];
      for (const page of state.pages) {
        if (!page.svgUrl) {
          continue;
        }
        const node = pageNodes.get(page.pageId);
        const stage = node?.querySelector(".page-stage");
        if (!stage || stage.hidden) {
          continue;
        }
        const stageRect = stage.getBoundingClientRect();
        const left = Math.max(0, Math.round(frameRect.left - stageRect.left));
        const top = Math.max(0, Math.round(frameRect.top - stageRect.top));
        const right = Math.min(Math.round(stageRect.width), Math.round(frameRect.right - stageRect.left));
        const bottom = Math.min(Math.round(stageRect.height), Math.round(frameRect.bottom - stageRect.top));
        if (right <= left || bottom <= top) {
          continue;
        }
        visiblePages.push(page.pageId);
        const startTileX = Math.floor(left / TILE_SIZE);
        const endTileX = Math.floor((right - 1) / TILE_SIZE);
        const startTileY = Math.floor(top / TILE_SIZE);
        const endTileY = Math.floor((bottom - 1) / TILE_SIZE);
        const existingLayer = state.tileLayers[page.pageId];
        if (
          existingLayer
          && existingLayer.zoomBucket === zoomBucket
          && existingLayer.tileSize === TILE_SIZE
        ) {
          const cachedTiles = new Set(existingLayer.items.map((item) => tileCacheKey(item)));
          let hasAllTiles = true;
          for (let tileY = startTileY; tileY <= endTileY && hasAllTiles; tileY += 1) {
            for (let tileX = startTileX; tileX <= endTileX; tileX += 1) {
              if (!cachedTiles.has(`${tileX}:${tileY}`)) {
                hasAllTiles = false;
                break;
              }
            }
          }
          if (hasAllTiles) {
            continue;
          }
        }
        const requestKey = [
          state.lastAppliedRev,
          page.pageId,
          zoomBucket,
          startTileX,
          endTileX,
          startTileY,
          endTileY
        ].join(":");
        if (tileRequestKeys.get(page.pageId) === requestKey) {
          continue;
        }
        tileRequestKeys.set(page.pageId, requestKey);
        const requestRev = state.lastAppliedRev;
        const requestPageId = page.pageId;
        const requestSerial = (tileRequestSerials.get(requestPageId) ?? 0) + 1;
        tileRequestSerials.set(requestPageId, requestSerial);
        transport.fetchTileManifest({
          rev: requestRev,
          pageId: requestPageId,
          scale: state.zoom,
          left: startTileX * TILE_SIZE,
          top: startTileY * TILE_SIZE,
          width: (endTileX - startTileX + 1) * TILE_SIZE,
          height: (endTileY - startTileY + 1) * TILE_SIZE,
          tileSize: TILE_SIZE
        })
          .then((manifest) => {
            if (tileRequestSerials.get(requestPageId) !== requestSerial) {
              return;
            }
            if (state.lastAppliedRev !== requestRev) {
              return;
            }
            if (!pageExistsForState(state, requestPageId)) {
              return;
            }
            if (zoomBucketForZoom(state.zoom) !== zoomBucket) {
              return;
            }
            state = reduce(state, {
              type: "ui_tiles_ready",
              rev: manifest.rev,
              page_id: manifest.page_id,
              zoom_bucket: zoomBucket,
              tile_size: manifest.tile_size,
              items: manifest.items
            });
            render();
          })
          .catch(() => {
            if (
              tileRequestSerials.get(requestPageId) === requestSerial
              && tileRequestKeys.get(requestPageId) === requestKey
            ) {
              tileRequestKeys.delete(requestPageId);
            }
          });
      }
      const payload = JSON.stringify({
        type: "viewport_changed",
        zoom: state.zoom,
        current_page: state.currentPage,
        scroll_top: elements.frame.scrollTop,
        visible_pages: visiblePages
      });
      if (socket.readyState === 1 && payload !== lastViewportPayload) {
        lastViewportPayload = payload;
        socket.send(payload);
      }
    });
  };

  const render = () => {
    elements.revision.textContent = String(state.currentRev);
    elements.pageLabel.textContent = state.pageIds.length > 0
      ? `${state.currentPage} / ${state.pageIds.length}`
      : String(state.currentPage);
    elements.zoomLabel.textContent = `${Math.round(state.zoom * 100)}%`;
    elements.changedFiles.textContent = state.changedFiles.length > 0
      ? `Changed: ${state.changedFiles.join(", ")}`
      : "No file changes yet";
    if (state.building) {
      elements.buildStatus.textContent = "Building...";
    } else if (state.lastBuildSucceeded === true) {
      elements.buildStatus.textContent = "Last build succeeded";
    } else if (state.lastBuildSucceeded === false) {
      elements.buildStatus.textContent = "Last build failed; keeping previous preview";
    } else {
      elements.buildStatus.textContent = "Waiting for first build";
    }

    const focusedSource = state.selectedSource ?? state.hoveredSource;
    if (state.selectedSource) {
      elements.sourceStatus.textContent = "Selected source span";
    } else if (state.hoveredSource) {
      elements.sourceStatus.textContent = "Hovering source span";
    } else {
      elements.sourceStatus.textContent = "Click or hover the preview to inspect source spans";
    }
    elements.sourceSelection.textContent = focusedSource
      ? focusedSource.startLine
        ? `${focusedSource.file}:${focusedSource.startLine}${focusedSource.endLine && focusedSource.endLine !== focusedSource.startLine ? `-${focusedSource.endLine}` : ""} (${focusedSource.startUtf8}-${focusedSource.endUtf8}) on ${focusedSource.pageId}`
        : `${focusedSource.file}:${focusedSource.startUtf8}-${focusedSource.endUtf8} on ${focusedSource.pageId}`
      : "No source span selected";
    const sourceRequest = sourceRequestFromSelection(focusedSource);
    const sourceHash = formatSourceSelectionHash(focusedSource);
    elements.sourceOpen.disabled = !sourceRequest;
    elements.sourceLink.hidden = !sourceHash;
    elements.sourceLink.href = sourceHash || "#";
    elements.sourceFile.textContent = focusedSource?.file ?? "No source file loaded";

    const focusedFileEntry = focusedSource ? state.sourceFiles[focusedSource.file] ?? null : null;
    if (focusedSource?.file && !focusedFileEntry && state.lastAppliedRev > 0) {
      ensureSourceFile(focusedSource.file);
    }
    if (!focusedSource) {
      elements.sourceViewer.replaceChildren();
      const empty = viewerDocument.createElement("div");
      empty.className = "source-line source-line--empty";
      empty.textContent = "Select a source span to inspect nearby source lines.";
      elements.sourceViewer.replaceChildren(empty);
    } else if (!focusedFileEntry) {
      elements.sourceViewer.replaceChildren();
      const loading = viewerDocument.createElement("div");
      loading.className = "source-line source-line--empty";
      loading.textContent = "Loading source…";
      elements.sourceViewer.replaceChildren(loading);
    } else {
      const selectedRange = state.selectedSource?.file === focusedSource.file
        ? sourceLineRangeFromSelection(focusedFileEntry.content, state.selectedSource)
        : null;
      const hoveredRange = state.hoveredSource?.file === focusedSource.file
        ? sourceLineRangeFromSelection(focusedFileEntry.content, state.hoveredSource)
        : null;
      const activeRange = selectedRange ?? hoveredRange ?? sourceLineRangeFromSelection(
        focusedFileEntry.content,
        focusedSource
      );
      const lines = focusedFileEntry.content.split("\n");
      const centerLine = activeRange?.startLine ?? 1;
      const windowStart = Math.max(1, centerLine - 8);
      const windowEnd = Math.min(lines.length, (activeRange?.endLine ?? centerLine) + 8);
      const fragment = viewerDocument.createDocumentFragment();
      if (windowStart > 1) {
        const skipped = viewerDocument.createElement("div");
        skipped.className = "source-line source-line--empty";
        skipped.textContent = `… ${windowStart - 1} line(s) above`;
        fragment.append(skipped);
      }
      for (let lineNumber = windowStart; lineNumber <= windowEnd; lineNumber += 1) {
        const line = viewerDocument.createElement("button") as HTMLButtonElement;
        line.type = "button";
        line.className = "source-line";
        line.dataset.selected = String(
          !!selectedRange
          && lineNumber >= selectedRange.startLine
          && lineNumber <= selectedRange.endLine
        );
        line.dataset.hovered = String(
          !!hoveredRange
          && lineNumber >= hoveredRange.startLine
          && lineNumber <= hoveredRange.endLine
        );
        line.innerHTML = `
          <span class="source-line__number">${lineNumber}</span>
          <span class="source-line__text"></span>
        `;
        const lineText = line.querySelector(".source-line__text") as HTMLElement | null;
        if (lineText) {
          lineText.textContent = lines[lineNumber - 1] || " ";
        }
        line.addEventListener("click", () => {
          resolveSourceJump({ file: focusedSource.file, line: lineNumber });
        });
        line.addEventListener("mouseenter", () => {
          hoverSourceJump({ file: focusedSource.file, line: lineNumber });
        });
        fragment.append(line);
      }
      if (windowEnd < lines.length) {
        const skipped = viewerDocument.createElement("div");
        skipped.className = "source-line source-line--empty";
        skipped.textContent = `… ${lines.length - windowEnd} line(s) below`;
        fragment.append(skipped);
      }
      elements.sourceViewer.replaceChildren(fragment);
    }

    elements.diagnostics.textContent = "";
    for (const item of state.diagnostics) {
      const block = viewerDocument.createElement("div");
      block.className = "diagnostic";
      const file = item.file ? `${item.file}${item.line ? `:${item.line}` : ""}` : "project";
      block.textContent = `[${item.level}] ${file} — ${item.message}`;
      elements.diagnostics.append(block);
    }

    if (!state.pdfUrl || state.pages.length === 0) {
      if (!state.pdfUrl) {
        elements.preview.hidden = true;
        elements.previewStack.hidden = true;
        elements.placeholder.hidden = false;
        return;
      }
      const nextSrc = `${state.pdfUrl}#page=${state.currentPage}&zoom=${Math.round(state.zoom * 100)}`;
      if (elements.preview.dataset.src !== nextSrc) {
        elements.preview.dataset.src = nextSrc;
        elements.preview.src = nextSrc;
      }
      elements.preview.hidden = false;
      elements.previewStack.hidden = true;
      elements.placeholder.hidden = true;
      return;
    }

    const fragment = viewerDocument.createDocumentFragment();
    const activePageId = activePageForState(state)?.pageId ?? null;
    const zoomBucket = zoomBucketForZoom(state.zoom);
    for (const [index, page] of state.pages.entries()) {
      let node = pageNodes.get(page.pageId);
      if (!node) {
        node = viewerDocument.createElement("article");
        node.className = "page-card";
        node.innerHTML = `
          <div class="page-card__meta">
            <span class="page-card__label"></span>
            <span class="page-card__id"></span>
          </div>
          <div class="page-stage" hidden>
            <img class="page-image" alt="">
            <div class="page-tiles"></div>
            <div class="page-sync-marker page-sync-marker--selected" hidden></div>
            <div class="page-sync-marker page-sync-marker--hover" hidden></div>
          </div>
          <iframe class="page-frame" loading="lazy"></iframe>
        `;
        const stage = node.querySelector(".page-stage");
        const image = node.querySelector(".page-image");
        image.addEventListener("load", () => {
          const pageId = node.dataset.pageId;
          if (!pageId) {
            return;
          }
          const width = image.naturalWidth || image.width;
          const height = image.naturalHeight || image.height;
          if (width > 0 && height > 0) {
            pageMetrics.set(pageId, { width, height });
            render();
          }
        });
        stage.addEventListener("mousemove", (event) => {
          const pageId = node.dataset.pageId;
          if (pageId) {
            updateHoverSelectionForPage(pageId, event.clientX, event.clientY);
          }
        });
        stage.addEventListener("mouseleave", () => {
          const pageId = node.dataset.pageId;
          if (pageId) {
            syncInteractionSerials.set(
              `hover:${pageId}`,
              (syncInteractionSerials.get(`hover:${pageId}`) ?? 0) + 1
            );
          }
          dispatch({ type: "ui_sync_hovered", item: null });
        });
        stage.addEventListener("click", (event) => {
          const pageId = node.dataset.pageId;
          if (pageId) {
            updateSelectedSourceForPage(pageId, event.clientX, event.clientY);
          }
        });
        pageNodes.set(page.pageId, node);
      }
      node.dataset.pageId = page.pageId;
      node.dataset.active = String(page.pageId === activePageId);
      node.querySelector(".page-card__label").textContent = `Page ${index + 1}`;
      node.querySelector(".page-card__id").textContent = page.pageId.slice(0, 12);
      const stage = node.querySelector(".page-stage");
      const image = node.querySelector(".page-image");
      const tiles = node.querySelector(".page-tiles");
      const selectedMarker = node.querySelector(".page-sync-marker--selected");
      const hoverMarker = node.querySelector(".page-sync-marker--hover");
      const frame = node.querySelector(".page-frame");
      if (page.svgUrl) {
        if (image.dataset.src !== page.svgUrl) {
          image.dataset.src = page.svgUrl;
          image.src = page.svgUrl;
        }
        const size = pageMetrics.get(page.pageId) ?? {
          width: FALLBACK_PAGE_WIDTH,
          height: FALLBACK_PAGE_HEIGHT
        };
        stage.style.width = `${Math.round(size.width * state.zoom)}px`;
        stage.style.height = `${Math.round(size.height * state.zoom)}px`;
        const applyMarker = (marker, source) => {
          if (!source || source.pageId !== page.pageId || source.pageHeightPx < 1) {
            marker.hidden = true;
            return;
          }
          const topPercent = (Math.min(source.topPx, source.bottomPx) / source.pageHeightPx) * 100;
          const bottomPercent = (Math.max(source.topPx, source.bottomPx) / source.pageHeightPx) * 100;
          const leftPercent = source.pageWidthPx > 0
            ? (Math.min(source.leftPx ?? 0, source.rightPx ?? source.pageWidthPx) / source.pageWidthPx) * 100
            : 0;
          const rightPercent = source.pageWidthPx > 0
            ? (Math.max(source.leftPx ?? 0, source.rightPx ?? source.pageWidthPx) / source.pageWidthPx) * 100
            : 100;
          marker.style.left = `${Math.max(0, Math.min(100, leftPercent))}%`;
          marker.style.width = `${Math.max(
            1 / Math.max(source.pageWidthPx ?? 1, 1) * 100,
            rightPercent - leftPercent
          )}%`;
          marker.style.top = `${Math.max(0, Math.min(100, topPercent))}%`;
          marker.style.height = `${Math.max(
            1 / Math.max(source.pageHeightPx, 1) * 100,
            bottomPercent - topPercent
          )}%`;
          marker.hidden = false;
        };
        applyMarker(selectedMarker, state.selectedSource);
        applyMarker(hoverMarker, state.hoveredSource);
        const tileLayer = state.tileLayers[page.pageId];
        if (tileLayer && tileLayer.zoomBucket === zoomBucket) {
          const tileFragment = viewerDocument.createDocumentFragment();
          for (const tile of tileLayer.items) {
            const tileNode = viewerDocument.createElement("img");
            tileNode.className = "page-tile";
            tileNode.alt = "";
            tileNode.src = tile.png_url;
            tileNode.style.left = `${tile.tile_x * tileLayer.tileSize}px`;
            tileNode.style.top = `${tile.tile_y * tileLayer.tileSize}px`;
            tileFragment.append(tileNode);
          }
          tiles.replaceChildren(tileFragment);
        } else {
          tiles.replaceChildren();
        }
        stage.hidden = false;
        frame.hidden = true;
      } else {
        const desiredSrc = `${page.pdfUrl ?? state.pdfUrl}#page=1&zoom=${Math.round(state.zoom * 100)}`;
        if (frame.dataset.src !== desiredSrc) {
          frame.dataset.src = desiredSrc;
          frame.src = desiredSrc;
        }
        stage.hidden = true;
        selectedMarker.hidden = true;
        hoverMarker.hidden = true;
        tiles.replaceChildren();
        frame.hidden = false;
      }
      fragment.append(node);
    }
    for (const pageId of [...pageNodes.keys()]) {
      if (!state.pages.some((page) => page.pageId === pageId)) {
        pageNodes.delete(pageId);
        pageMetrics.delete(pageId);
        tileRequestKeys.delete(pageId);
        tileRequestSerials.delete(pageId);
        syncInteractionSerials.delete(`hover:${pageId}`);
        syncInteractionSerials.delete(`select:${pageId}`);
        for (const key of [...syncMapInflight.keys()]) {
          if (key.endsWith(`:${pageId}`)) {
            syncMapInflight.delete(key);
          }
        }
      }
    }
    elements.previewStack.replaceChildren(fragment);
    elements.preview.hidden = true;
    elements.placeholder.hidden = true;
    elements.previewStack.hidden = false;
    queueTileRefresh();
  };

  const dispatch = (message) => {
    const previousHovered = state.hoveredSource;
    const previousSelected = state.selectedSource;
    state = reduce(state, message);
    render();
    const eventDetailForSource = (source) => ({
      rev: state.lastAppliedRev,
      source
    });
    if (sourceKey(previousHovered) !== sourceKey(state.hoveredSource)) {
      emitEvent({
        type: "source-hovered",
        detail: eventDetailForSource(state.hoveredSource)
      });
    }
    if (sourceKey(previousSelected) !== sourceKey(state.selectedSource)) {
      emitEvent({
        type: "source-selected",
        detail: eventDetailForSource(state.selectedSource)
      });
    }
    emitEvent({
      type: "state-changed",
      state
    });
  };

  const ensureSyncMap = (pageId) => {
    const cached = state.syncMaps[pageId];
    if (cached && cached.rev === state.lastAppliedRev) {
      return Promise.resolve(cached);
    }
    if (state.lastAppliedRev < 1 || !pageExistsForState(state, pageId)) {
      return Promise.resolve(null);
    }
    const requestKey = `${state.lastAppliedRev}:${pageId}`;
    if (syncMapInflight.has(requestKey)) {
      return syncMapInflight.get(requestKey);
    }
    const requestRev = state.lastAppliedRev;
    const request = transport.fetchSyncMap({
      rev: requestRev,
      pageId
    })
      .then((syncMap) => {
        if (state.lastAppliedRev !== requestRev) {
          return null;
        }
        if (!pageExistsForState(state, pageId)) {
          return null;
        }
        state = reduce(state, {
          type: "ui_syncmap_ready",
          rev: syncMap.rev,
          page_id: syncMap.page_id,
          page_width_px: syncMap.page_width_px,
          page_height_px: syncMap.page_height_px,
          page_source_start_utf8: syncMap.page_source_start_utf8,
          page_source_end_utf8: syncMap.page_source_end_utf8,
          page_output_start_utf8: syncMap.page_output_start_utf8,
          page_output_end_utf8: syncMap.page_output_end_utf8,
          items: syncMap.items
        });
        render();
        return state.syncMaps[pageId] ?? null;
      })
      .catch(() => null)
      .finally(() => {
        syncMapInflight.delete(requestKey);
      });
    syncMapInflight.set(requestKey, request);
    return request;
  };

  const ensureSourceFile = (file) => {
    if (typeof file !== "string" || file.length === 0 || state.lastAppliedRev < 1) {
      return Promise.resolve(null);
    }
    const cached = state.sourceFiles[file];
    if (cached && cached.rev === state.lastAppliedRev) {
      return Promise.resolve(cached);
    }
    const requestKey = `${state.lastAppliedRev}:${file}`;
    if (sourceFileInflight.has(requestKey)) {
      return sourceFileInflight.get(requestKey);
    }
    const requestRev = state.lastAppliedRev;
    const request = transport.fetchSourceFile({
      rev: requestRev,
      file
    })
      .then((payload) => {
        if (state.lastAppliedRev !== requestRev) {
          return null;
        }
        state = reduce(state, {
          type: "ui_source_file_ready",
          rev: payload.rev,
          file: payload.file,
          content: payload.content,
          line_count: payload.line_count
        });
        render();
        return state.sourceFiles[file] ?? null;
      })
      .catch(() => null)
      .finally(() => {
        sourceFileInflight.delete(requestKey);
      });
    sourceFileInflight.set(requestKey, request);
    return request;
  };

  const updateHoverSelectionForPage = (pageId, clientX, clientY) => {
    const serialKey = `hover:${pageId}`;
    const serial = (syncInteractionSerials.get(serialKey) ?? 0) + 1;
    syncInteractionSerials.set(serialKey, serial);
    ensureSyncMap(pageId).then((syncMap) => {
      if (syncInteractionSerials.get(serialKey) !== serial) {
        return;
      }
      if (!syncMap) {
        dispatch({ type: "ui_sync_hovered", item: null });
        return;
      }
      const stage = pageNodes.get(pageId)?.querySelector(".page-stage");
      if (!stage) {
        return;
      }
      const rect = stage.getBoundingClientRect();
      if (rect.height <= 0 || rect.width <= 0) {
        return;
      }
      const pageX = ((clientX - rect.left) / rect.width) * syncMap.page_width_px;
      const pageY = ((clientY - rect.top) / rect.height) * syncMap.page_height_px;
      const item = syncSelectionFromItem(
        pageId,
        syncMap,
        selectNearestSyncItem(syncMap, pageY, pageX)
      );
      if (sourceKey(item) === sourceKey(state.hoveredSource)) {
        return;
      }
      dispatch({
        type: "ui_sync_hovered",
        item
      });
    });
  };

  const updateSelectedSourceForPage = (pageId, clientX, clientY) => {
    const serialKey = `select:${pageId}`;
    const serial = (syncInteractionSerials.get(serialKey) ?? 0) + 1;
    syncInteractionSerials.set(serialKey, serial);
    ensureSyncMap(pageId).then((syncMap) => {
      if (syncInteractionSerials.get(serialKey) !== serial) {
        return;
      }
      if (!syncMap) {
        dispatch({ type: "ui_sync_selected", item: null });
        return;
      }
      const stage = pageNodes.get(pageId)?.querySelector(".page-stage");
      if (!stage) {
        return;
      }
      const rect = stage.getBoundingClientRect();
      if (rect.height <= 0 || rect.width <= 0) {
        return;
      }
      const pageX = ((clientX - rect.left) / rect.width) * syncMap.page_width_px;
      const pageY = ((clientY - rect.top) / rect.height) * syncMap.page_height_px;
      const item = syncSelectionFromItem(
        pageId,
        syncMap,
        selectNearestSyncItem(syncMap, pageY, pageX)
      );
      dispatch({
        type: "ui_sync_selected",
        item
      });
      if (state.editorBridgeEnabled && item) {
        openSourceRequest(item);
      }
    });
  };

  const runSourceJumpRequest = (
    requestInput,
    messageType,
    eventType: "source-jump-resolved" | "source-hover-resolved",
    failureEventType: "source-jump-failed" | "source-hover-failed"
  ) => {
    const request = normalizeSourceJumpRequest(requestInput);
    if (!request) {
      return Promise.resolve(null);
    }
    if (state.lastAppliedRev < 1) {
      return Promise.resolve(null);
    }
    const requestRev = state.lastAppliedRev;
    const source = sourceSelectionFromRequest(request);
    const sourceHash = source?.sourceHash ?? "";
    const detail = {
      rev: requestRev,
      request,
      ...(source ? { source } : {}),
      sourceHash
    };
    return transport.jumpToSource({
      rev: requestRev,
      request
    })
      .then((jump) => {
        if (state.lastAppliedRev !== requestRev) {
          return detail;
        }
        const resolvedDetail = resolvedSourceRequestDetail(requestRev, request, jump, source);
        dispatch({
          type: messageType,
          page_id: jump.page_id,
          page_index: jump.page_index,
          item: resolvedDetail.item
        });
        emitEvent({
          type: eventType,
          detail: resolvedDetail
        });
        scrollPageIntoFrame(jump.page_id);
        return resolvedDetail;
      })
      .catch((error) => {
        const failureDetail = {
          ...detail,
          error: error instanceof Error ? error.message : String(error ?? "source jump request failed")
        };
        emitEvent({
          type: failureEventType,
          detail: failureDetail
        });
        return failureDetail;
      });
  };

  const resolveSourceJump = (file: any, offset?: any) =>
    runSourceJumpRequest(
      offset === undefined ? file : { file, ...(typeof offset === "number" ? { offset } : offset) },
      "ui_source_jump_resolved",
      "source-jump-resolved",
      "source-jump-failed"
    );

  const hoverSourceJump = (file: any, offset?: any) =>
    runSourceJumpRequest(
      offset === undefined ? file : { file, ...(typeof offset === "number" ? { offset } : offset) },
      "ui_source_hover_resolved",
      "source-hover-resolved",
      "source-hover-failed"
    );

  const openSourceRequest = (source, options = null) => {
    const baseRequest = sourceRequestFromSelection(source);
    const hashRequest = baseRequest
      ? null
      : typeof source === "string"
        ? parseSourceHashRequest(source)
        : source
          && typeof source === "object"
          && typeof source.sourceHash === "string"
          && source.sourceHash.length > 0
            ? parseSourceHashRequest(source.sourceHash)
            : null;
    const directRequest = baseRequest ? null : normalizeSourceJumpRequest(source) ?? hashRequest;
    const directSource = baseRequest ? null : sourceSelectionFromRequest(directRequest);
    const previewOnly = options?.launch === false;
    const sourceHash = baseRequest
      ? formatSourceSelectionHash(source)
      : directSource?.sourceHash ?? "";
    const request = baseRequest
      ? {
        ...baseRequest,
        ...(sourceHash ? { source_hash: sourceHash } : {}),
        ...(previewOnly ? { launch: false } : {})
      }
      : directRequest
        ? {
          ...(typeof directRequest.source_hash === "string" && directRequest.source_hash.length > 0
            ? { source_hash: directRequest.source_hash }
            : {
              ...directRequest,
              ...(sourceHash ? { source_hash: sourceHash } : {})
            }),
          ...(previewOnly ? { launch: false } : {})
        }
        : null;
    if (!request) {
      return Promise.resolve(null);
    }
    const requestRev = state.lastAppliedRev;
    const detail = {
      rev: requestRev,
      ...((baseRequest ? source : directSource) ? { source: baseRequest ? source : directSource } : {}),
      request,
      sourceHash,
      launchRequested: !previewOnly,
      previewOnly
    };
    emitEvent({
      type: "open-source",
      detail
    });
    if (requestRev < 1) {
      return Promise.resolve(detail);
    }
    return transport.openSource({
      rev: requestRev,
      request
    })
      .then((response) => {
        if (state.lastAppliedRev !== requestRev) {
          return detail;
        }
        const resolvedDetail = {
          ...detail,
          ...resolvedSourceRequestDetail(
            requestRev,
            request,
            response,
            baseRequest ? source : directSource
          )
        };
        emitEvent({
          type: "open-source-resolved",
          detail: resolvedDetail
        });
        if (
          resolvedDetail.item
          && typeof response.page_id === "string"
          && pageExistsForState(state, response.page_id)
        ) {
          dispatch({
            type: "ui_open_source_resolved",
            page_id: response.page_id,
            page_index: response.page_index ?? Math.max(0, state.pageIds.indexOf(response.page_id)),
            item: resolvedDetail.item
          });
          scrollPageIntoFrame(response.page_id);
        }
        return resolvedDetail;
      })
      .catch((error) => {
        const failedDetail = {
          ...detail,
          error: error instanceof Error ? error.message : String(error ?? "open source request failed")
        };
        emitEvent({
          type: "open-source-failed",
          detail: failedDetail
        });
        return failedDetail;
      });
  };

  const splitOpenSourceInvocation = (sourceInput = null, optionsInput = null) => {
    let source = sourceInput;
    let options = optionsInput && typeof optionsInput === "object" && !Array.isArray(optionsInput)
      ? { ...optionsInput }
      : null;
    if (source && typeof source === "object" && !Array.isArray(source)) {
      const sourceLike = { ...source };
      if (Object.prototype.hasOwnProperty.call(sourceLike, "launch")) {
        if (!options || !Object.prototype.hasOwnProperty.call(options, "launch")) {
          options = {
            ...(options ?? {}),
            launch: sourceLike.launch
          };
        }
        delete sourceLike.launch;
      }
      source = Object.keys(sourceLike).length > 0 ? sourceLike : null;
    }
    return {
      source: source ?? (state.selectedSource ?? state.hoveredSource),
      options
    };
  };

  const emitOpenSourceRequest = (sourceInput = null, optionsInput = null) => {
    const { source, options } = splitOpenSourceInvocation(sourceInput, optionsInput);
    return openSourceRequest(source, options);
  };

  const emitPreviewSourceRequest = (requestInput = null) =>
    emitOpenSourceRequest(requestInput, { launch: false });

  const changePageBy = (delta) => {
    const nextPage = state.pageIds.length > 0
      ? Math.max(1, Math.min(state.pageIds.length, state.currentPage + delta))
      : Math.max(1, state.currentPage + delta);
    if (nextPage === state.currentPage) {
      return;
    }
    dispatch({ type: "ui_page_changed", page: nextPage });
    const nextPageId = activePageForState(state)?.pageId;
    if (nextPageId) {
      scrollPageIntoFrame(nextPageId);
    }
    queueTileRefresh();
  };

  elements.prevPage.addEventListener("click", () => changePageBy(-1));
  elements.nextPage.addEventListener("click", () => changePageBy(1));
  elements.zoomOut.addEventListener("click", () => dispatch({ type: "ui_zoom_changed", zoom: state.zoom - 0.1 }));
  elements.zoomIn.addEventListener("click", () => dispatch({ type: "ui_zoom_changed", zoom: state.zoom + 0.1 }));
  elements.sourceOpen.addEventListener("click", () => {
    emitOpenSourceRequest();
  });
  elements.frame.addEventListener("scroll", () => {
    state = reduce(state, { type: "ui_scroll_changed", scrollTop: elements.frame.scrollTop });
    if (state.pageIds.length > 0) {
      const frameRect = elements.frame.getBoundingClientRect();
      if (frameRect.height > 0) {
        const frameCenter = frameRect.top + (frameRect.height / 2);
        let nextPage = state.currentPage;
        let bestVisibleOverlap = -1;
        let bestVisibleDistance = Number.POSITIVE_INFINITY;
        let bestFallbackDistance = Number.POSITIVE_INFINITY;
        for (const [index, page] of state.pages.entries()) {
          const pageNode = pageNodes.get(page.pageId);
          if (!pageNode) {
            continue;
          }
          const pageRect = pageNode.getBoundingClientRect();
          if (pageRect.height <= 0) {
            continue;
          }
          const overlap = Math.min(frameRect.bottom, pageRect.bottom) - Math.max(frameRect.top, pageRect.top);
          const pageCenter = pageRect.top + (pageRect.height / 2);
          const distance = Math.abs(pageCenter - frameCenter);
          if (overlap > 0) {
            if (overlap > bestVisibleOverlap || (overlap === bestVisibleOverlap && distance < bestVisibleDistance)) {
              bestVisibleOverlap = overlap;
              bestVisibleDistance = distance;
              nextPage = index + 1;
            }
            continue;
          }
          if (bestVisibleOverlap > 0) {
            continue;
          }
          if (distance < bestFallbackDistance) {
            bestFallbackDistance = distance;
            nextPage = index + 1;
          }
        }
        if (nextPage !== state.currentPage) {
          dispatch({ type: "ui_page_changed", page: nextPage });
        }
      }
    }
    queueTileRefresh();
  });
  viewerWindow.addEventListener("resize", queueTileRefresh);

  transport.fetchState()
    .then((snapshot) => {
      state = {
        ...state,
        currentRev: snapshot.current_rev,
        lastAppliedRev: snapshot.last_applied_rev,
        pdfUrl: snapshot.pdf_url,
        pageIds: snapshot.page_artifacts?.length > 0
          ? snapshot.page_artifacts.map((page) => page.page_id)
          : snapshot.page_ids ?? [],
        pages: snapshot.page_artifacts?.map((page) => ({
          pageId: page.page_id,
          pdfUrl: page.pdf_url,
          svgUrl: page.svg_url ?? null
        })) ?? [],
        sourceFiles: sourceFilesFromSnapshot(
          (snapshot.source_snapshot ?? []).map((entry) => ({
            ...entry,
            rev: snapshot.last_applied_rev
          }))
        ),
        tileLayers: {},
        diagnostics: snapshot.diagnostics,
        changedFiles: snapshot.changed_files,
        building: snapshot.building,
        lastBuildSucceeded: snapshot.last_build_succeeded,
        editorBridgeEnabled: snapshot.editor_bridge_enabled ?? false
      };
      render();
      emitEvent({
        type: "state-changed",
        state
      });
    })
    .catch(() => {
      render();
      emitEvent({
        type: "state-changed",
        state
      });
    });

  const socket = transport.openWebSocket();
  socket.addEventListener("open", () => {
    lastViewportPayload = null;
    queueTileRefresh();
  });
  socket.addEventListener("message", (event: any) => {
    dispatch(JSON.parse(event.data));
  });
  socket.addEventListener("close", () => {
    lastViewportPayload = null;
    state = reduce(state, {
      type: "diagnostics",
      rev: state.currentRev,
      items: [
        {
          level: "warning",
          file: null,
          line: null,
          message: "Preview socket disconnected"
        }
      ]
    });
    render();
    emitEvent({
      type: "state-changed",
      state
    });
  });


  return {
    destroy() {
      if (typeof socket?.close === "function") {
        socket.close();
      }
      shadowRoot.innerHTML = "";
    },
    openSelectedSource: emitOpenSourceRequest,
    previewSelectedSource: emitPreviewSourceRequest,
    jumpToSource: resolveSourceJump,
    hoverSource: hoverSourceJump,
    getState() {
      return state;
    },
    shadowRoot
  };
}
