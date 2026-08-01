import type { BrowserAssetManifestEntry } from "./browser-artifacts";

export type BrowserAssetMaterialization = {
  urls: Record<string, string>;
  diagnostics: string[];
};

type ObjectUrlFactory = (
  bytes: Uint8Array,
  mimeType: string,
  assetRef: string
) => string;

const mimeTypes: Record<string, string> = {
  jpeg: "image/jpeg",
  jpg: "image/jpeg",
  png: "image/png",
  svg: "image/svg+xml"
};

function normalizeAssetPath(assetRef: string): string | null {
  const normalized = assetRef.replaceAll("\\", "/");
  if (
    normalized.startsWith("/")
    || /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(normalized)
    || normalized.split("/").includes("..")
  ) {
    return null;
  }

  const parts = normalized.split("/").filter((part) => part && part !== ".");
  return parts.length > 0 ? parts.join("/") : null;
}

function assetFormat(entry: BrowserAssetManifestEntry, path: string): string {
  const extension = path.includes(".") ? path.slice(path.lastIndexOf(".") + 1) : "";
  return (entry.format || extension).toLowerCase();
}

export function materializeBrowserAssets(
  manifest: BrowserAssetManifestEntry[],
  files: Record<string, Uint8Array>,
  createObjectUrl: ObjectUrlFactory
): BrowserAssetMaterialization {
  const urls: Record<string, string> = {};
  const diagnostics: string[] = [];
  const seenAssetRefs = new Set<string>();

  for (const entry of manifest) {
    if (seenAssetRefs.has(entry.asset_ref)) {
      continue;
    }
    seenAssetRefs.add(entry.asset_ref);
    const path = normalizeAssetPath(entry.asset_ref);
    if (!path) {
      diagnostics.push(`browser asset path is unsafe: ${entry.asset_ref}`);
      continue;
    }

    const format = assetFormat(entry, path);
    const mimeType = mimeTypes[format];
    if (!mimeType) {
      diagnostics.push(`browser preview does not decode ${format || "unknown"} assets: ${entry.asset_ref}`);
      continue;
    }

    const bytes = files[path];
    if (!bytes) {
      diagnostics.push(`browser asset is missing from memfs: ${entry.asset_ref}`);
      continue;
    }

    try {
      urls[entry.asset_ref] = createObjectUrl(bytes, mimeType, entry.asset_ref);
    } catch (error) {
      const detail = error instanceof Error ? `: ${error.message}` : "";
      diagnostics.push(`browser asset URL creation failed: ${entry.asset_ref}${detail}`);
    }
  }

  return { urls, diagnostics };
}

export function revokeBrowserAssetUrls(
  urls: Record<string, string>,
  revokeObjectUrl: (url: string) => void
) {
  for (const url of new Set(Object.values(urls))) {
    revokeObjectUrl(url);
  }
}
