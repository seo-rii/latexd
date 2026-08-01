import type { BrowserDrawOp, BrowserRect } from "./browser-artifacts.ts";

type BrowserGraphicsStateOp = Extract<
  BrowserDrawOp,
  { kind: "save" | "restore" | "clip_rect" }
>;

export type BrowserDrawableOp = Exclude<BrowserDrawOp, BrowserGraphicsStateOp>;

export type PreparedDisplayListOp = {
  op: BrowserDrawableOp;
  clip_rect: BrowserRect | null;
};

export type PreparedDisplayList = {
  ops: PreparedDisplayListOp[];
  diagnostics: string[];
};

export function browserDestinationId(name: string) {
  const encoded = encodeURIComponent(name).replaceAll("%", "_");
  return `destination-${encoded || "unnamed"}`;
}

export function browserLinkHref(target: string): string | null {
  const value = target.trim();
  if (!value || /^(?:javascript|data|vbscript):/i.test(value)) {
    return null;
  }
  if (/^(?:https?|mailto|tel):/i.test(value) || /^(?:\/|\.\/|\.\.\/)/.test(value)) {
    return value;
  }
  return `#${browserDestinationId(value.startsWith("#") ? value.slice(1) : value)}`;
}

function intersectRects(left: BrowserRect, right: BrowserRect): BrowserRect {
  const x = Math.max(left.x, right.x);
  const y = Math.max(left.y, right.y);
  const rightEdge = Math.min(left.x + left.width, right.x + right.width);
  const bottomEdge = Math.min(left.y + left.height, right.y + right.height);
  return {
    x,
    y,
    width: Math.max(0, rightEdge - x),
    height: Math.max(0, bottomEdge - y)
  };
}

export function prepareDisplayListOps(ops: BrowserDrawOp[]): PreparedDisplayList {
  const prepared: PreparedDisplayListOp[] = [];
  const diagnostics: string[] = [];
  const stack: Array<BrowserRect | null> = [];
  let clipRect: BrowserRect | null = null;

  for (const op of ops) {
    if (op.kind === "save") {
      stack.push(clipRect ? { ...clipRect } : null);
      continue;
    }
    if (op.kind === "restore") {
      if (stack.length === 0) {
        diagnostics.push("display list restored an empty graphics-state stack");
      } else {
        clipRect = stack.pop() ?? null;
      }
      continue;
    }
    if (op.kind === "clip_rect") {
      const nextClip = { x: op.x, y: op.y, width: op.width, height: op.height };
      clipRect = clipRect ? intersectRects(clipRect, nextClip) : nextClip;
      continue;
    }
    prepared.push({
      op,
      clip_rect: clipRect ? { ...clipRect } : null
    });
  }

  if (stack.length > 0) {
    diagnostics.push(`display list ended with ${stack.length} saved graphics state(s)`);
  }

  return { ops: prepared, diagnostics };
}
