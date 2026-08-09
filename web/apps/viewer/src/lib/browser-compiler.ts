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
import {
  type BrowserBuildMetadata,
  type BrowserPagesArtifact,
  validateBrowserArtifacts
} from "./browser-artifacts";

export type {
  BrowserBuildMetadata,
  BrowserFontAsset,
  BrowserPageDisplayList,
  BrowserPagesArtifact
} from "./browser-artifacts";

export type BrowserCompileResult = {
  schema_version: number;
  extracted_text: string;
  event_count: number;
  page_artifact: BrowserPagesArtifact;
  build_metadata: BrowserBuildMetadata;
  diagnostics: string[];
  pdf: Uint8Array;
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
  validateBrowserArtifacts(pageArtifact, buildMetadata, {
    revision,
    page_count: response.page_count,
    event_count: response.event_count,
    diagnostic_count: response.diagnostics.length
  });
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
