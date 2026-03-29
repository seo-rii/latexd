import {
  formatSourceRequestHash,
  formatSourceSelectionHash,
  mountViewer,
  parseSourceHashRequest,
  type ViewerEvent,
  type ViewerMountOptions
} from "@latexd/viewer-core";

import {
  createLatexdViewerTransport,
  type LatexdViewerTransportOptions
} from "./latexd-client";

type ViewerWindow = Window & typeof globalThis & Record<string, any>;

const HOST_EVENT_TYPES: Record<Exclude<ViewerEvent["type"], "state-changed">, string> = {
  "source-hovered": "latexd:source-hovered",
  "source-selected": "latexd:source-selected",
  "source-jump-resolved": "latexd:source-jump-resolved",
  "source-jump-failed": "latexd:source-jump-failed",
  "source-hover-resolved": "latexd:source-hover-resolved",
  "source-hover-failed": "latexd:source-hover-failed",
  "open-source": "latexd:open-source",
  "open-source-resolved": "latexd:open-source-resolved",
  "open-source-failed": "latexd:open-source-failed"
};

export interface LatexdViewerHostOptions
  extends Omit<ViewerMountOptions, "transport" | "onEvent">,
    LatexdViewerTransportOptions {
  CustomEvent?: typeof CustomEvent;
  exposeGlobals?: boolean;
}

export function mountLatexdViewerHost(
  root: HTMLElement,
  options: LatexdViewerHostOptions = {}
) {
  const {
    apiBase,
    fetch,
    WebSocket,
    window,
    wsPath,
    CustomEvent: CustomEventCtor = CustomEvent,
    exposeGlobals = true,
    ...viewerOptions
  } = options;
  const viewerWindow = (window ?? globalThis.window) as ViewerWindow;
  let state = null as any;
  let pendingSourceHashRequest = parseSourceHashRequest(viewerWindow.location.hash);

  const viewer = mountViewer(root, {
    ...viewerOptions,
    window: viewerWindow,
    transport: createLatexdViewerTransport({
      apiBase,
      fetch,
      WebSocket,
      window: viewerWindow,
      wsPath
    }),
    onEvent(event) {
      if (event.type === "state-changed") {
        state = event.state;
        flushPendingSourceHashRequest();
        return;
      }
      viewerWindow.dispatchEvent(new CustomEventCtor(HOST_EVENT_TYPES[event.type], {
        detail: event.detail
      }));
      if (event.type === "source-selected" && event.detail.source) {
        const nextHash = formatSourceSelectionHash(event.detail.source);
        if (nextHash && nextHash !== viewerWindow.location.hash) {
          const url = new URL(viewerWindow.location.href);
          url.hash = nextHash.slice(1);
          viewerWindow.history.replaceState({}, "", url);
        }
      }
    }
  });

  const flushPendingSourceHashRequest = () => {
    if (!pendingSourceHashRequest || !state || state.lastAppliedRev < 1) {
      return;
    }
    const request = pendingSourceHashRequest as NonNullable<typeof pendingSourceHashRequest>;
    pendingSourceHashRequest = null;
    const requestHash = formatSourceRequestHash(request);
    const selectedHash = formatSourceSelectionHash(state.selectedSource);
    if (requestHash && requestHash === selectedHash) {
      return;
    }
    viewer.jumpToSource(request);
  };

  const handleHashChange = () => {
    pendingSourceHashRequest = parseSourceHashRequest(viewerWindow.location.hash);
    flushPendingSourceHashRequest();
  };
  const handleJumpToSource = (event: any) => {
    viewer.jumpToSource(event.detail ?? null);
  };
  const handleSelectSource = (event: any) => {
    viewer.jumpToSource(event.detail ?? null);
  };
  const handleHoverSource = (event: any) => {
    viewer.hoverSource(event.detail ?? null);
  };

  viewerWindow.addEventListener("hashchange", handleHashChange);
  viewerWindow.addEventListener("latexd:jump-to-source", handleJumpToSource);
  viewerWindow.addEventListener("latexd:select-source", handleSelectSource);
  viewerWindow.addEventListener("latexd:hover-source", handleHoverSource);

  const globalAssignments = exposeGlobals
    ? {
      latexdJumpToSource: viewer.jumpToSource,
      latexdSelectSource: viewer.jumpToSource,
      latexdHoverSource: viewer.hoverSource,
      latexdOpenSelectedSource: viewer.openSelectedSource,
      latexdPreviewSelectedSource: viewer.previewSelectedSource
    }
    : null;

  if (globalAssignments) {
    Object.assign(viewerWindow, globalAssignments);
  }

  flushPendingSourceHashRequest();

  return {
    ...viewer,
    destroy() {
      viewerWindow.removeEventListener("hashchange", handleHashChange);
      viewerWindow.removeEventListener("latexd:jump-to-source", handleJumpToSource);
      viewerWindow.removeEventListener("latexd:select-source", handleSelectSource);
      viewerWindow.removeEventListener("latexd:hover-source", handleHoverSource);
      if (globalAssignments) {
        for (const [key, value] of Object.entries(globalAssignments)) {
          if (viewerWindow[key] === value) {
            delete viewerWindow[key];
          }
        }
      }
      viewer.destroy();
    }
  };
}
