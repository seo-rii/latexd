import { base } from "$app/paths";
import {
  ConsoleStdout,
  Directory,
  File,
  OpenFile,
  PreopenDirectory,
  WASI,
  type Inode
} from "@bjorn3/browser_wasi_shim";

export type BrowserCompileResult = {
  schema_version: number;
  extracted_text: string;
  event_count: number;
  page_artifact: BrowserPagesArtifact;
  build_metadata: BrowserBuildMetadata;
  diagnostics: string[];
  pdf: Uint8Array;
};

export type BrowserPageDisplayList = {
  page_id: string;
  width_pt: number;
  height_pt: number;
  ops: unknown[];
  source_spans: unknown[];
  content_hash: string;
};

export type BrowserPagesArtifact = {
  schema_version: number;
  revision: number;
  pages: BrowserPageDisplayList[];
  changed_page_ids: string[];
  removed_page_ids: string[];
  assets: Array<{
    asset_ref: string;
    format?: string;
    content_hash?: string;
  }>;
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

type WasiResponse = {
  schema_version: number;
  success: boolean;
  extracted_text: string;
  event_count: number;
  page_count: number;
  diagnostics: string[];
  error: string | null;
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();
let modulePromise: Promise<WebAssembly.Module> | null = null;

function loadCompiler() {
  modulePromise ??= WebAssembly.compileStreaming(fetch(`${base}/wasi/latexd-wasi.wasm`));
  return modulePromise;
}

function addFile(root: Directory, path: string, file: File) {
  const parts = path.split("/").filter(Boolean);
  let directory = root;
  for (const part of parts.slice(0, -1)) {
    const existing = directory.contents.get(part);
    if (existing instanceof Directory) {
      directory = existing;
      continue;
    }
    const child = new Directory(new Map());
    directory.contents.set(part, child);
    directory = child;
  }
  directory.contents.set(parts.at(-1) ?? path, file);
}

export async function compileProjectInBrowser(
  files: Record<string, Uint8Array>,
  entry = "main.tex",
  revision = 0
): Promise<BrowserCompileResult> {
  const root = new Directory(new Map<string, Inode>());
  for (const [path, bytes] of Object.entries(files)) {
    addFile(root, path, new File(bytes, { readonly: true }));
  }
  const outputJson = new File([]);
  const outputPdf = new File([]);
  const pagesJson = new File([]);
  const buildMetaJson = new File([]);
  addFile(root, "request.json", new File(encoder.encode(JSON.stringify({
    revision,
    entry,
    files: Object.keys(files)
  })), { readonly: true }));
  addFile(root, "output.json", outputJson);
  addFile(root, "output.pdf", outputPdf);
  addFile(root, "pages.json", pagesJson);
  addFile(root, "build-meta.json", buildMetaJson);

  const stderr: string[] = [];
  const wasi = new WASI(
    ["latexd-wasi"],
    [],
    [
      new OpenFile(new File([])),
      ConsoleStdout.lineBuffered(() => {}),
      ConsoleStdout.lineBuffered((line) => stderr.push(line)),
      new PreopenDirectory("/workspace", root.contents)
    ]
  );
  const instance = await WebAssembly.instantiate(await loadCompiler(), {
    wasi_snapshot_preview1: wasi.wasiImport
  });
  wasi.start(instance as WebAssembly.Instance & {
    exports: { memory: WebAssembly.Memory; _start: () => unknown };
  });
  const response = JSON.parse(decoder.decode(outputJson.data)) as WasiResponse;
  if (!response.success) {
    throw new Error(response.error ?? stderr.join("\n") ?? "WASI compilation failed");
  }
  const pageArtifact = JSON.parse(decoder.decode(pagesJson.data)) as BrowserPagesArtifact;
  const buildMetadata = JSON.parse(decoder.decode(buildMetaJson.data)) as BrowserBuildMetadata;
  if (pageArtifact.schema_version !== 1 || buildMetadata.schema_version !== 1) {
    throw new Error("WASI compiler returned an unsupported browser artifact schema");
  }
  if (pageArtifact.revision !== revision || buildMetadata.revision !== revision) {
    throw new Error("WASI compiler returned browser artifacts for the wrong revision");
  }
  if (
    buildMetadata.compile_mode !== "one_shot"
    || response.page_count !== pageArtifact.pages.length
    || buildMetadata.pages.total !== pageArtifact.pages.length
    || buildMetadata.pages.changed !== pageArtifact.changed_page_ids.length
    || buildMetadata.pages.reused !== 0
    || buildMetadata.pages.removed !== pageArtifact.removed_page_ids.length
    || buildMetadata.event_count !== response.event_count
    || buildMetadata.diagnostic_count !== response.diagnostics.length
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
  return {
    schema_version: response.schema_version,
    extracted_text: response.extracted_text,
    event_count: response.event_count,
    page_artifact: pageArtifact,
    build_metadata: buildMetadata,
    diagnostics: [...response.diagnostics, ...stderr],
    pdf: outputPdf.data.slice()
  };
}

export function compileInBrowser(source: string, revision = 0): Promise<BrowserCompileResult> {
  return compileProjectInBrowser({ "main.tex": encoder.encode(source) }, "main.tex", revision);
}
