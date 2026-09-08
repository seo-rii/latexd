use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CompileTrace {
    pub schema_version: u32,
    pub engine: String,
    pub output_mode: String,
    pub total_us: u64,
    pub phases_us: BTreeMap<String, u64>,
    pub vm_executions: u64,
    pub dispatched_tokens: u64,
    #[serde(default)]
    pub legacy_layout_runs: u64,
    #[serde(default)]
    pub legacy_layout_source_bytes: u64,
    #[serde(default)]
    pub legacy_pdf_exports: u64,
    #[serde(default)]
    pub legacy_pdf_bytes: u64,
    #[serde(default)]
    pub legacy_layout_us: u64,
    #[serde(default)]
    pub legacy_pdf_us: u64,

    /// None means unmeasured, never inferred from equal page hashes.
    pub skipped_execution_tokens: Option<u64>,
    pub restored_prefix_source_bytes: u64,
    pub semantic_passes: usize,
    pub output_pages: usize,
    #[serde(default)]
    pub full_pdf_exports: usize,
    #[serde(default)]
    pub debug_artifact_exports: usize,
    pub reused_output_pages: usize,
    pub emitted_render_events: usize,
    pub external_processes: usize,
    pub checkpoint_fallback: Option<String>,
}

pub(crate) struct TraceClock {
    start: Instant,
    boundary: Instant,
    pub trace: CompileTrace,
}
impl TraceClock {
    pub fn new(engine: &str) -> Self {
        let now = Instant::now();
        Self {
            start: now,
            boundary: now,
            trace: CompileTrace {
                schema_version: 1,
                engine: engine.into(),
                ..CompileTrace::default()
            },
        }
    }
    pub fn phase(&mut self, name: &str) {
        let now = Instant::now();
        *self.trace.phases_us.entry(name.into()).or_default() +=
            now.duration_since(self.boundary)
                .as_micros()
                .min(u64::MAX as u128) as u64;
        self.boundary = now;
    }
    pub fn finish(mut self) -> CompileTrace {
        self.trace.total_us = self.start.elapsed().as_micros().min(u64::MAX as u128) as u64;
        self.trace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{CompileRequest, CompilerDriver};
    use camino::Utf8PathBuf;

    #[tokio::test]
    async fn compile_trace_counts_execution_separately_from_output_reuse() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        std::fs::write(
            root.join("main.tex"),
            r"\documentclass{article}\begin{document}Alpha\newpage Bravo\end{document}",
        )
        .unwrap();
        let build_root = root.join(".latexd/build");
        let driver = CompilerDriver::new(Some("internal".into()), Vec::new());
        for rev in 1..=2 {
            if rev == 2 {
                let source = std::fs::read_to_string(root.join("main.tex")).unwrap();
                std::fs::write(root.join("main.tex"), source.replace("Alpha", "Delta")).unwrap();
            }
            let outcome = driver
                .compile(CompileRequest {
                    root: root.clone(),
                    manifest: tex_world::ProjectManifest::discover(&root).unwrap(),
                    toplevel: "main.tex".into(),
                    rev,
                    build_root: build_root.clone(),
                    changed_files: vec!["main.tex".into()],
                })
                .await
                .unwrap();
            let metadata: serde_json::Value = serde_json::from_slice(
                &std::fs::read(build_root.join(format!("rev-{rev}/build-meta.json"))).unwrap(),
            )
            .unwrap();
            let trace: CompileTrace = serde_json::from_value(metadata["trace"].clone()).unwrap();
            assert_eq!(trace.schema_version, 1);
            assert_eq!(trace.engine, "internal");
            assert_eq!(trace.output_mode, "supported_approximate");
            assert!(trace.vm_executions > 0);
            assert!(trace.dispatched_tokens > 0, "{trace:?}");
            assert!(trace.legacy_layout_runs > 0, "{trace:?}");
            assert!(trace.legacy_layout_source_bytes > 0);
            assert_eq!(trace.legacy_pdf_exports, 0);
            assert_eq!(trace.legacy_pdf_bytes, 0);
            assert_eq!(trace.legacy_pdf_us, 0);
            assert!(
                std::fs::read(&outcome.pdf_path)
                    .unwrap()
                    .starts_with(b"%PDF-")
            );
            assert_eq!(trace.skipped_execution_tokens, None);
            assert_eq!(trace.output_pages, outcome.renderer_page_metadata.len());
            assert!(trace.total_us >= trace.phases_us.values().sum());
            assert_eq!(trace.external_processes, 0);
            if rev == 2 {
                assert!(trace.reused_output_pages > 0, "{trace:?}");
            }
        }
    }

}
