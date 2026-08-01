export type BrowserPoint = {
  x: number;
  y: number;
};

export type BrowserRect = BrowserPoint & {
  width: number;
  height: number;
};

export type BrowserProvenanceSpan =
  | {
      kind: "file";
      path: string;
      start_utf8: number;
      end_utf8: number;
    }
  | {
      kind: "generated";
      stable_id: string;
      description: string;
    };

export type BrowserSourceProvenance = {
  primary: BrowserProvenanceSpan;
  related: unknown[];
  expansion_stack: unknown[];
  generated_by: string;
  expansion_stack_truncated: boolean;
};

export type BrowserFontFamily =
  | "serif"
  | "sans"
  | "mono"
  | "math"
  | "symbol"
  | "math_extension"
  | { named: string };

export type BrowserFontRequest = {
  family: BrowserFontFamily;
  series: "regular" | "bold";
  shape: "upright" | "italic";
  size_pt: number;
  role: "body" | "heading" | "math" | "mono";
};

export type BrowserDrawOp =
  | { kind: "save" }
  | { kind: "restore" }
  | ({ kind: "clip_rect" } & BrowserRect)
  | {
      kind: "text_run";
      origin: BrowserPoint;
      text: string;
      font: BrowserFontRequest;
      size_pt: number;
      approximate_advance_pt: number;
      resolved_font?: {
        face_id: string;
        postscript_name: string;
        glyph_id_kind: "type1_char_code" | "open_type_glyph_id";
        content_hash: string;
      };
      glyphs?: Array<{
        glyph_id: number;
        advance_pt: number;
        offset: BrowserPoint;
      }>;
      clusters?: Array<{
        text_start_utf8: number;
        text_end_utf8: number;
        glyph_start: number;
        glyph_end: number;
      }>;
      source: BrowserSourceProvenance;
    }
  | ({ kind: "rule" } & BrowserRect)
  | {
      kind: "image";
      rect: BrowserRect;
      asset_ref: string;
      asset_format?: string;
      page_selection?: unknown;
      asset_hash?: string;
      natural_width_pt?: number;
      natural_height_pt?: number;
      crop?: unknown;
      scale?: { x: number; y: number };
      rotation?: {
        angle_degrees: number;
        origin?: string;
      };
      diagnostic?: string;
      source: BrowserSourceProvenance;
    }
  | {
      kind: "link_annotation";
      rect: BrowserRect;
      target: string;
      source: BrowserSourceProvenance;
    }
  | {
      kind: "named_destination";
      name: string;
      point: BrowserPoint;
      source: BrowserSourceProvenance;
    };

export type BrowserPageDisplayList = {
  page_id: string;
  width_pt: number;
  height_pt: number;
  ops: BrowserDrawOp[];
  source_spans: unknown[];
  content_hash: string;
};

export type BrowserAssetManifestEntry = {
  asset_ref: string;
  format?: string;
  content_hash?: string;
};

export type BrowserPagesArtifact = {
  schema_version: number;
  revision: number;
  pages: BrowserPageDisplayList[];
  changed_page_ids: string[];
  removed_page_ids: string[];
  assets: BrowserAssetManifestEntry[];
};

export type BrowserBuildMetadata = {
  schema_version: number;
  revision: number;
  compile_mode: "one_shot" | "incremental";
  event_count: number;
  diagnostic_count: number;
  pages: {
    total: number;
    changed: number;
    reused: number;
    removed: number;
  };
};

export type BrowserArtifactExpectations = {
  revision: number;
  page_count: number;
  event_count: number;
  diagnostic_count: number;
};

export function validateBrowserArtifacts(
  pageArtifact: BrowserPagesArtifact,
  buildMetadata: BrowserBuildMetadata,
  expected: BrowserArtifactExpectations
) {
  if (pageArtifact.schema_version !== 1 || buildMetadata.schema_version !== 1) {
    throw new Error("WASI compiler returned an unsupported browser artifact schema");
  }
  if (pageArtifact.revision !== expected.revision || buildMetadata.revision !== expected.revision) {
    throw new Error("WASI compiler returned browser artifacts for the wrong revision");
  }
  if (
    buildMetadata.compile_mode !== "one_shot"
    || expected.page_count !== pageArtifact.pages.length
    || buildMetadata.pages.total !== pageArtifact.pages.length
    || buildMetadata.pages.changed !== pageArtifact.changed_page_ids.length
    || buildMetadata.pages.reused !== 0
    || buildMetadata.pages.removed !== pageArtifact.removed_page_ids.length
    || buildMetadata.event_count !== expected.event_count
    || buildMetadata.diagnostic_count !== expected.diagnostic_count
  ) {
    throw new Error("WASI compiler returned inconsistent browser page counts");
  }
  const pageIds = new Set(pageArtifact.pages.map((page) => page.page_id));
  const changedPageIds = new Set(pageArtifact.changed_page_ids);
  const removedPageIds = new Set(pageArtifact.removed_page_ids);
  if (
    pageIds.size !== pageArtifact.pages.length
    || changedPageIds.size !== pageArtifact.changed_page_ids.length
    || removedPageIds.size !== pageArtifact.removed_page_ids.length
    || changedPageIds.size !== pageIds.size
    || pageArtifact.pages.some((page) => (
      page.page_id.length === 0
      || page.content_hash.length === 0
      || !Number.isFinite(page.width_pt)
      || page.width_pt <= 0
      || !Number.isFinite(page.height_pt)
      || page.height_pt <= 0
    ))
    || pageArtifact.changed_page_ids.some((pageId) => !pageIds.has(pageId))
    || pageArtifact.removed_page_ids.some((pageId) => pageIds.has(pageId))
  ) {
    throw new Error("WASI compiler returned an invalid browser page manifest");
  }

  const assets: unknown = pageArtifact.assets;
  if (!Array.isArray(assets)) {
    throw new Error("WASI compiler returned an invalid browser asset manifest");
  }
  const assetRefs = new Set<string>();
  for (const asset of assets) {
    if (!asset || typeof asset !== "object") {
      throw new Error("WASI compiler returned an invalid browser asset manifest");
    }
    const entry = asset as Record<string, unknown>;
    if (
      typeof entry.asset_ref !== "string"
      || entry.asset_ref.length === 0
      || (entry.format !== undefined && (
        typeof entry.format !== "string"
        || entry.format.length === 0
      ))
      || (entry.content_hash !== undefined && (
        typeof entry.content_hash !== "string"
        || entry.content_hash.length === 0
      ))
      || assetRefs.has(entry.asset_ref)
    ) {
      throw new Error("WASI compiler returned an invalid browser asset manifest");
    }
    assetRefs.add(entry.asset_ref);
  }
}
