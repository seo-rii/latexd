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

import type { BrowserCompileResult } from "./browser-compiler-protocol";

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
const modulePromises = new Map<string, Promise<WebAssembly.Module>>();

function loadCompiler(compilerUrl: string) {
  let modulePromise = modulePromises.get(compilerUrl);
  if (!modulePromise) {
    modulePromise = WebAssembly.compileStreaming(fetch(compilerUrl)).catch((error) => {
      // A transient fetch/compile failure must not poison subsequent builds.
      modulePromises.delete(compilerUrl);
      throw error;
    });
    modulePromises.set(compilerUrl, modulePromise);
  }
  return modulePromise;
}

function compilerFailure(reason: string, stderr: string[]) {
  return new Error(stderr.length > 0
    ? `WASI compilation failed: ${reason}\n${stderr.join("\n")}`
    : `WASI compilation failed: ${reason}`);
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

export async function runBrowserCompiler(
  files: Record<string, Uint8Array>,
  entry = "main.tex",
  revision: number,
  compilerUrl: string
): Promise<BrowserCompileResult> {
  const root = new Directory(new Map<string, Inode>());
  for (const [path, bytes] of Object.entries(files)) {
    addFile(root, path, new File(bytes, { readonly: true }));
  }
  // The request and outputs get their own preopen so project files named
  // `output.pdf`, `pages.json`, ... are neither shadowed nor overwritten.
  const control = new Directory(new Map<string, Inode>());
  const outputJson = new File([]);
  const outputPdf = new File([]);
  const pagesJson = new File([]);
  const buildMetaJson = new File([]);
  addFile(control, "request.json", new File(encoder.encode(JSON.stringify({
    revision,
    entry,
    files: Object.keys(files)
  })), { readonly: true }));
  addFile(control, "output.json", outputJson);
  addFile(control, "output.pdf", outputPdf);
  addFile(control, "pages.json", pagesJson);
  addFile(control, "build-meta.json", buildMetaJson);

  const stderr: string[] = [];
  const wasi = new WASI(
    ["latexd-wasi"],
    [],
    [
      new OpenFile(new File([])),
      ConsoleStdout.lineBuffered(() => {}),
      ConsoleStdout.lineBuffered((line) => stderr.push(line)),
      new PreopenDirectory("/workspace", root.contents),
      new PreopenDirectory("/latexd", control.contents)
    ]
  );
  const instance = await WebAssembly.instantiate(await loadCompiler(compilerUrl), {
    wasi_snapshot_preview1: wasi.wasiImport
  });
  // A Rust panic aborts with a trap and a fatal error exits non-zero; either way the
  // reason is on stderr and output.json is missing, so report those instead of a
  // JSON parse error.
  let exitCode: number;
  try {
    exitCode = wasi.start(instance as WebAssembly.Instance & {
      exports: { memory: WebAssembly.Memory; _start: () => unknown };
    });
  } catch (error) {
    throw compilerFailure(
      `the compiler crashed (${error instanceof Error ? error.message : String(error)})`,
      stderr
    );
  }
  if (exitCode !== 0) {
    throw compilerFailure(`the compiler exited with code ${exitCode}`, stderr);
  }
  if (outputJson.data.byteLength === 0) {
    throw compilerFailure("the compiler produced no output.json", stderr);
  }
  const response = JSON.parse(decoder.decode(outputJson.data)) as WasiResponse;
  if (!response.success) {
    throw new Error(response.error || stderr.join("\n") || "WASI compilation failed");
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
