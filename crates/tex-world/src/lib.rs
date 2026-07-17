use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

/// Reads TeX for best-effort preview compilation without mutating the source bytes.
pub fn read_tex_source_lossy(path: &Utf8Path) -> std::io::Result<String> {
    let bytes = fs::read(path.as_std_path())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TexLiveVersion {
    Tl2023,
    #[default]
    Tl2025,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompilerMode {
    #[default]
    PdfLatex,
    LatexDvipsPs2Pdf,
    XeLatex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceUsage {
    Include,
    Toplevel,
}

impl Default for SourceUsage {
    fn default() -> Self {
        Self::Include
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntry {
    pub path: Utf8PathBuf,
    pub usage: SourceUsage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub compiler: CompilerMode,
    pub texlive_version: TexLiveVersion,
    pub compile_from_root: bool,
    pub allow_shell_escape: bool,
    pub supported_image_exts: Vec<String>,
    pub sources: Vec<SourceEntry>,
    pub toplevels: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ProjectWorld {
    pub root: Utf8PathBuf,
    pub manifest: ProjectManifest,
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    compiler: Option<CompilerMode>,
    texlive_version: Option<TexLiveVersion>,
    compile_from_root: Option<bool>,
    allow_shell_escape: Option<bool>,
    supported_image_exts: Option<Vec<String>>,
    #[serde(default)]
    sources: Vec<RawSourceEntry>,
    #[serde(default)]
    toplevel: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawSourceEntry {
    Path(String),
    Detailed {
        path: String,
        #[serde(default)]
        usage: SourceUsage,
    },
}

impl ProjectWorld {
    pub fn load(root: impl Into<Utf8PathBuf>) -> Result<Self> {
        let root = root.into();
        let manifest = ProjectManifest::discover(&root)?;
        Ok(Self { root, manifest })
    }

    pub fn resolve_path(&self, base_file: &Utf8Path, target: &str) -> Result<Utf8PathBuf> {
        let base_dir = base_file.parent().unwrap_or_else(|| Utf8Path::new(""));
        let joined = base_dir.join(target);
        normalize_relative_path(&joined)
    }

    pub fn resolve_local_file(&self, target: &str) -> Result<Utf8PathBuf> {
        normalize_relative_path(Utf8Path::new(target))
    }

    pub fn resolve_graphics(&self, base_file: &Utf8Path, target: &str) -> Result<Utf8PathBuf> {
        let resolved = self.resolve_path(base_file, target)?;
        if resolved.extension().is_some() {
            return Ok(resolved);
        }

        for extension in &self.manifest.supported_image_exts {
            let candidate = resolved.with_extension(extension);
            if self.root.join(&candidate).exists() {
                return Ok(candidate);
            }
        }

        bail!(
            "could not resolve graphics target `{target}` under {}",
            self.root.join(&resolved)
        )
    }
}

impl ProjectManifest {
    pub fn discover(root: &Utf8Path) -> Result<Self> {
        let mut raw_manifest = None;
        for candidate in ["00README.yaml", "00README.yml", "00README.json", "00README"] {
            let path = root.join(candidate);
            if !path.exists() {
                continue;
            }

            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read manifest file {}", path))?;
            raw_manifest = Some(if candidate.ends_with(".json") {
                serde_json::from_str::<RawManifest>(&contents)
                    .with_context(|| format!("failed to parse json manifest {}", path))?
            } else {
                serde_yaml::from_str::<RawManifest>(&contents)
                    .with_context(|| format!("failed to parse yaml manifest {}", path))?
            });
            break;
        }

        let compiler = raw_manifest
            .as_ref()
            .and_then(|manifest| manifest.compiler)
            .unwrap_or_default();
        let supported_image_exts = raw_manifest
            .as_ref()
            .and_then(|manifest| manifest.supported_image_exts.clone())
            .unwrap_or_else(|| match compiler {
                CompilerMode::PdfLatex | CompilerMode::XeLatex => {
                    vec![
                        "pdf".to_string(),
                        "png".to_string(),
                        "jpg".to_string(),
                        "jpeg".to_string(),
                    ]
                }
                CompilerMode::LatexDvipsPs2Pdf => vec!["eps".to_string(), "ps".to_string()],
            });

        let mut sources = Vec::new();
        let mut toplevels = Vec::new();
        if let Some(raw_manifest) = raw_manifest {
            for source in raw_manifest.sources {
                let source = match source {
                    RawSourceEntry::Path(path) => SourceEntry {
                        path: normalize_relative_path(Utf8Path::new(&path))
                            .with_context(|| format!("invalid source path `{path}`"))?,
                        usage: SourceUsage::Include,
                    },
                    RawSourceEntry::Detailed { path, usage } => SourceEntry {
                        path: normalize_relative_path(Utf8Path::new(&path))
                            .with_context(|| format!("invalid source path `{path}`"))?,
                        usage,
                    },
                };
                if source.usage == SourceUsage::Toplevel {
                    toplevels.push(source.path.clone());
                }
                sources.push(source);
            }

            for toplevel in raw_manifest.toplevel {
                toplevels.push(
                    normalize_relative_path(Utf8Path::new(&toplevel))
                        .with_context(|| format!("invalid toplevel path `{toplevel}`"))?,
                );
            }

            if toplevels.is_empty() {
                let default = root.join("main.tex");
                if default.exists() {
                    toplevels.push(Utf8PathBuf::from("main.tex"));
                }
            }

            if toplevels.is_empty() {
                bail!("manifest does not declare a toplevel TeX file");
            }

            return Ok(Self {
                compiler,
                texlive_version: raw_manifest.texlive_version.unwrap_or_default(),
                compile_from_root: raw_manifest.compile_from_root.unwrap_or(true),
                allow_shell_escape: raw_manifest.allow_shell_escape.unwrap_or(false),
                supported_image_exts,
                sources,
                toplevels,
            });
        }

        let default_main = root.join("main.tex");
        if !default_main.exists() {
            bail!("no manifest file found in {} and main.tex is missing", root);
        }

        Ok(Self {
            compiler,
            texlive_version: TexLiveVersion::default(),
            compile_from_root: true,
            allow_shell_escape: false,
            supported_image_exts,
            sources: vec![SourceEntry {
                path: Utf8PathBuf::from("main.tex"),
                usage: SourceUsage::Toplevel,
            }],
            toplevels: vec![Utf8PathBuf::from("main.tex")],
        })
    }
}

pub fn normalize_relative_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let mut normalized = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            Utf8Component::Normal(part) => normalized.push(part),
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                if !normalized.pop() {
                    bail!("path `{path}` escapes the project root");
                }
            }
            Utf8Component::RootDir | Utf8Component::Prefix(_) => {
                return Err(anyhow!(
                    "path `{path}` must be relative to the project root"
                ));
            }
        }
    }

    if normalized.as_str().is_empty() {
        bail!("path `{path}` resolves to an empty relative path");
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use camino::Utf8PathBuf;
    use tempfile::tempdir;

    use super::{
        CompilerMode, ProjectManifest, ProjectWorld, SourceUsage, TexLiveVersion,
        normalize_relative_path, read_tex_source_lossy,
    };

    #[test]
    fn tex_source_reader_preserves_valid_text_and_replaces_invalid_utf8() {
        let tempdir = tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("legacy.tex")).expect("utf8 temp path");
        fs::write(&path, b"before\xa0after").expect("write legacy source");

        let source = read_tex_source_lossy(&path).expect("read legacy source");

        assert_eq!(source, "before\u{fffd}after");
    }

    #[test]
    fn parses_yaml_manifest_and_collects_toplevels() {
        let tempdir = tempdir().expect("tempdir");
        let root =
            Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
        fs::write(
            root.join("00README.yaml"),
            r#"
compiler: pdf_latex
texlive_version: tl2023
compile_from_root: true
sources:
  - path: main.tex
    usage: toplevel
  - path: sections/intro.tex
    usage: include
"#,
        )
        .expect("write manifest");

        let manifest = ProjectManifest::discover(&root).expect("manifest");

        assert_eq!(manifest.compiler, CompilerMode::PdfLatex);
        assert_eq!(manifest.texlive_version, TexLiveVersion::Tl2023);
        assert_eq!(manifest.toplevels, vec![Utf8PathBuf::from("main.tex")]);
        assert_eq!(manifest.sources[1].usage, SourceUsage::Include);
    }

    #[test]
    fn parses_json_manifest_with_explicit_toplevel_list() {
        let tempdir = tempdir().expect("tempdir");
        let root =
            Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
        fs::write(
            root.join("00README.json"),
            r#"{
  "compiler": "xe_latex",
  "toplevel": ["paper/main.tex"],
  "supported_image_exts": ["pdf", "png"]
}"#,
        )
        .expect("write manifest");

        let manifest = ProjectManifest::discover(&root).expect("manifest");

        assert_eq!(manifest.compiler, CompilerMode::XeLatex);
        assert_eq!(
            manifest.toplevels,
            vec![Utf8PathBuf::from("paper/main.tex")]
        );
        assert_eq!(
            manifest.supported_image_exts,
            vec!["pdf".to_string(), "png".to_string()]
        );
    }

    #[test]
    fn rejects_root_escape_during_normalization() {
        let error =
            normalize_relative_path("../outside.tex".into()).expect_err("root escape should fail");
        assert!(error.to_string().contains("escapes the project root"));
    }

    #[test]
    fn resolves_graphics_using_compiler_extension_priority() {
        let tempdir = tempdir().expect("tempdir");
        let root =
            Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
        fs::create_dir_all(root.join("figures")).expect("figures dir");
        fs::write(
            root.join("00README.yaml"),
            r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#,
        )
        .expect("write manifest");
        fs::write(root.join("figures/plot.png"), b"png").expect("plot png");
        fs::write(root.join("figures/plot.pdf"), b"pdf").expect("plot pdf");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let resolved = world
            .resolve_graphics("sections/body.tex".into(), "../figures/plot")
            .expect("resolved graphic");

        assert_eq!(resolved, Utf8PathBuf::from("figures/plot.pdf"));
    }

    #[test]
    fn falls_back_to_main_tex_without_manifest() {
        let tempdir = tempdir().expect("tempdir");
        let root =
            Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 temp path");
        fs::write(root.join("main.tex"), b"\\documentclass{article}").expect("main tex");

        let manifest = ProjectManifest::discover(&root).expect("manifest");

        assert_eq!(manifest.toplevels, vec![Utf8PathBuf::from("main.tex")]);
    }
}
