import { readable, writable, type Readable } from "svelte/store";
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

  let socket: (ViewerSocket & EventTarget) | null = null;
  let unbindSocket = () => {};
  let closed = false;
  const statusStore = writable<LatexdViewerSocketState>(initialState);
  const messagesStore = writable<LatexdServerMessage | null>(null);

  const bindSocket = (nextSocket: ViewerSocket & EventTarget) => {
    const handleOpen = () => {
      statusStore.set({
        phase: "open",
        url: socketUrl.toString()
      });
    };
    const handleClose = () => {
      statusStore.set({
        phase: "closed",
        url: socketUrl.toString()
      });
    };
    const handleMessage = (event: Event) => {
      const payload = (event as MessageEvent).data;
      if (typeof payload !== "string") {
        return;
      }
      try {
        messagesStore.set(JSON.parse(payload) as LatexdServerMessage);
      } catch {
        // Ignore malformed websocket frames at the app boundary.
      }
    };
    nextSocket.addEventListener("open", handleOpen);
    nextSocket.addEventListener("close", handleClose);
    nextSocket.addEventListener("error", handleClose);
    nextSocket.addEventListener("message", handleMessage);
    statusStore.set({
      phase: nextSocket.readyState === 1
        ? "open"
        : nextSocket.readyState >= 2
          ? "closed"
          : "connecting",
      url: socketUrl.toString()
    });
    return () => {
      nextSocket.removeEventListener("open", handleOpen);
      nextSocket.removeEventListener("close", handleClose);
      nextSocket.removeEventListener("error", handleClose);
      nextSocket.removeEventListener("message", handleMessage);
    };
  };

  const ensureSocket = () => {
    if (socket && socket.readyState < 2) {
      return socket;
    }
    unbindSocket();
    socket = openLatexdWebSocket(options) as ViewerSocket & EventTarget;
    unbindSocket = bindSocket(socket);
    return socket;
  };

  ensureSocket();

  return {
    openWebSocket() {
      if (closed) {
        throw new Error("latexd viewer realtime has been destroyed");
      }
      return ensureSocket();
    },
    messages: { subscribe: messagesStore.subscribe },
    status: { subscribe: statusStore.subscribe },
    destroy() {
      if (closed) {
        return;
      }
      closed = true;
      if (!socket || socket.readyState < 2) {
        statusStore.set({
          phase: "closed",
          url: socketUrl.toString()
        });
      }
      unbindSocket();
      if (socket && typeof socket.close === "function") {
        socket.close();
      }
    }
  };
}
