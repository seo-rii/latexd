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
    BuildFinished {
        rev: RevId,
        success: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
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
    };

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
}
