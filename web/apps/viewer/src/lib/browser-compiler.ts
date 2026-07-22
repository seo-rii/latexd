import { base } from "$app/paths";

export type BrowserCompilePage = {
  page_id: string;
  width_pt: number;
  height_pt: number;
  lines: string[];
};

export type BrowserCompileResult = {
  schema_version: number;
  extracted_text: string;
  event_count: number;
  pages: BrowserCompilePage[];
  diagnostics: string[];
};

type WasmModule = {
  default: (input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module) => Promise<unknown>;
  compile_source: (source: string) => string;
};

let modulePromise: Promise<WasmModule> | null = null;

async function loadCompiler() {
  modulePromise ??= import(/* @vite-ignore */ `${base}/wasm/latexd_wasm.js`)
    .then(async (module: WasmModule) => {
      await module.default(`${base}/wasm/latexd_wasm_bg.wasm`);
      return module;
    });
  return modulePromise;
}

export async function compileInBrowser(source: string): Promise<BrowserCompileResult> {
  const compiler = await loadCompiler();
  return JSON.parse(compiler.compile_source(source)) as BrowserCompileResult;
}
