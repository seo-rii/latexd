import type { BrowserSourceProvenance } from "./browser-artifacts";

export type BrowserSourceSelection = {
  key: string;
  path: string;
  start_utf8: number;
  end_utf8: number;
  start_index: number;
  end_index: number;
  line: number;
  column: number;
  end_line: number;
  end_column: number;
};

export function browserSourceKey(source: BrowserSourceProvenance): string | null {
  const span = source?.primary;
  if (
    !span
    || span.kind !== "file"
    || typeof span.path !== "string"
    || span.path.length === 0
    || !Number.isSafeInteger(span.start_utf8)
    || !Number.isSafeInteger(span.end_utf8)
    || span.start_utf8 < 0
    || span.end_utf8 < span.start_utf8
  ) {
    return null;
  }
  return `${span.path}:${span.start_utf8}:${span.end_utf8}`;
}

export function resolveBrowserSourceSelection(
  provenance: BrowserSourceProvenance,
  sourceFiles: Record<string, string>
): BrowserSourceSelection | null {
  const key = browserSourceKey(provenance);
  const span = provenance?.primary;
  if (!key || !span || span.kind !== "file") {
    return null;
  }
  const source = sourceFiles[span.path];
  if (source === undefined) {
    return null;
  }

  let utf8Offset = 0;
  let utf16Index = 0;
  let startIndex = span.start_utf8 === 0 ? 0 : -1;
  let endIndex = span.end_utf8 === 0 ? 0 : -1;
  while (utf16Index < source.length) {
    // UTF-8 width from the code point; a TextEncoder call per character made
    // one hover over a large file cost hundreds of milliseconds. A lone
    // surrogate counts 3 bytes, as TextEncoder encodes it as U+FFFD.
    const codePoint = source.codePointAt(utf16Index)!;
    utf8Offset += codePoint < 0x80 ? 1 : codePoint < 0x800 ? 2 : codePoint < 0x10000 ? 3 : 4;
    utf16Index += codePoint > 0xffff ? 2 : 1;
    if (utf8Offset === span.start_utf8) {
      startIndex = utf16Index;
    }
    if (utf8Offset === span.end_utf8) {
      endIndex = utf16Index;
    }
    if (startIndex >= 0 && endIndex >= 0) {
      break;
    }
  }
  if (startIndex < 0 || endIndex < 0) {
    return null;
  }

  const startLines = source.slice(0, startIndex).split("\n");
  const endLines = source.slice(0, endIndex).split("\n");
  return {
    key,
    path: span.path,
    start_utf8: span.start_utf8,
    end_utf8: span.end_utf8,
    start_index: startIndex,
    end_index: endIndex,
    line: startLines.length,
    column: (startLines.at(-1)?.length ?? 0) + 1,
    end_line: endLines.length,
    end_column: (endLines.at(-1)?.length ?? 0) + 1
  };
}
