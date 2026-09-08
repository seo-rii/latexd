import type { BrowserBuildMetadata, BrowserPagesArtifact } from "./browser-artifacts";

export type BrowserCompileResult = {
  schema_version: number;
  extracted_text: string;
  event_count: number;
  page_artifact: BrowserPagesArtifact;
  build_metadata: BrowserBuildMetadata;
  diagnostics: string[];
  pdf: Uint8Array;
};

export type BrowserCompileRequest = {
  type: "compile";
  requestId: number;
  revision: number;
  entry: string;
  files: Record<string, Uint8Array>;
  compilerUrl: string;
};

export type BrowserCompileResponse = {
  requestId: number;
  revision: number;
} & (
  | { type: "compiled"; result: BrowserCompileResult }
  | { type: "failed"; error: string }
);
