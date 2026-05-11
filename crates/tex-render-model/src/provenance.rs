use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

pub const MAX_EXPANSION_FRAMES_IN_EVENT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub primary: ProvenanceSpan,
    #[serde(default)]
    pub related: Vec<RelatedSourceSpan>,
    #[serde(default)]
    pub expansion_stack: Vec<ExpansionFrame>,
    pub generated_by: GeneratedBy,
    #[serde(default)]
    pub expansion_stack_truncated: bool,
}

impl SourceProvenance {
    pub fn file(path: impl Into<Utf8PathBuf>, start_utf8: u32, end_utf8: u32) -> Self {
        Self {
            primary: ProvenanceSpan::File(SourceSpan {
                path: path.into(),
                start_utf8,
                end_utf8,
            }),
            related: Vec::new(),
            expansion_stack: Vec::new(),
            generated_by: GeneratedBy::Source,
            expansion_stack_truncated: false,
        }
    }

    pub fn generated(stable_id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            primary: ProvenanceSpan::Generated(GeneratedSpan {
                stable_id: stable_id.into(),
                description: description.into(),
            }),
            related: Vec::new(),
            expansion_stack: Vec::new(),
            generated_by: GeneratedBy::Generated,
            expansion_stack_truncated: false,
        }
    }

    pub fn with_related(mut self, role: SourceSpanRole, span: ProvenanceSpan) -> Self {
        self.related.push(RelatedSourceSpan { role, span });
        self
    }

    pub fn with_expansion_frame(mut self, frame: ExpansionFrame) -> Self {
        if self.expansion_stack.len() < MAX_EXPANSION_FRAMES_IN_EVENT {
            self.expansion_stack.push(frame);
        } else {
            self.expansion_stack_truncated = true;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvenanceSpan {
    File(SourceSpan),
    Generated(GeneratedSpan),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSpan {
    pub stable_id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedSourceSpan {
    pub role: SourceSpanRole,
    pub span: ProvenanceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSpanRole {
    Invocation,
    Argument,
    ArgumentContent,
    Definition,
    EmitSite,
    CitationKey,
    MetadataDefinition,
    SyntheticNumbering,
    FallbackSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpansionFrame {
    pub call_span: ProvenanceSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_span: Option<ProvenanceSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedBy {
    Source,
    MacroExpansion,
    Command,
    Shim,
    AuxFile,
    Fallback,
    Generated,
}

#[cfg(test)]
mod tests {
    use super::{
        ExpansionFrame, MAX_EXPANSION_FRAMES_IN_EVENT, ProvenanceSpan, SourceProvenance,
        SourceSpan, SourceSpanRole,
    };

    #[test]
    fn generated_provenance_roundtrips_through_json() {
        let provenance = SourceProvenance::generated("shim:article", "article class shim");
        let encoded = serde_json::to_string(&provenance).expect("encode provenance");
        let decoded: SourceProvenance = serde_json::from_str(&encoded).expect("decode provenance");

        assert_eq!(decoded, provenance);
    }

    #[test]
    fn expansion_stack_is_bounded() {
        let mut provenance = SourceProvenance::file("main.tex", 0, 1);
        for index in 0..MAX_EXPANSION_FRAMES_IN_EVENT + 2 {
            provenance = provenance.with_expansion_frame(ExpansionFrame {
                call_span: ProvenanceSpan::File(SourceSpan {
                    path: "main.tex".into(),
                    start_utf8: index as u32,
                    end_utf8: index as u32 + 1,
                }),
                definition_span: None,
                command_name: Some("macro".to_string()),
            });
        }

        assert_eq!(
            provenance.expansion_stack.len(),
            MAX_EXPANSION_FRAMES_IN_EVENT
        );
        assert!(provenance.expansion_stack_truncated);
    }

    #[test]
    fn related_spans_keep_roles() {
        let provenance = SourceProvenance::file("main.tex", 10, 20).with_related(
            SourceSpanRole::Invocation,
            ProvenanceSpan::File(SourceSpan {
                path: "main.tex".into(),
                start_utf8: 8,
                end_utf8: 21,
            }),
        );

        assert_eq!(provenance.related[0].role, SourceSpanRole::Invocation);
    }
}
