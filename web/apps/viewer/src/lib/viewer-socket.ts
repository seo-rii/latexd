import { readable, type Readable } from "svelte/store";
import type { ViewerSocket } from "@latexd/viewer-core";

import {
  openLatexdWebSocket,
  type LatexdServerMessage,
  type LatexdViewerTransportOptions
} from "./latexd-client";

export type LatexdViewerSocketPhase = "idle" | "connecting" | "open" | "closed";

export interface LatexdViewerSocketState {
  phase: LatexdViewerSocketPhase;
  url: string;
}

export interface LatexdViewerRealtime {
  openWebSocket(): ViewerSocket;
  messages: Readable<LatexdServerMessage | null>;
  status: Readable<LatexdViewerSocketState>;
  destroy(): void;
}

export function createLatexdViewerRealtime(
  options: LatexdViewerTransportOptions = {}
): LatexdViewerRealtime {
  const viewerWindow = options.window ?? (typeof window !== "undefined" ? window : null);
  const apiBase = new URL(options.apiBase ?? "./", viewerWindow?.location.href ?? "http://127.0.0.1/");
  const wsPath = options.wsPath ?? "ws";
  const socketUrl = new URL(wsPath, apiBase);
  socketUrl.protocol = viewerWindow?.location.protocol === "https:" ? "wss:" : "ws:";
  const initialState: LatexdViewerSocketState = {
    phase: viewerWindow ? "connecting" : "idle",
    url: socketUrl.toString()
  };

  if (!viewerWindow) {
    return {
      openWebSocket() {
        throw new Error("latexd viewer realtime is only available in the browser");
      },
      messages: readable(null),
      status: readable(initialState),
      destroy() {}
    };
  }

  const socket = openLatexdWebSocket(options) as ViewerSocket & EventTarget;
  let closed = false;
  const status = readable<LatexdViewerSocketState>(initialState, (set) => {
    const handleOpen = () => {
      set({
        phase: "open",
        url: socketUrl.toString()
      });
    };
    const handleClose = () => {
      set({
        phase: "closed",
        url: socketUrl.toString()
      });
    };
    socket.addEventListener("open", handleOpen);
    socket.addEventListener("close", handleClose);
    socket.addEventListener("error", handleClose);
    set({
      phase: socket.readyState === 1 ? "open" : initialState.phase,
      url: socketUrl.toString()
    });
    return () => {
      socket.removeEventListener("open", handleOpen);
      socket.removeEventListener("close", handleClose);
      socket.removeEventListener("error", handleClose);
    };
  });
  const messages = readable<LatexdServerMessage | null>(null, (set) => {
    const handleMessage = (event: Event) => {
      const payload = (event as MessageEvent).data;
      if (typeof payload !== "string") {
        return;
      }
      try {
        set(JSON.parse(payload) as LatexdServerMessage);
      } catch {
        // Ignore malformed websocket frames at the app boundary.
      }
    };
    socket.addEventListener("message", handleMessage);
    return () => {
      socket.removeEventListener("message", handleMessage);
    };
  });

  return {
    openWebSocket() {
      return socket;
    },
    messages,
    status,
    destroy() {
      if (closed) {
        return;
      }
      closed = true;
      if (typeof socket.close === "function") {
        socket.close();
      }
    }
  };
}
