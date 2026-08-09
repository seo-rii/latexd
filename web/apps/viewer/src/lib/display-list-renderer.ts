import type {
  BrowserDrawOp,
  BrowserFontAsset,
  BrowserGlyphOutline,
  BrowserPageDisplayList,
  BrowserRect
} from "./browser-artifacts.ts";

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

type BrowserTextRun = Extract<BrowserDrawOp, { kind: "text_run" }>;

export type BrowserPositionedOutline = {
  glyph_id: number;
  path: string;
  transform: string;
};

export function browserGlyphPathData(outline: BrowserGlyphOutline) {
  return outline.commands.map((command) => {
    switch (command.kind) {
      case "move_to":
        return `M ${command.x} ${command.y}`;
      case "line_to":
        return `L ${command.x} ${command.y}`;
      case "quad_to":
        return `Q ${command.x1} ${command.y1} ${command.x} ${command.y}`;
      case "curve_to":
        return `C ${command.x1} ${command.y1} ${command.x2} ${command.y2} ${command.x} ${command.y}`;
      case "close":
        return "Z";
    }
  }).join(" ");
}

export function browserPositionedGlyphOutlines(
  run: BrowserTextRun,
  fonts: BrowserFontAsset[]
): BrowserPositionedOutline[] | null {
  const resolved = run.resolved_font;
  const positionedGlyphs = run.glyphs;
  if (!resolved || !positionedGlyphs) {
    return null;
  }
  const font = fonts.find((candidate) => (
    candidate.face_id === resolved.face_id
    && candidate.postscript_name === resolved.postscript_name
    && candidate.glyph_id_kind === resolved.glyph_id_kind
    && candidate.content_hash === resolved.content_hash
  ));
  if (!font) {
    return null;
  }
  const outlines = new Map(font.glyphs.map((outline) => [outline.glyph_id, outline]));
  const result: BrowserPositionedOutline[] = [];
  for (const glyph of positionedGlyphs) {
    const outline = outlines.get(glyph.glyph_id);
    if (!outline) {
      return null;
    }
    result.push({
      glyph_id: glyph.glyph_id,
      path: browserGlyphPathData(outline),
      transform: `matrix(${run.size_pt} 0 0 ${-run.size_pt} ${run.origin.x + glyph.offset.x} ${run.origin.y + glyph.offset.y})`
    });
  }
  return result;
}

export function countBrowserTextFallbacks(
  pages: BrowserPageDisplayList[],
  fonts: BrowserFontAsset[]
) {
  let count = 0;
  for (const op of pages.flatMap((page) => page.ops)) {
    if (op.kind === "text_run" && browserPositionedGlyphOutlines(op, fonts) === null) {
      count += 1;
    }
  }
  return count;
}

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
