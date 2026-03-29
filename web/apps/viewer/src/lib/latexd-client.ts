import type {
  OpenSourceRequest,
  SourceJumpRequest,
  ViewerSocket,
  ViewerTransport
} from "@latexd/viewer-core";

export interface LatexdViewerTransportOptions {
  apiBase?: string;
  fetch?: typeof fetch;
  WebSocket?: typeof WebSocket;
  window?: Window & typeof globalThis;
  wsPath?: string;
}

export function createLatexdViewerTransport(
  options: LatexdViewerTransportOptions = {}
): ViewerTransport {
  const viewerWindow = options.window ?? window;
  const viewerFetch = options.fetch ?? fetch;
  const ViewerWebSocket = options.WebSocket ?? WebSocket;
  const apiBase = new URL(options.apiBase ?? "/", viewerWindow.location.origin);
  const wsPath = options.wsPath ?? "/ws";

  const resolveApiUrl = (path: string) => new URL(path, apiBase);
  const resolveWebSocketUrl = () => {
    const url = new URL(wsPath, apiBase);
    url.protocol = viewerWindow.location.protocol === "https:" ? "wss:" : "ws:";
    return url.toString();
  };

  const fetchJson = async (input: URL | string, init?: RequestInit) => {
    const response = await viewerFetch(input, init);
    if (!response.ok) {
      throw new Error(`latexd request failed: ${response.status}`);
    }
    return response.json();
  };

  return {
    fetchState() {
      return fetchJson(resolveApiUrl("/api/state"));
    },
    fetchTileManifest({ rev, pageId, scale, left, top, width, height, tileSize }) {
      const url = resolveApiUrl(`/api/tiles/${rev}/${encodeURIComponent(pageId)}`);
      url.searchParams.set("scale", scale.toFixed(2));
      url.searchParams.set("left", String(left));
      url.searchParams.set("top", String(top));
      url.searchParams.set("width", String(width));
      url.searchParams.set("height", String(height));
      url.searchParams.set("tile_size", String(tileSize));
      return fetchJson(url);
    },
    fetchSyncMap({ rev, pageId }) {
      return fetchJson(resolveApiUrl(`/api/syncmap/${rev}/${encodeURIComponent(pageId)}`));
    },
    fetchSourceFile({ rev, file }) {
      const url = resolveApiUrl(`/api/source-file/${rev}`);
      url.searchParams.set("file", file);
      return fetchJson(url);
    },
    jumpToSource({ rev, request }) {
      return fetchJson(buildSourceJumpUrl(resolveApiUrl(`/api/source-jump/${rev}`), request));
    },
    openSource({ rev, request }) {
      return fetchJson(resolveApiUrl(`/api/open-source/${rev}`), {
        method: "POST",
        headers: {
          "content-type": "application/json"
        },
        body: JSON.stringify(request)
      });
    },
    openWebSocket() {
      return new ViewerWebSocket(resolveWebSocketUrl()) as ViewerSocket;
    }
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
