use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;
use tex_world::ProjectManifest;

/// A private copy of project-local inputs. Compiler reads, cache hashes and
/// preview source text all refer to the bytes captured here, never a later save.
/// The package tree outside the project remains an engine environment dependency.
#[derive(Debug)]
pub(crate) struct ProjectRevision {
    _directory: TempDir,
    pub root: Utf8PathBuf,
    pub manifest: ProjectManifest,
    pub hashes: BTreeMap<Utf8PathBuf, String>,
    pub paths: BTreeSet<Utf8PathBuf>,
    pub sources: BTreeMap<Utf8PathBuf, String>,
}

impl ProjectRevision {
    /// Cwd isolation cannot redirect absolute TeX inputs. Refuse a recorded read
    /// of the live project rather than publish it with a snapshot-based hash.
    pub async fn validate_recorded_inputs(
        &self,
        live_root: &Utf8Path,
        revision_dir: &Utf8Path,
        toplevel: &Utf8Path,
    ) -> Result<()> {
        let root_prefix = format!("{live_root}/");
        for name in [
            format!("{}.fls", toplevel.file_stem().unwrap_or("main")),
            "deps.mk".into(),
        ] {
            let contents = match tokio::fs::read_to_string(revision_dir.join(&name)).await {
                Ok(contents) => contents,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error).context("failed to read compiler input trace"),
            };
            let records = if name.ends_with(".fls") {
                contents
                    .lines()
                    .filter_map(|line| line.strip_prefix("INPUT "))
                    .collect::<Vec<_>>()
            } else {
                contents
                    .split_once(':')
                    .map(|(_, inputs)| vec![inputs])
                    .unwrap_or_default()
            };
            for record in records {
                for (start, _) in record.match_indices(&root_prefix) {
                    let relative = &record[start + root_prefix.len()..];
                    // Snapshot inputs and generated aux/output are private build data.
                    if !relative.starts_with(".latexd/") {
                        bail!(
                            "compiler input trace references the live project outside the input snapshot; use project-relative inputs"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn capture(root: &Utf8Path, build_root: &Utf8Path) -> Result<Self> {
        let root = root.to_owned();
        let build_root = build_root.to_owned();
        tokio::task::spawn_blocking(move || {
            fs::create_dir_all(&build_root)?;
            let directory = tempfile::Builder::new()
                .prefix("input-")
                .tempdir_in(&build_root)?;
            let snapshot_root = Utf8PathBuf::from_path_buf(directory.path().to_owned())
                .map_err(|_| anyhow::anyhow!("input snapshot path is not UTF-8"))?;
            let mut hashes = BTreeMap::new();
            let mut paths = BTreeSet::new();
            let mut sources = BTreeMap::new();
            let excluded_build_root = fs::canonicalize(&build_root)?;
            let mut queue = vec![(Utf8PathBuf::new(), BTreeSet::new())];
            while let Some((relative, mut ancestors)) = queue.pop() {
                let source = root.join(&relative);
                let canonical = fs::canonicalize(&source)
                    .with_context(|| format!("failed to resolve snapshot input {relative}"))?;
                if !ancestors.insert(canonical.clone()) {
                    bail!("cyclic directory link in snapshot input {relative}");
                }
                fs::create_dir_all(snapshot_root.join(&relative))?;
                let entries = match fs::read_dir(&source) {
                    Ok(entries) => entries,
                    // One unreadable subdirectory must not fail every build. It is
                    // absent from this revision, as it is to the TeX engine.
                    Err(error)
                        if error.kind() == std::io::ErrorKind::PermissionDenied
                            && !relative.as_str().is_empty() =>
                    {
                        tracing::warn!("skipping unreadable project directory {relative}: {error}");
                        continue;
                    }
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("failed to snapshot directory {relative}"));
                    }
                };
                for entry in entries {
                    let entry = entry?;
                    // Inputs are addressed by UTF-8 paths, so such a name could never be
                    // referenced from a revision. Skip it rather than fail every build.
                    let Ok(name) = entry.file_name().into_string() else {
                        tracing::warn!(
                            "skipping project entry with a non-UTF-8 name in {relative}: {:?}",
                            entry.file_name()
                        );
                        continue;
                    };
                    if relative.as_str().is_empty() && matches!(name.as_str(), ".latexd" | ".git") {
                        continue;
                    }
                    let path = relative.join(name);
                    let input = root.join(&path);
                    let metadata = match fs::metadata(&input) {
                        Ok(metadata) => metadata,
                        // A disappeared input or dangling link remains absent in this revision.
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => {
                            return Err(error)
                                .with_context(|| format!("failed to snapshot {path}"));
                        }
                    };
                    let canonical_input = fs::canonicalize(&input)?;
                    if canonical_input.starts_with(&excluded_build_root) {
                        continue;
                    }
                    paths.insert(path.clone());
                    if metadata.is_dir() {
                        queue.push((path, ancestors.clone()));
                    } else if metadata.is_file() {
                        let bytes = match fs::read(&input) {
                            Ok(bytes) => bytes,
                            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                                tracing::warn!("skipping unreadable project file {path}: {error}");
                                continue;
                            }
                            Err(error) => {
                                return Err(error)
                                    .with_context(|| format!("failed to snapshot {path}"));
                            }
                        };
                        hashes.insert(path.clone(), blake3::hash(&bytes).to_hex().to_string());
                        if let Ok(text) = String::from_utf8(bytes.clone()) {
                            sources.insert(path.clone(), text);
                        } else if path
                            .extension()
                            .is_some_and(|ext| matches!(ext, "tex" | "sty" | "cls" | "bbl" | "bib"))
                        {
                            sources
                                .insert(path.clone(), String::from_utf8_lossy(&bytes).into_owned());
                        }
                        let destination = snapshot_root.join(&path);
                        fs::write(&destination, bytes)?;
                        fs::set_permissions(&destination, metadata.permissions())?;
                    } else {
                        // Sockets, FIFOs and devices are not document inputs, and
                        // reading a FIFO could block the snapshot forever.
                        tracing::warn!("skipping project entry that is not a regular file: {path}");
                    }
                }
            }
            let manifest = ProjectManifest::discover(&snapshot_root)?;
            Ok(Self {
                _directory: directory,
                root: snapshot_root,
                manifest,
                hashes,
                paths,
                sources,
            })
        })
        .await
        .context("input snapshot task failed")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_rejects_recorded_absolute_live_inputs() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        fs::write(root.join("main.tex"), "source").unwrap();
        let revision = ProjectRevision::capture(root, &root.join(".latexd/build"))
            .await
            .unwrap();
        let rev_dir = root.join(".latexd/build/rev-1");
        fs::create_dir_all(&rev_dir).unwrap();
        fs::write(
            rev_dir.join("main.fls"),
            format!("INPUT {}/main.tex\n", revision.root),
        )
        .unwrap();
        revision
            .validate_recorded_inputs(root, &rev_dir, Utf8Path::new("main.tex"))
            .await
            .unwrap();
        fs::write(
            rev_dir.join("main.fls"),
            format!("INPUT {root}/chapter.tex\n"),
        )
        .unwrap();
        assert!(
            revision
                .validate_recorded_inputs(root, &rev_dir, Utf8Path::new("main.tex"))
                .await
                .is_err()
        );
        fs::remove_file(rev_dir.join("main.fls")).unwrap();
        fs::write(
            rev_dir.join("deps.mk"),
            format!("{rev_dir}/main.pdf: {root}/chapter.tex\n"),
        )
        .unwrap();
        assert!(
            revision
                .validate_recorded_inputs(root, &rev_dir, Utf8Path::new("main.tex"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn snapshot_keeps_source_and_assets_after_live_edits_and_deletion() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        fs::write(root.join("main.tex"), "before").unwrap();
        fs::write(root.join("figure.png"), [0, 255, 1, 2]).unwrap();
        fs::create_dir_all(root.join(".latexd/build/rev-1")).unwrap();
        fs::write(root.join(".latexd/build/rev-1/old.pdf"), "old output").unwrap();
        let revision = ProjectRevision::capture(root, &root.join(".latexd/build"))
            .await
            .unwrap();
        fs::write(root.join("main.tex"), "after").unwrap();
        fs::remove_file(root.join("figure.png")).unwrap();
        fs::write(root.join("new.sty"), "new dependency").unwrap();
        assert_eq!(
            fs::read_to_string(revision.root.join("main.tex")).unwrap(),
            "before"
        );
        assert_eq!(
            fs::read(revision.root.join("figure.png")).unwrap(),
            [0, 255, 1, 2]
        );
        assert!(!revision.root.join("new.sty").exists());
        assert!(!revision.root.join(".latexd").exists());
        assert_eq!(
            revision.hashes[Utf8Path::new("main.tex")],
            blake3::hash(b"before").to_hex().to_string()
        );
        let snapshot_root = revision.root.clone();
        drop(revision);
        assert!(!snapshot_root.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn snapshot_materializes_linked_inputs_and_rejects_directory_cycles() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        fs::write(root.join("main.tex"), "source").unwrap();
        std::os::unix::fs::symlink("main.tex", root.join("linked.tex")).unwrap();
        let revision = ProjectRevision::capture(root, &root.join(".latexd/build"))
            .await
            .unwrap();
        assert!(
            !fs::symlink_metadata(revision.root.join("linked.tex"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::write(root.join("main.tex"), "changed").unwrap();
        assert_eq!(
            fs::read_to_string(revision.root.join("linked.tex")).unwrap(),
            "source"
        );
        std::os::unix::fs::symlink(".", root.join("cycle")).unwrap();
        assert!(
            ProjectRevision::capture(root, &root.join(".latexd/build"))
                .await
                .unwrap_err()
                .to_string()
                .contains("cyclic")
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn snapshot_skips_entries_with_non_utf8_names() {
        use std::os::unix::ffi::OsStrExt;

        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        fs::write(root.join("main.tex"), "source").unwrap();
        fs::create_dir_all(root.join("figures")).unwrap();
        // A Latin-1 name, as left behind by unpacking an archive made on Windows.
        let latin1_name = std::ffi::OsStr::from_bytes(b"caf\xe9.png");
        fs::write(
            root.join("figures").as_std_path().join(latin1_name),
            [1, 2, 3],
        )
        .unwrap();

        let revision = ProjectRevision::capture(root, &root.join(".latexd/build"))
            .await
            .expect("an unrelated non-UTF-8 file name must not fail the snapshot");
        assert_eq!(
            fs::read_to_string(revision.root.join("main.tex")).unwrap(),
            "source"
        );
        assert!(revision.paths.contains(Utf8Path::new("figures")));
        assert!(
            fs::read_dir(revision.root.join("figures"))
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn snapshot_skips_special_files_and_unreadable_entries() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        fs::write(root.join("main.tex"), "source").unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(root.join("editor.sock")).unwrap();
        fs::write(root.join("secret.key"), "private").unwrap();
        fs::create_dir_all(root.join("private")).unwrap();
        fs::write(root.join("private/notes.txt"), "private").unwrap();
        fs::set_permissions(root.join("secret.key"), fs::Permissions::from_mode(0o000)).unwrap();
        fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o000)).unwrap();
        // Permission bits do not restrict root, so only check them when they apply.
        let permissions_apply = fs::read(root.join("secret.key")).is_err();

        let result = ProjectRevision::capture(root, &root.join(".latexd/build")).await;
        fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(root.join("secret.key"), fs::Permissions::from_mode(0o644)).unwrap();
        let revision = result.expect("a socket or an unreadable entry must not fail the snapshot");
        assert_eq!(
            fs::read_to_string(revision.root.join("main.tex")).unwrap(),
            "source"
        );
        assert!(!revision.root.join("editor.sock").exists());
        assert!(revision.paths.contains(Utf8Path::new("editor.sock")));
        if permissions_apply {
            assert!(!revision.root.join("secret.key").exists());
            assert!(!revision.root.join("private/notes.txt").exists());
            assert!(!revision.hashes.contains_key(Utf8Path::new("secret.key")));
        }
    }
}
