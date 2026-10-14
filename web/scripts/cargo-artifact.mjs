/**
 * Returns the file cargo reported for `targetName` (from the JSON lines of
 * `cargo build --message-format=json-render-diagnostics`) that ends with
 * `extension`. Cargo places artifacts under whatever target directory the
 * environment or `.cargo/config.toml` selects, so callers must not assume
 * `<repo>/target`.
 */
export function cargoArtifactFile(cargoStdout, targetName, extension) {
  let artifact = null;
  for (const line of cargoStdout.split("\n")) {
    if (!line.startsWith("{")) {
      continue;
    }
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      continue;
    }
    if (message.reason !== "compiler-artifact" || message.target?.name !== targetName) {
      continue;
    }
    artifact = message.filenames?.find((file) => file.endsWith(extension)) ?? artifact;
  }
  if (!artifact) {
    throw new Error(`cargo did not report a ${extension} artifact for ${targetName}`);
  }
  return artifact;
}
