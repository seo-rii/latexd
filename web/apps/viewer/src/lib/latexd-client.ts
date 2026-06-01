import type {
  OpenSourceRequest,
  SourceJumpRequest,
  ViewerSocket,
  ViewerTransport
} from "@latexd/viewer-core";

export interface LatexdSourceSnapshotFile {
  file: string;
  content: string;
  line_count: number;
}

export interface RenderIrArtifactUrls {
  legacy_output_url: string;
  events_url: string;
  document_ir_url: string;
  page_display_list_url: string;
  display_list_pdf_url: string;
  display_list_svg_urls: string[];
}

export interface PreviewStateResponse {
  current_rev: number;
  last_applied_rev: number;
  pdf_url: string | null;
  page_count: number;
  page_ids: string[];
  page_artifacts: Array<{
    page_id: string;
    pdf_url: string;
    svg_url?: string | null;
  }>;
  render_ir_artifacts?: RenderIrArtifactUrls | null;
  source_snapshot?: LatexdSourceSnapshotFile[];
  diagnostics: unknown[];
  changed_files: string[];
  building: boolean;
  last_build_succeeded: boolean | null;
  editor_bridge_enabled: boolean;
}

export type LatexdServerMessage =
  | {
    type: "build_started";
    rev: number;
    changed_files: string[];
  }
  | {
    type: "diagnostics";
    rev: number;
    items: unknown[];
  }
  | {
    type: "full_pdf_ready";
    rev: number;
    pdf_url: string;
    page_ids: string[];
    page_artifacts: Array<{
      page_id: string;
      pdf_url: string;
      svg_url?: string | null;
    }>;
  }
  | {
    type: "patch_pages";
    rev: number;
    ops: unknown[];
  }
  | {
    type: "source_snapshot";
    rev: number;
    files: LatexdSourceSnapshotFile[];
  }
  | {
    type: "build_finished";
    rev: number;
    success: boolean;
  };

export interface SourceFilesResponse {
  rev: number;
  files: string[];
}

export interface SourceFileResponse {
  rev: number;
  file: string;
  content: string;
  line_count: number;
}

export interface UpdateSourceFileRequest {
  file: string;
  content: string;
}

export interface UpdateSourceFileResponse {
  file: string;
  line_count: number;
  byte_len: number;
}

export interface LatexdViewerTransportOptions {
  apiBase?: string;
  fetch?: typeof fetch;
  WebSocket?: typeof WebSocket;
  window?: Window & typeof globalThis;
  wsPath?: string;
  openWebSocket?: () => ViewerSocket;
}

export interface LatexdApiClient {
  fetchState(): Promise<PreviewStateResponse>;
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
  }): Promise<SourceFileResponse>;
  fetchSourceFiles(args: {
    rev: number;
  }): Promise<SourceFilesResponse>;
  jumpToSource(args: {
    request: SourceJumpRequest;
    rev: number;
  }): Promise<any>;
  openSource(args: {
    request: OpenSourceRequest;
    rev: number;
  }): Promise<any>;
  updateSourceFile(request: UpdateSourceFileRequest): Promise<UpdateSourceFileResponse>;
}

