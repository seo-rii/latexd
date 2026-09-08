use serde::{Deserialize, Serialize};

pub type RevId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagePreviewArtifact {
    pub page_id: String,
    pub pdf_url: String,
    #[serde(default)]
    pub svg_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshotFile {
    pub file: String,
    pub content: String,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RenderIrArtifactUrls {
    pub legacy_output_url: String,
    pub events_url: String,
    pub document_ir_url: String,
    pub page_display_list_url: String,
    pub display_list_pdf_url: String,
    pub display_list_svg_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PagePatchOp {
    ReplacePage {
        index: usize,
        page_id: String,
        pdf_url: String,
        #[serde(default)]
        svg_url: Option<String>,
    },
    InsertPage {
        index: usize,
        page_id: String,
        pdf_url: String,
        #[serde(default)]
        svg_url: Option<String>,
    },
    DeletePage {
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    PreviewResyncRequired {
        preview_rev: RevId,
    },
    /// Pages and interaction sources are published in one transaction. A client
    /// missing the base must reload /api/state instead of applying partial ops.
    PreviewCommitted {
        base_preview_rev: RevId,
        new_preview_rev: RevId,
        pdf_url: String,
        page_ids: Vec<String>,
        page_artifacts: Vec<PagePreviewArtifact>,
        ops: Vec<PagePatchOp>,
        source_snapshot: Vec<SourceSnapshotFile>,
        render_ir_artifacts: Option<RenderIrArtifactUrls>,
    },
    BuildStarted {
        rev: RevId,
        changed_files: Vec<String>,
    },
    Diagnostics {
        rev: RevId,
        items: Vec<Diagnostic>,
    },
    FullPdfReady {
        rev: RevId,
        pdf_url: String,
        page_ids: Vec<String>,
        page_artifacts: Vec<PagePreviewArtifact>,
    },
    PatchPages {
        rev: RevId,
        ops: Vec<PagePatchOp>,
    },
    SourceSnapshot {
        rev: RevId,
        files: Vec<SourceSnapshotFile>,
    },
    BuildFinished {
        rev: RevId,
        success: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    PreviewLease {
        rev: RevId,
    },
    OpenDocument {
        doc: String,
    },
    ViewportChanged {
        zoom: f32,
        current_page: u32,
        scroll_top: f32,
        #[serde(default)]
        visible_pages: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        ClientMsg, Diagnostic, DiagnosticLevel, PagePatchOp, PagePreviewArtifact, ServerMsg,
        SourceSnapshotFile,
    };

    #[test]
    fn roundtrips_atomic_preview_commit_with_base_and_sources() {
        let message = ServerMsg::PreviewCommitted {
            base_preview_rev: 3,
            new_preview_rev: 5,
            pdf_url: "/artifacts/rev/5/main.pdf".into(),
            page_ids: vec!["page-a".into()],
            page_artifacts: vec![PagePreviewArtifact {
                page_id: "page-a".into(),
                pdf_url: "/artifacts/rev/1/pages/a.pdf".into(),
                svg_url: None,
            }],
            ops: Vec::new(),
            source_snapshot: vec![SourceSnapshotFile {
                file: "main.tex".into(),
                content: "new source".into(),
                line_count: 1,
            }],
            render_ir_artifacts: None,
        };
        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["type"], "preview_committed");
        assert_eq!(json["base_preview_rev"], 3);
        assert_eq!(json["new_preview_rev"], 5);
        assert_eq!(serde_json::from_value::<ServerMsg>(json).unwrap(), message);
    }

    #[test]
    fn roundtrips_server_messages() {
        let original = ServerMsg::Diagnostics {
            rev: 7,
            items: vec![Diagnostic {
                level: DiagnosticLevel::Error,
                file: Some("main.tex".to_string()),
                line: Some(12),
                message: "Undefined control sequence".to_string(),
            }],
        };

        let serialized = serde_json::to_string(&original).expect("serialize server msg");
        let restored =
            serde_json::from_str::<ServerMsg>(&serialized).expect("deserialize server msg");

        assert_eq!(restored, original);
    }

    #[test]
    fn roundtrips_patch_pages_messages() {
        let original = ServerMsg::PatchPages {
            rev: 8,
            ops: vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "page-a".to_string(),
                    pdf_url: "/artifacts/rev/8/pages/page-a.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/8/pages/page-a.svg".to_string()),
                },
                PagePatchOp::InsertPage {
                    index: 2,
                    page_id: "page-b".to_string(),
                    pdf_url: "/artifacts/rev/8/pages/page-b.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/8/pages/page-b.svg".to_string()),
                },
                PagePatchOp::DeletePage { index: 4 },
            ],
        };

        let serialized = serde_json::to_string(&original).expect("serialize patch msg");
        let restored =
            serde_json::from_str::<ServerMsg>(&serialized).expect("deserialize patch msg");

        assert_eq!(restored, original);
    }

    #[test]
    fn roundtrips_full_pdf_ready_with_page_ids() {
        let original = ServerMsg::FullPdfReady {
            rev: 9,
            pdf_url: "/artifacts/rev/9/main.pdf".to_string(),
            page_ids: vec!["page-a".to_string(), "page-b".to_string()],
            page_artifacts: vec![
                PagePreviewArtifact {
                    page_id: "page-a".to_string(),
                    pdf_url: "/artifacts/rev/9/pages/page-a.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/9/pages/page-a.svg".to_string()),
                },
                PagePreviewArtifact {
                    page_id: "page-b".to_string(),
                    pdf_url: "/artifacts/rev/9/pages/page-b.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/9/pages/page-b.svg".to_string()),
                },
            ],
        };

        let serialized = serde_json::to_string(&original).expect("serialize full pdf msg");
        let restored =
            serde_json::from_str::<ServerMsg>(&serialized).expect("deserialize full pdf msg");

        assert_eq!(restored, original);
    }

    #[test]
    fn roundtrips_source_snapshot_messages() {
        let original = ServerMsg::SourceSnapshot {
            rev: 10,
            files: vec![SourceSnapshotFile {
                file: "main.tex".to_string(),
                content: "\\section{Hello}\n".to_string(),
                line_count: 2,
            }],
        };

        let serialized = serde_json::to_string(&original).expect("serialize source snapshot");
        let restored =
            serde_json::from_str::<ServerMsg>(&serialized).expect("deserialize source snapshot");

        assert_eq!(restored, original);
    }

    #[test]
    fn roundtrips_client_messages() {
        let original = ClientMsg::ViewportChanged {
            zoom: 1.5,
            current_page: 3,
            scroll_top: 88.0,
            visible_pages: vec!["page-a".to_string(), "page-b".to_string()],
        };

        let serialized = serde_json::to_string(&original).expect("serialize client msg");
        let restored =
            serde_json::from_str::<ClientMsg>(&serialized).expect("deserialize client msg");

        assert_eq!(restored, original);
    }

    #[test]
    fn roundtrips_preview_lease_release_and_resync_request() {
        for rev in [0, 8] {
            let message = ClientMsg::PreviewLease { rev };
            let json = serde_json::to_value(&message).unwrap();
            assert_eq!(json["type"], "preview_lease");
            assert_eq!(serde_json::from_value::<ClientMsg>(json).unwrap(), message);
        }
        let message = ServerMsg::PreviewResyncRequired { preview_rev: 9 };
        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["type"], "preview_resync_required");
        assert_eq!(serde_json::from_value::<ServerMsg>(json).unwrap(), message);
    }
}
