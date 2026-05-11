use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationLabel {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationStyleHint {
    Numeric,
    AuthorYear,
    Textual,
    Parenthetical,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyRecordView {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelTargetView {
    pub key: String,
    pub number: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

pub trait AuxView {
    fn citation_label(&self, key: &str, style: CitationStyleHint) -> Option<CitationLabel>;

    fn bibliography_record(&self, key: &str) -> Option<BibliographyRecordView>;

    fn label_target(&self, key: &str) -> Option<LabelTargetView>;
}

impl AuxView for () {
    fn citation_label(&self, _key: &str, _style: CitationStyleHint) -> Option<CitationLabel> {
        None
    }

    fn bibliography_record(&self, _key: &str) -> Option<BibliographyRecordView> {
        None
    }

    fn label_target(&self, _key: &str) -> Option<LabelTargetView> {
        None
    }
}