export function createLatexdApiClient(
  options: LatexdViewerTransportOptions = {}
): LatexdApiClient {
  const { resolveApiUrl, fetchJson } = createLatexdApiContext(options);

  return {
    fetchState() {
      return fetchJson(resolveApiUrl("api/state"));
    },
    fetchTileManifest({ rev, pageId, scale, left, top, width, height, tileSize }) {
      const url = resolveApiUrl(`api/tiles/${rev}/${encodeURIComponent(pageId)}`);
      url.searchParams.set("scale", scale.toFixed(2));
      url.searchParams.set("left", String(left));
      url.searchParams.set("top", String(top));
      url.searchParams.set("width", String(width));
      url.searchParams.set("height", String(height));
      url.searchParams.set("tile_size", String(tileSize));
      return fetchJson(url);
    },
    fetchSyncMap({ rev, pageId }) {
      return fetchJson(resolveApiUrl(`api/syncmap/${rev}/${encodeURIComponent(pageId)}`));
    },
    fetchSourceFile({ rev, file }) {
      const url = resolveApiUrl(`api/source-file/${rev}`);
      url.searchParams.set("file", file);
      return fetchJson(url);
    },
    fetchSourceFiles({ rev }) {
      return fetchJson(resolveApiUrl(`api/source-files/${rev}`));
    },
    jumpToSource({ rev, request }) {
      return fetchJson(buildSourceJumpUrl(resolveApiUrl(`api/source-jump/${rev}`), request));
    },
    openSource({ rev, request }) {
      return fetchJson(resolveApiUrl(`api/open-source/${rev}`), {
        method: "POST",
        headers: {
          "content-type": "application/json"
        },
        body: JSON.stringify(request),
        credentials: "include"
      });
    },
    updateSourceFile(request) {
      return fetchJson(resolveApiUrl("api/source-file"), {
        method: "PUT",
        headers: {
          "content-type": "application/json"
        },
        body: JSON.stringify(request),
        credentials: "include"
      });
    }
  };
}

export function createLatexdViewerTransport(
  options: LatexdViewerTransportOptions = {}
): ViewerTransport {
  const client = createLatexdApiClient(options);
  const openExternalWebSocket = options.openWebSocket;

  return {
    fetchState: client.fetchState,
    fetchTileManifest: client.fetchTileManifest,
    fetchSyncMap: client.fetchSyncMap,
    fetchSourceFile: client.fetchSourceFile,
    jumpToSource: client.jumpToSource,
    openSource: client.openSource,
    openWebSocket() {
      return openExternalWebSocket
        ? openExternalWebSocket()
        : openLatexdWebSocket(options);
    }
  };
}

export function openLatexdWebSocket(
  options: LatexdViewerTransportOptions = {}
): ViewerSocket {
  const { resolveWebSocketUrl } = createLatexdApiContext(options);
  const ViewerWebSocket = options.WebSocket ?? WebSocket;
  return new ViewerWebSocket(resolveWebSocketUrl()) as ViewerSocket;
}

function createLatexdApiContext(options: LatexdViewerTransportOptions) {
  const viewerWindow = options.window ?? window;
  const viewerFetch = options.fetch ?? fetch;
  const apiBase = new URL(options.apiBase ?? "./", viewerWindow.location.href);
  const wsPath = options.wsPath ?? "ws";

  const resolveApiUrl = (path: string) => new URL(path, apiBase);
  const resolveWebSocketUrl = () => {
    const url = new URL(wsPath, apiBase);
    url.protocol = viewerWindow.location.protocol === "https:" ? "wss:" : "ws:";
    return url.toString();
  };
  const fetchJson = async (input: URL | string, init: RequestInit = {credentials: 'include'}) => {
    const response = await viewerFetch(input, init);
    if (!response.ok) {
      throw new Error(`latexd request failed: ${response.status}`);
    }
    return response.json();
  };

  return {
    resolveApiUrl,
    resolveWebSocketUrl,
    fetchJson
  };
}

function buildSourceJumpUrl(baseUrl: URL, request: SourceJumpRequest | OpenSourceRequest) {
  if (typeof request.source_hash === "string" && request.source_hash.length > 0) {
    baseUrl.searchParams.set("source_hash", request.source_hash);
  }
  if (typeof request.file === "string" && request.file.length > 0) {
    baseUrl.searchParams.set("file", request.file);
  }
  if (typeof request.offset === "number" && Number.isFinite(request.offset)) {
    baseUrl.searchParams.set("offset", String(request.offset));
  } else if (typeof request.line === "number" && Number.isFinite(request.line)) {
    baseUrl.searchParams.set("line", String(request.line));
    if (typeof request.column === "number" && Number.isFinite(request.column)) {
      baseUrl.searchParams.set("column", String(request.column));
    }
  }
  return baseUrl;
}
