import assert from "node:assert/strict";
import test from "node:test";

import type { BrowserAssetManifestEntry } from "../src/lib/browser-artifacts.ts";
import {
  materializeBrowserAssets,
  revokeBrowserAssetUrls
} from "../src/lib/browser-assets.ts";

test("browser assets materialize supported memfs images and diagnose the rest", () => {
  const manifest: BrowserAssetManifestEntry[] = [
    { asset_ref: "figure.png", format: "png", content_hash: "png-hash" },
    { asset_ref: "./icons/diagram.svg", format: "svg" },
    { asset_ref: "paper.pdf", format: "pdf" },
    { asset_ref: "missing.jpg", format: "jpeg" },
    { asset_ref: "../secret.png", format: "png" }
  ];
  const files = {
    "figure.png": new Uint8Array([137, 80, 78, 71]),
    "icons/diagram.svg": new TextEncoder().encode("<svg></svg>"),
    "paper.pdf": new TextEncoder().encode("%PDF-")
  };
  const calls: Array<{ asset_ref: string; mime_type: string; byte_length: number }> = [];

  const result = materializeBrowserAssets(
    manifest,
    files,
    (bytes, mimeType, assetRef) => {
      calls.push({
        asset_ref: assetRef,
        mime_type: mimeType,
        byte_length: bytes.byteLength
      });
      return `blob:${assetRef}`;
    }
  );

  assert.deepEqual(result.urls, {
    "figure.png": "blob:figure.png",
    "./icons/diagram.svg": "blob:./icons/diagram.svg"
  });
  assert.deepEqual(calls, [
    { asset_ref: "figure.png", mime_type: "image/png", byte_length: 4 },
    { asset_ref: "./icons/diagram.svg", mime_type: "image/svg+xml", byte_length: 11 }
  ]);
  assert.deepEqual(result.diagnostics, [
    "browser preview does not decode pdf assets: paper.pdf",
    "browser asset is missing from memfs: missing.jpg",
    "browser asset path is unsafe: ../secret.png"
  ]);
});

test("browser asset cleanup revokes each object URL once", () => {
  const revoked: string[] = [];

  revokeBrowserAssetUrls(
    {
      first: "blob:shared",
      second: "blob:shared",
      third: "blob:third"
    },
    (url) => revoked.push(url)
  );

  assert.deepEqual(revoked, ["blob:shared", "blob:third"]);
});
