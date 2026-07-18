use std::{
    collections::{BTreeMap, HashSet},
    fs,
};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use tex_render_model::{
    AuxView, BibliographyRecordView, CitationLabel, CitationLabelForm, CitationStyleHint,
    LabelTargetView,
};
use tex_world::{normalize_relative_path, read_tex_source_lossy};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAux {
    #[serde(default)]
    pub labels: Vec<SemanticLabel>,
    #[serde(default)]
    pub toc: Vec<TocEntry>,
    #[serde(default)]
    pub citation_keys: Vec<String>,
    #[serde(default)]
    pub bibliography_inputs: Vec<Utf8PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibliography_style: Option<String>,
    #[serde(default, skip_serializing_if = "CitationMode::is_auto")]
    pub citation_mode: CitationMode,
    #[serde(default)]
    pub citation_aliases: Vec<CitationAlias>,
    #[serde(default)]
    pub bibliography: Vec<BibliographyEntry>,
    #[serde(default)]
    pub bibliography_titles: Vec<BibliographyTitle>,
    #[serde(default)]
    pub bibliography_authors: Vec<BibliographyAuthor>,
    #[serde(default)]
    pub bibliography_years: Vec<BibliographyYear>,
    #[serde(default)]
    pub bibliography_fields: Vec<BibliographyField>,
    #[serde(default)]
    pub bibliography_urls: Vec<BibliographyUrl>,
    #[serde(default)]
    pub bibliography_dois: Vec<BibliographyDoi>,
    #[serde(default)]
    pub bibliography_eprints: Vec<BibliographyEprint>,
    #[serde(default)]
    pub float_captions: Vec<FloatCaption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CitationMode {
    #[default]
    Auto,
    Numeric,
    AuthorYear,
}

impl CitationMode {
    fn is_auto(&self) -> bool {
        *self == Self::Auto
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Numeric => "numeric",
            Self::AuthorYear => "author_year",
        }
    }

    fn parse(value: &str) -> Self {
        match value.trim() {
            "numeric" => Self::Numeric,
            "author_year" => Self::AuthorYear,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAuxIndex {
    #[serde(default)]
    pub has_table_of_contents: bool,
    #[serde(default)]
    pub has_bibliography_heading: bool,
    #[serde(default)]
    pub bibliography_inputs: Vec<Utf8PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibliography_style: Option<String>,
    #[serde(default)]
    pub label_count: usize,
    #[serde(default)]
    pub toc_count: usize,
    #[serde(default)]
    pub citation_key_count: usize,
    #[serde(default)]
    pub bibliography_entry_count: usize,
    #[serde(default)]
    pub files: Vec<SemanticAuxFileSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAuxFileSummary {
    pub path: Utf8PathBuf,
    #[serde(default)]
    pub label_keys: Vec<String>,
    #[serde(default)]
    pub toc: Vec<SemanticAuxTocSummary>,
    #[serde(default)]
    pub citation_keys: Vec<String>,
    #[serde(default)]
    pub bibliography_keys: Vec<String>,
    #[serde(default)]
    pub bibliography_entries: Vec<SemanticAuxBibliographySummary>,
    #[serde(default)]
    pub float_captions: Vec<SemanticAuxFloatSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAuxTocSummary {
    #[serde(default)]
    pub level: u8,
    pub number: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAuxFloatSummary {
    pub kind: String,
    pub number: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAuxBibliographySummary {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eprint: Option<String>,
    #[serde(default)]
    pub fields: Vec<SemanticAuxNamedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticAuxNamedValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticLabel {
    pub key: String,
    pub number: String,
    pub page: u32,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocEntry {
    pub level: u8,
    pub number: String,
    pub title: String,
    pub page: u32,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyEntry {
    pub key: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyTitle {
    pub key: String,
    pub title: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyAuthor {
    pub key: String,
    pub author: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyYear {
    pub key: String,
    pub year: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyField {
    pub key: String,
    pub field: String,
    pub value: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyUrl {
    pub key: String,
    pub url: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyDoi {
    pub key: String,
    pub doi: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibliographyEprint {
    pub key: String,
    pub eprint: String,
    pub file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloatCaption {
    pub kind: String,
    pub number: String,
    pub title: String,
    pub body_title: String,
    pub page: u32,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationAlias {
    pub key: String,
    pub text: String,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedBibliographyEntry {
    key: String,
    text: String,
    label: Option<String>,
    title: Option<String>,
    author: Option<String>,
    year: Option<String>,
    fields: Vec<(String, String)>,
    url: Option<String>,
    doi: Option<String>,
    eprint: Option<String>,
    file: Utf8PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSourceSlice {
    pub page_index: usize,
    pub source_spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub file: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

impl AuxView for SemanticAux {
    fn citation_label(&self, key: &str, style: CitationStyleHint) -> Option<CitationLabel> {
        if let Some(alias) = self.citation_aliases.iter().find(|alias| alias.key == key) {
            return Some(CitationLabel {
                text: alias.text.clone(),
                form: citation_label_form(style),
            });
        }
        if let Some(entry) = self.bibliography.iter().find(|entry| entry.key == key) {
            let fallback_position = self
                .bibliography
                .iter()
                .position(|entry| entry.key == key)
                .unwrap_or_default()
                + 1;
            if self.citation_mode == CitationMode::Numeric || entry.label.is_none() {
                let normalized = entry
                    .label
                    .as_deref()
                    .map(normalize_bibliography_inline_markup);
                let numeric_label = normalized
                    .as_deref()
                    .filter(|label| parse_citation_label(label).1.is_none())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| fallback_position.to_string());
                if style == CitationStyleHint::Textual
                    && let Some(author) = entry
                        .label
                        .as_deref()
                        .and_then(|label| parse_citation_label(label).0)
                        .filter(|author| !author.is_empty())
                {
                    return Some(CitationLabel {
                        text: format!("{author} [{fallback_position}]"),
                        form: CitationLabelForm::Textual,
                    });
                }
                return Some(CitationLabel {
                    text: numeric_label,
                    form: CitationLabelForm::Numeric,
                });
            }

            let normalized = normalize_bibliography_inline_markup(
                entry.label.as_deref().expect("label checked above"),
            );
            if style == CitationStyleHint::Numeric {
                return Some(CitationLabel {
                    text: normalized,
                    form: CitationLabelForm::Numeric,
                });
            }
            let (author, year, _) = parse_citation_label(&normalized);
            if year.is_none() {
                return Some(CitationLabel {
                    text: normalized,
                    form: CitationLabelForm::Numeric,
                });
            }
            let text = match (author, year) {
                (Some(author), Some(year)) if style == CitationStyleHint::Textual => {
                    format!("{author} ({year})")
                }
                (Some(author), Some(year)) => format!("{author}, {year}"),
                (Some(author), None) => author,
                (None, Some(year)) => year,
                (None, None) => normalized,
            };
            return Some(CitationLabel {
                text,
                form: citation_label_form(style),
            });
        }
        None
    }

    fn bibliography_record(&self, key: &str) -> Option<BibliographyRecordView> {
        self.bibliography
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| BibliographyRecordView {
                key: entry.key.clone(),
                label: entry.label.clone(),
                text: entry.text.clone(),
            })
    }

    fn label_target(&self, key: &str) -> Option<LabelTargetView> {
        self.labels
            .iter()
            .find(|label| label.key == key)
            .map(|label| LabelTargetView {
                key: label.key.clone(),
                number: label.number.clone(),
                page: Some(label.page),
            })
    }
}

fn citation_label_form(style: CitationStyleHint) -> CitationLabelForm {
    match style {
        CitationStyleHint::Numeric => CitationLabelForm::Numeric,
        CitationStyleHint::Textual => CitationLabelForm::Textual,
        CitationStyleHint::AuthorYear
        | CitationStyleHint::Parenthetical
        | CitationStyleHint::Unknown => CitationLabelForm::Parenthetical,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScan {
    pub files: BTreeMap<Utf8PathBuf, String>,
    pub sections: Vec<SectionSite>,
    pub equations: Vec<EquationSite>,
    pub floats: Vec<FloatSite>,
    pub blocks: Vec<BlockSite>,
    pub captions: Vec<CaptionSite>,
    pub has_float_lists: bool,
    pub has_bibliography_heading: bool,
    pub appendices: Vec<AppendixSite>,
    pub labels: Vec<LabelSite>,
    pub citations: Vec<CitationSite>,
    pub citation_aliases: Vec<CitationAliasSite>,
    pub bibliography_files: Vec<Utf8PathBuf>,
    pub bibliography_style: Option<String>,
    pub custom_block_environments: Vec<String>,
    pub has_table_of_contents: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedProject {
    pub scan: ProjectScan,
    pub files: BTreeMap<Utf8PathBuf, String>,
    pub rewrite_spans: BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>,
    pub tracked_inputs: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaterializedRewriteSpan {
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub output_start_utf8: u32,
    pub output_end_utf8: u32,
    pub rendered: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionSite {
    pub level: u8,
    pub toc_title: String,
    pub body_title: String,
    pub numbered: bool,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendixSite {
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquationSite {
    pub numbered: bool,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatSite {
    pub kind: FloatKind,
    pub numbered: bool,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionSite {
    pub kind: FloatKind,
    pub numbered: bool,
    pub list_title: String,
    pub body_title: String,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSite {
    pub kind: BlockKind,
    pub numbered: bool,
    pub counter_key: String,
    pub within_level: Option<u8>,
    pub title: Option<String>,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatKind {
    Figure,
    Table,
    Algorithm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Theorem,
    Lemma,
    Proposition,
    Corollary,
    Definition,
    Remark,
    Claim,
    Example,
    Assumption,
    Conjecture,
    Axiom,
    Fact,
    Observation,
    Problem,
    Exercise,
    Question,
    Notation,
    Custom {
        display_name: String,
        lower_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSite {
    pub key: String,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationSite {
    pub keys: Vec<String>,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationAliasSite {
    pub key: String,
    pub text: String,
    pub file: Utf8PathBuf,
    pub offset_utf8: u32,
}

#[derive(Debug, Clone)]
enum CommandEvent {
    Section {
        offset: u32,
        level: u8,
        toc_title: String,
        body_title: String,
        numbered: bool,
    },
    Label {
        offset: u32,
        key: String,
    },
    Cite {
        offset: u32,
        keys: Vec<String>,
    },
    CitationAlias {
        offset: u32,
        key: String,
        text: String,
    },
    Equation {
        offset: u32,
        numbered: bool,
    },
    Float {
        offset: u32,
        kind: FloatKind,
        numbered: bool,
    },
    Caption {
        offset: u32,
        kind: FloatKind,
        numbered: bool,
        list_title: String,
        body_title: String,
    },
    Block {
        offset: u32,
        environment: String,
        title: Option<String>,
    },
    NewTheoremDefinition {
        environment: String,
        display_name: String,
        numbered: bool,
        shared_counter: Option<String>,
        within_counter: Option<String>,
    },
    FloatList,
    BibliographyHeading,
    TableOfContents,
    Bibliography {
        stems: Vec<String>,
    },
    BibliographyStyle {
        style: String,
    },
    Appendix {
        offset: u32,
    },
    Input {
        path: Utf8PathBuf,
        is_include: bool,
    },
    IncludeOnly {
        paths: Vec<Utf8PathBuf>,
    },
}

impl SemanticAux {
    pub fn equivalent_to(&self, other: &Self) -> bool {
        self.labels == other.labels
            && self.toc == other.toc
            && self.citation_keys == other.citation_keys
            && self.bibliography_inputs == other.bibliography_inputs
            && self.bibliography_style == other.bibliography_style
            && self.citation_mode == other.citation_mode
            && self.citation_aliases == other.citation_aliases
            && self.bibliography == other.bibliography
            && self.bibliography_titles == other.bibliography_titles
            && self.bibliography_authors == other.bibliography_authors
            && self.bibliography_years == other.bibliography_years
            && self.bibliography_fields == other.bibliography_fields
            && self.bibliography_urls == other.bibliography_urls
            && self.bibliography_dois == other.bibliography_dois
            && self.bibliography_eprints == other.bibliography_eprints
            && self.float_captions == other.float_captions
    }

    pub fn label_number(&self, key: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|label| label.key == key)
            .map(|label| label.number.as_str())
    }

    pub fn label_page(&self, key: &str) -> Option<u32> {
        self.labels
            .iter()
            .find(|label| label.key == key)
            .map(|label| label.page)
    }

    pub fn label_title(&self, key: &str) -> Option<&str> {
        let label = self.labels.iter().find(|label| label.key == key)?;
        let latest_heading = self
            .toc
            .iter()
            .filter(|entry| entry.file == label.file && entry.offset_utf8 <= label.offset_utf8)
            .max_by_key(|entry| entry.offset_utf8);
        let latest_caption = self
            .float_captions
            .iter()
            .filter(|caption| {
                caption.file == label.file && caption.offset_utf8 <= label.offset_utf8
            })
            .max_by_key(|caption| caption.offset_utf8);
        if latest_caption.is_some_and(|caption| {
            latest_heading
                .map(|entry| caption.offset_utf8 > entry.offset_utf8)
                .unwrap_or(true)
        }) {
            latest_caption.map(|caption| caption.body_title.as_str())
        } else {
            latest_heading.map(|entry| entry.title.as_str())
        }
    }

    pub fn bibliography_number(&self, key: &str) -> Option<usize> {
        self.bibliography
            .iter()
            .position(|entry| entry.key == key)
            .map(|index| index + 1)
    }

    pub fn citation_author(&self, key: &str) -> Option<String> {
        let label_author = self
            .bibliography
            .iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| entry.label.as_deref())
            .and_then(|label| parse_citation_label(label).0);
        label_author.or_else(|| {
            self.bibliography_authors
                .iter()
                .find(|entry| entry.key == key)
                .map(|entry| entry.author.clone())
        })
    }

    pub fn citation_full_author(&self, key: &str) -> Option<String> {
        let label_author = self
            .bibliography
            .iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| entry.label.as_deref())
            .and_then(|label| parse_citation_label(label).2);
        label_author.or_else(|| {
            self.bibliography_authors
                .iter()
                .find(|entry| entry.key == key)
                .map(|entry| entry.author.clone())
        })
    }

    pub fn citation_display_author(&self, key: &str, starred: bool) -> Option<String> {
        if starred {
            self.citation_full_author(key)
                .or_else(|| self.citation_author(key))
        } else {
            self.citation_author(key)
        }
    }

    pub fn citation_year(&self, key: &str) -> Option<String> {
        let label_year = self
            .bibliography
            .iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| entry.label.as_deref())
            .and_then(|label| parse_citation_label(label).1);
        label_year.or_else(|| {
            self.bibliography_years
                .iter()
                .find(|entry| entry.key == key)
                .map(|entry| entry.year.clone())
        })
    }

    pub fn bibliography_text(&self, key: &str) -> Option<&str> {
        self.bibliography
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.text.as_str())
    }

    pub fn citation_title(&self, key: &str) -> Option<&str> {
        self.bibliography_titles
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.title.as_str())
            .or_else(|| {
                self.bibliography_text(key)
                    .map(|text| text.trim().trim_end_matches('.'))
            })
    }

    pub fn citation_field(&self, key: &str, field_name: &str) -> Option<&str> {
        self.bibliography_fields
            .iter()
            .find(|entry| entry.key == key && entry.field == field_name)
            .map(|entry| entry.value.as_str())
    }

    pub fn citation_url(&self, key: &str) -> Option<&str> {
        self.bibliography_urls
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.url.as_str())
    }

    pub fn citation_doi(&self, key: &str) -> Option<&str> {
        self.bibliography_dois
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.doi.as_str())
    }

    pub fn citation_eprint(&self, key: &str) -> Option<&str> {
        self.bibliography_eprints
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.eprint.as_str())
    }

    pub fn citation_alias_text(&self, key: &str) -> Option<&str> {
        self.citation_aliases
            .iter()
            .find(|alias| alias.key == key)
            .map(|alias| alias.text.as_str())
    }
}

impl BlockKind {
    fn from_environment(environment: &str) -> Option<Self> {
        match environment {
            "theorem" | "theorem*" => Some(Self::Theorem),
            "lemma" | "lemma*" => Some(Self::Lemma),
            "proposition" | "proposition*" => Some(Self::Proposition),
            "corollary" | "corollary*" => Some(Self::Corollary),
            "definition" | "definition*" => Some(Self::Definition),
            "remark" | "remark*" => Some(Self::Remark),
            "claim" | "claim*" => Some(Self::Claim),
            "example" | "example*" => Some(Self::Example),
            "assumption" | "assumption*" => Some(Self::Assumption),
            "conjecture" | "conjecture*" => Some(Self::Conjecture),
            "axiom" | "axiom*" => Some(Self::Axiom),
            "fact" | "fact*" => Some(Self::Fact),
            "observation" | "observation*" => Some(Self::Observation),
            "problem" | "problem*" => Some(Self::Problem),
            "exercise" | "exercise*" => Some(Self::Exercise),
            "question" | "question*" => Some(Self::Question),
            "notation" | "notation*" => Some(Self::Notation),
            _ => None,
        }
    }

    fn custom(display_name: &str) -> Self {
        let display_name = display_name.trim().to_string();
        let lower_name = display_name.to_lowercase();
        Self::Custom {
            display_name,
            lower_name,
        }
    }

    fn display_name(&self) -> &str {
        match self {
            Self::Theorem => "Theorem",
            Self::Lemma => "Lemma",
            Self::Proposition => "Proposition",
            Self::Corollary => "Corollary",
            Self::Definition => "Definition",
            Self::Remark => "Remark",
            Self::Claim => "Claim",
            Self::Example => "Example",
            Self::Assumption => "Assumption",
            Self::Conjecture => "Conjecture",
            Self::Axiom => "Axiom",
            Self::Fact => "Fact",
            Self::Observation => "Observation",
            Self::Problem => "Problem",
            Self::Exercise => "Exercise",
            Self::Question => "Question",
            Self::Notation => "Notation",
            Self::Custom { display_name, .. } => display_name.as_str(),
        }
    }

    fn lower_name(&self) -> &str {
        match self {
            Self::Theorem => "theorem",
            Self::Lemma => "lemma",
            Self::Proposition => "proposition",
            Self::Corollary => "corollary",
            Self::Definition => "definition",
            Self::Remark => "remark",
            Self::Claim => "claim",
            Self::Example => "example",
            Self::Assumption => "assumption",
            Self::Conjecture => "conjecture",
            Self::Axiom => "axiom",
            Self::Fact => "fact",
            Self::Observation => "observation",
            Self::Problem => "problem",
            Self::Exercise => "exercise",
            Self::Question => "question",
            Self::Notation => "notation",
            Self::Custom { lower_name, .. } => lower_name.as_str(),
        }
    }
}

fn parse_citation_label(label: &str) -> (Option<String>, Option<String>, Option<String>) {
    let label = normalize_bibliography_inline_markup(label);
    let label = label.trim();
    let mut year = None;
    let mut year_start = None;
    for (index, _) in label.char_indices() {
        let Some(candidate) = label.get(index..index + 4) else {
            continue;
        };
        if candidate.chars().all(|ch| ch.is_ascii_digit()) {
            let suffix = label
                .get(index + 4..)
                .and_then(|rest| rest.chars().next())
                .filter(|ch| ch.is_ascii_lowercase())
                .map(|ch| ch.to_string())
                .unwrap_or_default();
            year = Some(format!("{candidate}{suffix}"));
            year_start = Some(index);
            break;
        }
    }

    let short_author = if let Some(open_paren) = label.find('(') {
        Some(
            label[..open_paren]
                .trim_end_matches(|ch: char| ch == ',' || ch.is_whitespace())
                .to_string(),
        )
    } else if let Some(year_start) = year_start {
        Some(
            label[..year_start]
                .trim_end_matches(|ch: char| ch == ',' || ch.is_whitespace())
                .to_string(),
        )
    } else if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    };

    let full_author = label
        .rfind(')')
        .and_then(|index| label.get(index + 1..))
        .map(str::trim)
        .filter(|tail| !tail.is_empty())
        .map(ToOwned::to_owned);

    (short_author, year, full_author)
}

fn normalize_bibliography_inline_markup(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < source.len() {
        let remaining = &source[index..];
        let matched = if remaining.starts_with("\\natexlab") {
            Some("\\natexlab".len())
        } else if remaining.starts_with("\\NAT@exlab") {
            Some("\\NAT@exlab".len())
        } else {
            None
        };
        if let Some(prefix_len) = matched
            && let Some((argument_end, argument)) = read_braced_argument(source, index + prefix_len)
        {
            output.push_str(argument.trim());
            index = argument_end;
            continue;
        }
        let Some(ch) = remaining.chars().next() else {
            break;
        };
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

pub fn scan_project(root: &Utf8Path, toplevel: &Utf8Path) -> Result<ProjectScan> {
    let mut files = BTreeMap::new();
    let mut sections = Vec::new();
    let mut equations = Vec::new();
    let mut floats = Vec::new();
    let mut blocks = Vec::new();
    let mut captions = Vec::new();
    let mut has_float_lists = false;
    let mut has_bibliography_heading = false;
    let mut appendices = Vec::new();
    let mut labels = Vec::new();
    let mut citations = Vec::new();
    let mut citation_aliases = Vec::new();
    let mut bibliography_files = Vec::new();
    let mut seen_bibliography_files = HashSet::new();
    let mut bibliography_style = None::<String>;
    let mut custom_block_definitions =
        BTreeMap::<String, (BlockKind, bool, String, Option<u8>)>::new();
    let mut custom_block_environments = Vec::<String>::new();
    let mut seen_custom_block_environments = HashSet::<String>::new();
    let mut has_table_of_contents = false;
    let mut active = HashSet::new();
    let mut include_only = None;
    let mut order = 0usize;

    fn walk(
        root: &Utf8Path,
        path: &Utf8Path,
        files: &mut BTreeMap<Utf8PathBuf, String>,
        sections: &mut Vec<SectionSite>,
        equations: &mut Vec<EquationSite>,
        floats: &mut Vec<FloatSite>,
        blocks: &mut Vec<BlockSite>,
        captions: &mut Vec<CaptionSite>,
        has_float_lists: &mut bool,
        has_bibliography_heading: &mut bool,
        appendices: &mut Vec<AppendixSite>,
        labels: &mut Vec<LabelSite>,
        citations: &mut Vec<CitationSite>,
        citation_aliases: &mut Vec<CitationAliasSite>,
        bibliography_files: &mut Vec<Utf8PathBuf>,
        seen_bibliography_files: &mut HashSet<Utf8PathBuf>,
        bibliography_style: &mut Option<String>,
        custom_block_definitions: &mut BTreeMap<String, (BlockKind, bool, String, Option<u8>)>,
        custom_block_environments: &mut Vec<String>,
        seen_custom_block_environments: &mut HashSet<String>,
        has_table_of_contents: &mut bool,
        active: &mut HashSet<Utf8PathBuf>,
        include_only: &mut Option<HashSet<Utf8PathBuf>>,
        order: &mut usize,
    ) -> Result<()> {
        let path = resolve_existing_project_path(root, path).unwrap_or_else(|| path.to_path_buf());
        if !active.insert(path.to_path_buf()) {
            return Ok(());
        }
        let source = read_tex_source_lossy(&root.join(&path))
            .with_context(|| format!("failed to read source {}", root.join(&path)))?;
        files
            .entry(path.to_path_buf())
            .or_insert_with(|| source.clone());
        for event in scan_source(&source) {
            match event {
                CommandEvent::Section {
                    offset,
                    level,
                    toc_title,
                    body_title,
                    numbered,
                } => {
                    sections.push(SectionSite {
                        level,
                        toc_title,
                        body_title,
                        numbered,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                        order: *order,
                    });
                    *order += 1;
                }
                CommandEvent::Label { offset, key } => {
                    labels.push(LabelSite {
                        key,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                        order: *order,
                    });
                    *order += 1;
                }
                CommandEvent::Cite { offset, keys } => {
                    citations.push(CitationSite {
                        keys,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                    });
                }
                CommandEvent::CitationAlias { offset, key, text } => {
                    citation_aliases.push(CitationAliasSite {
                        key,
                        text,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                    });
                }
                CommandEvent::Equation { offset, numbered } => {
                    equations.push(EquationSite {
                        numbered,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                        order: *order,
                    });
                    *order += 1;
                }
                CommandEvent::Float {
                    offset,
                    kind,
                    numbered,
                } => {
                    floats.push(FloatSite {
                        kind,
                        numbered,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                        order: *order,
                    });
                    *order += 1;
                }
                CommandEvent::Block {
                    offset,
                    environment,
                    title,
                } => {
                    if let Some((kind, numbered, counter_key, within_level)) =
                        custom_block_definitions
                            .get(environment.trim_end_matches('*'))
                            .cloned()
                            .or_else(|| {
                                BlockKind::from_environment(environment.trim_end_matches('*')).map(
                                    |kind| {
                                        (
                                            kind,
                                            !environment.ends_with('*'),
                                            environment.trim_end_matches('*').to_string(),
                                            None,
                                        )
                                    },
                                )
                            })
                    {
                        blocks.push(BlockSite {
                            kind,
                            numbered,
                            counter_key,
                            within_level,
                            title,
                            file: path.to_path_buf(),
                            offset_utf8: offset,
                            order: *order,
                        });
                        *order += 1;
                    }
                }
                CommandEvent::NewTheoremDefinition {
                    environment,
                    display_name,
                    numbered,
                    shared_counter,
                    within_counter,
                } => {
                    let environment = environment.trim().to_string();
                    if environment.is_empty() {
                        continue;
                    }
                    if seen_custom_block_environments.insert(environment.clone()) {
                        custom_block_environments.push(environment.clone());
                    }
                    let mut inherited_within_level = None;
                    let counter_key = shared_counter
                        .as_deref()
                        .map(str::trim)
                        .filter(|shared| !shared.is_empty())
                        .map(|shared| shared.trim_end_matches('*'))
                        .map(|shared| {
                            if let Some((_, _, counter_key, within_level)) =
                                custom_block_definitions.get(shared)
                            {
                                inherited_within_level = *within_level;
                                counter_key.clone()
                            } else {
                                shared.to_string()
                            }
                        })
                        .unwrap_or_else(|| environment.clone());
                    let within_level = within_counter
                        .as_deref()
                        .map(str::trim)
                        .filter(|within| !within.is_empty())
                        .and_then(|within| match within.trim_end_matches('*') {
                            "chapter" => Some(0),
                            "section" => Some(1),
                            "subsection" => Some(2),
                            "subsubsection" => Some(3),
                            "paragraph" => Some(4),
                            "subparagraph" => Some(5),
                            _ => None,
                        })
                        .or(inherited_within_level);
                    custom_block_definitions.insert(
                        environment,
                        (
                            BlockKind::custom(&display_name),
                            numbered,
                            counter_key,
                            within_level,
                        ),
                    );
                }
                CommandEvent::Caption {
                    offset,
                    kind,
                    numbered,
                    list_title,
                    body_title,
                } => {
                    captions.push(CaptionSite {
                        kind,
                        numbered,
                        list_title,
                        body_title,
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                        order: *order,
                    });
                    *order += 1;
                }
                CommandEvent::FloatList => *has_float_lists = true,
                CommandEvent::BibliographyHeading => *has_bibliography_heading = true,
                CommandEvent::Appendix { offset } => {
                    appendices.push(AppendixSite {
                        file: path.to_path_buf(),
                        offset_utf8: offset,
                        order: *order,
                    });
                    *order += 1;
                }
                CommandEvent::TableOfContents => *has_table_of_contents = true,
                CommandEvent::Bibliography { stems } => {
                    for stem in stems {
                        let Ok(mut path) = normalize_relative_path(Utf8Path::new(&stem)) else {
                            continue;
                        };
                        if path.extension() == Some("bib") {
                            path = path.with_extension("bbl");
                        }
                        if path.extension().is_none() {
                            path = path.with_extension("bbl");
                        }
                        if root.join(&path).is_file()
                            && seen_bibliography_files.insert(path.clone())
                        {
                            bibliography_files.push(path);
                        }
                    }
                }
                CommandEvent::BibliographyStyle { style } => {
                    *bibliography_style = Some(style);
                }
                CommandEvent::Input {
                    path: input_path,
                    is_include,
                } => {
                    let input_path = resolve_existing_project_path(root, &input_path)
                        .unwrap_or_else(|| input_path.clone());
                    if is_include
                        && include_only
                            .as_ref()
                            .is_some_and(|paths| !paths.contains(&input_path))
                    {
                        continue;
                    }
                    if !root.join(&input_path).is_file() {
                        continue;
                    }
                    files.entry(input_path.clone()).or_insert_with(|| {
                        read_tex_source_lossy(&root.join(&input_path)).unwrap_or_default()
                    });
                    if input_path.extension() == Some("bbl") {
                        if seen_bibliography_files.insert(input_path.clone()) {
                            bibliography_files.push(input_path);
                        }
                    } else {
                        walk(
                            root,
                            &input_path,
                            files,
                            sections,
                            equations,
                            floats,
                            blocks,
                            captions,
                            has_float_lists,
                            has_bibliography_heading,
                            appendices,
                            labels,
                            citations,
                            citation_aliases,
                            bibliography_files,
                            seen_bibliography_files,
                            bibliography_style,
                            custom_block_definitions,
                            custom_block_environments,
                            seen_custom_block_environments,
                            has_table_of_contents,
                            active,
                            include_only,
                            order,
                        )?;
                    }
                }
                CommandEvent::IncludeOnly { paths } => {
                    *include_only = Some(paths.into_iter().collect());
                }
            }
        }
        active.remove(&path);
        Ok(())
    }

    walk(
        root,
        toplevel,
        &mut files,
        &mut sections,
        &mut equations,
        &mut floats,
        &mut blocks,
        &mut captions,
        &mut has_float_lists,
        &mut has_bibliography_heading,
        &mut appendices,
        &mut labels,
        &mut citations,
        &mut citation_aliases,
        &mut bibliography_files,
        &mut seen_bibliography_files,
        &mut bibliography_style,
        &mut custom_block_definitions,
        &mut custom_block_environments,
        &mut seen_custom_block_environments,
        &mut has_table_of_contents,
        &mut active,
        &mut include_only,
        &mut order,
    )?;

    let jobname_bibliography = toplevel.with_extension("bbl");
    if root.join(&jobname_bibliography).is_file() {
        bibliography_files.clear();
        bibliography_files.push(jobname_bibliography);
    }

    for path in &bibliography_files {
        let full_path = root.join(path);
        if full_path.exists() {
            files
                .entry(path.clone())
                .or_insert_with(|| read_tex_source_lossy(&full_path).unwrap_or_default());
        }
    }

    Ok(ProjectScan {
        files,
        sections,
        equations,
        floats,
        blocks,
        captions,
        has_float_lists,
        has_bibliography_heading,
        appendices,
        labels,
        citations,
        citation_aliases,
        bibliography_files,
        bibliography_style,
        custom_block_environments,
        has_table_of_contents,
    })
}

fn infer_project_citation_mode(
    files: &BTreeMap<Utf8PathBuf, String>,
    bibliography_style: Option<&str>,
) -> CitationMode {
    let mut saw_author_year = false;
    for source in files.values() {
        match scan_source_citation_mode(source) {
            CitationMode::Numeric => return CitationMode::Numeric,
            CitationMode::AuthorYear => saw_author_year = true,
            CitationMode::Auto => {}
        }
    }
    if saw_author_year {
        return CitationMode::AuthorYear;
    }

    let style = bibliography_style
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if [
        "plain", "unsrt", "abbrv", "alpha", "ieeetr", "ieeetran", "siam", "splncs",
    ]
    .iter()
    .any(|candidate| style == *candidate || style.ends_with(candidate))
    {
        CitationMode::Numeric
    } else {
        CitationMode::Auto
    }
}

fn scan_source_citation_mode(source: &str) -> CitationMode {
    let mut index = 0usize;
    let mut mode = CitationMode::Auto;
    while index < source.len() {
        let next_command = source[index..].find('\\').map(|offset| index + offset);
        let next_comment = source[index..].find('%').map(|offset| index + offset);
        if next_comment.is_some_and(|comment| next_command.is_none_or(|command| comment < command))
        {
            index = skip_comment(source, next_comment.expect("comment checked above"));
            continue;
        }
        let Some(command_start) = next_command else {
            break;
        };
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            index = command_start + 1;
            continue;
        };
        index = command_end;
        let candidate = match command_name.as_str() {
            "usepackage" | "RequirePackage" => {
                let (cursor, options) = read_bracket_argument(source, command_end)
                    .map(|(end, value)| (end, value))
                    .unwrap_or_else(|| (command_end, String::new()));
                read_braced_argument(source, cursor).and_then(|(end, packages)| {
                    index = end;
                    packages
                        .split(',')
                        .map(str::trim)
                        .any(|package| matches!(package, "natbib" | "biblatex" | "cite"))
                        .then(|| citation_mode_from_options(&options))
                })
            }
            "PassOptionsToPackage" => {
                read_braced_argument(source, command_end).and_then(|(options_end, options)| {
                    read_braced_argument(source, options_end).and_then(
                        |(packages_end, packages)| {
                            index = packages_end;
                            packages
                                .split(',')
                                .map(str::trim)
                                .any(|package| matches!(package, "natbib" | "biblatex" | "cite"))
                                .then(|| citation_mode_from_options(&options))
                        },
                    )
                })
            }
            "setcitestyle" | "ExecuteBibliographyOptions" => {
                read_braced_argument(source, command_end).map(|(end, options)| {
                    index = end;
                    citation_mode_from_options(&options)
                })
            }
            "bibpunct" => {
                let mut cursor = read_bracket_argument(source, command_end)
                    .map(|(end, _)| end)
                    .unwrap_or(command_end);
                let mut arguments = Vec::new();
                for _ in 0..4 {
                    let Some((end, argument)) = read_braced_argument(source, cursor) else {
                        break;
                    };
                    cursor = end;
                    arguments.push(argument);
                }
                index = cursor;
                arguments.get(3).map(|argument| match argument.trim() {
                    "n" | "s" => CitationMode::Numeric,
                    "a" => CitationMode::AuthorYear,
                    _ => CitationMode::Auto,
                })
            }
            _ => None,
        };
        match candidate.unwrap_or(CitationMode::Auto) {
            CitationMode::Numeric => return CitationMode::Numeric,
            CitationMode::AuthorYear => mode = CitationMode::AuthorYear,
            CitationMode::Auto => {}
        }
    }
    mode
}

fn citation_mode_from_options(options: &str) -> CitationMode {
    let mut mode = CitationMode::Auto;
    for option in options
        .split(',')
        .map(|option| option.trim().to_ascii_lowercase())
    {
        let value = option
            .split_once('=')
            .map(|(_, value)| value.trim())
            .unwrap_or(option.as_str());
        if matches!(value, "numbers" | "numeric" | "numeric-comp" | "super")
            || value.starts_with("numeric-")
        {
            return CitationMode::Numeric;
        }
        if matches!(value, "authoryear" | "author-year" | "alphabetic")
            || value.starts_with("authoryear-")
        {
            mode = CitationMode::AuthorYear;
        }
    }
    mode
}

fn resolve_existing_project_path(root: &Utf8Path, path: &Utf8Path) -> Option<Utf8PathBuf> {
    if root.join(path).exists() {
        return Some(path.to_path_buf());
    }
    let mut resolved = Utf8PathBuf::new();
    let mut directory = root.to_path_buf();
    for component in path.components() {
        let component = component.as_str();
        let mut matched = None::<String>;
        for entry in fs::read_dir(directory.as_std_path()).ok()? {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == component {
                matched = Some(name);
                break;
            }
            if matched.is_none() && name.eq_ignore_ascii_case(component) {
                matched = Some(name);
            }
        }
        let matched = matched?;
        resolved.push(&matched);
        directory.push(&matched);
    }
    Some(resolved)
}

pub fn materialize_project(
    root: &Utf8Path,
    toplevel: &Utf8Path,
    aux: &SemanticAux,
) -> Result<MaterializedProject> {
    let scan = scan_project(root, toplevel)?;
    let bibliography_text_by_file = bibliography_text_by_file(&scan);
    let mut block_numbers = BTreeMap::<usize, String>::new();
    let mut block_counters = BTreeMap::<(String, Option<String>), u32>::new();
    for block in scan.blocks.iter().filter(|block| block.numbered) {
        let within_scope = block.within_level.and_then(|within_level| {
            aux.toc
                .iter()
                .rev()
                .find(|entry| {
                    entry.file == block.file
                        && entry.offset_utf8 <= block.offset_utf8
                        && entry.level == within_level
                        && !entry.number.is_empty()
                })
                .map(|entry| entry.number.clone())
        });
        let counter = block_counters
            .entry((block.counter_key.clone(), within_scope.clone()))
            .or_insert(0);
        *counter += 1;
        let number = if let Some(scope) = within_scope {
            format!("{scope}.{}", *counter)
        } else {
            counter.to_string()
        };
        block_numbers.insert(block.order, number);
    }
    let mut files = BTreeMap::new();
    let mut rewrite_spans = BTreeMap::new();
    for (path, source) in &scan.files {
        if let Some(bibliography_text) = bibliography_text_by_file.get(path) {
            files.insert(path.clone(), bibliography_text.clone());
            rewrite_spans.insert(path.clone(), Vec::new());
            continue;
        }
        let (rewritten, file_rewrite_spans) =
            rewrite_source(path, source, aux, &scan, &block_numbers);
        files.insert(path.clone(), rewritten);
        rewrite_spans.insert(path.clone(), file_rewrite_spans);
    }
    let mut tracked_inputs = scan.files.keys().cloned().collect::<Vec<_>>();
    tracked_inputs.sort();
    tracked_inputs.dedup();
    Ok(MaterializedProject {
        scan,
        files,
        rewrite_spans,
        tracked_inputs,
    })
}

pub fn derive_semantic_aux(scan: &ProjectScan, pages: &[PageSourceSlice]) -> SemanticAux {
    let mut toc = Vec::new();
    let mut labels = Vec::new();
    let mut citation_aliases = BTreeMap::new();
    let mut bibliography = Vec::new();
    let mut bibliography_titles = Vec::new();
    let mut bibliography_authors = Vec::new();
    let mut bibliography_years = Vec::new();
    let mut bibliography_fields = Vec::new();
    let mut bibliography_urls = Vec::new();
    let mut bibliography_dois = Vec::new();
    let mut bibliography_eprints = Vec::new();
    let mut float_captions = Vec::new();
    for path in &scan.bibliography_files {
        let Some(source) = scan.files.get(path) else {
            continue;
        };
        for entry in parse_bibliography_entries(path, source) {
            if let Some(title) = entry.title.as_ref() {
                bibliography_titles.push(BibliographyTitle {
                    key: entry.key.clone(),
                    title: title.clone(),
                    file: entry.file.clone(),
                });
            }
            if let Some(author) = entry.author.as_ref() {
                bibliography_authors.push(BibliographyAuthor {
                    key: entry.key.clone(),
                    author: author.clone(),
                    file: entry.file.clone(),
                });
            }
            if let Some(year) = entry.year.as_ref() {
                bibliography_years.push(BibliographyYear {
                    key: entry.key.clone(),
                    year: year.clone(),
                    file: entry.file.clone(),
                });
            }
            for (field, value) in &entry.fields {
                bibliography_fields.push(BibliographyField {
                    key: entry.key.clone(),
                    field: field.clone(),
                    value: value.clone(),
                    file: entry.file.clone(),
                });
            }
            if let Some(url) = entry.url.as_ref() {
                bibliography_urls.push(BibliographyUrl {
                    key: entry.key.clone(),
                    url: url.clone(),
                    file: entry.file.clone(),
                });
            }
            if let Some(doi) = entry.doi.as_ref() {
                bibliography_dois.push(BibliographyDoi {
                    key: entry.key.clone(),
                    doi: doi.clone(),
                    file: entry.file.clone(),
                });
            }
            if let Some(eprint) = entry.eprint.as_ref() {
                bibliography_eprints.push(BibliographyEprint {
                    key: entry.key.clone(),
                    eprint: eprint.clone(),
                    file: entry.file.clone(),
                });
            }
            bibliography.push(BibliographyEntry {
                key: entry.key,
                text: entry.text,
                label: entry.label,
                file: entry.file,
            });
        }
    }
    let mut section_counters = vec![0u32; 6];
    let mut equation_counter = 0u32;
    let mut figure_counter = 0u32;
    let mut table_counter = 0u32;
    let mut algorithm_counter = 0u32;
    let mut block_counters = BTreeMap::<(String, Option<String>), u32>::new();
    let mut numbered_sections = Vec::new();
    let mut numbered_equations = Vec::new();
    let mut numbered_floats = Vec::new();
    let mut numbered_blocks = Vec::new();
    let mut in_appendix = false;
    let mut combined = scan
        .sections
        .iter()
        .map(|section| {
            (
                section.order,
                1u8,
                section.file.clone(),
                section.offset_utf8,
            )
        })
        .collect::<Vec<_>>();
    combined.extend(scan.appendices.iter().map(|appendix| {
        (
            appendix.order,
            0u8,
            appendix.file.clone(),
            appendix.offset_utf8,
        )
    }));
    combined.extend(
        scan.labels
            .iter()
            .map(|label| (label.order, 2u8, label.file.clone(), label.offset_utf8)),
    );
    combined.extend(scan.equations.iter().map(|equation| {
        (
            equation.order,
            3u8,
            equation.file.clone(),
            equation.offset_utf8,
        )
    }));
    combined.extend(
        scan.floats
            .iter()
            .map(|float| (float.order, 4u8, float.file.clone(), float.offset_utf8)),
    );
    combined.extend(
        scan.blocks
            .iter()
            .map(|block| (block.order, 5u8, block.file.clone(), block.offset_utf8)),
    );
    combined.sort_by_key(|entry| entry.0);
    let mut label_index = 0u32;
    let mut current_number = None::<String>;
    for (order, kind, file, offset_utf8) in combined {
        if kind == 0 {
            in_appendix = true;
            section_counters.fill(0);
            current_number = None;
            continue;
        }
        if kind == 1 {
            let section = scan
                .sections
                .iter()
                .find(|section| section.order == order)
                .unwrap();
            let number = if section.numbered {
                let level_index = section.level as usize;
                if level_index < section_counters.len() {
                    section_counters[level_index] += 1;
                    for counter in section_counters.iter_mut().skip(level_index + 1) {
                        *counter = 0;
                    }
                }
                if in_appendix {
                    let mut appendix_number = String::new();
                    let mut appendix_index = if section_counters[0] > 0 {
                        section_counters[0]
                    } else {
                        section_counters[1]
                    };
                    while appendix_index > 0 {
                        appendix_index -= 1;
                        appendix_number.insert(0, (b'A' + (appendix_index % 26) as u8) as char);
                        appendix_index /= 26;
                    }
                    if level_index == 0 || (level_index == 1 && section_counters[0] == 0) {
                        appendix_number
                    } else if section_counters[0] > 0 {
                        let mut suffix = section_counters
                            [1..=level_index.min(section_counters.len() - 1)]
                            .iter()
                            .copied()
                            .filter(|value| *value > 0)
                            .map(|value| value.to_string())
                            .collect::<Vec<_>>();
                        suffix.insert(0, appendix_number);
                        suffix.join(".")
                    } else {
                        let mut suffix = section_counters
                            [2..=level_index.min(section_counters.len() - 1)]
                            .iter()
                            .copied()
                            .filter(|value| *value > 0)
                            .map(|value| value.to_string())
                            .collect::<Vec<_>>();
                        suffix.insert(0, appendix_number);
                        suffix.join(".")
                    }
                } else {
                    let start_index = if section_counters[0] > 0 || level_index == 0 {
                        0
                    } else {
                        1
                    };
                    section_counters[start_index..=level_index.min(section_counters.len() - 1)]
                        .iter()
                        .copied()
                        .filter(|value| *value > 0)
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join(".")
                }
            } else {
                String::new()
            };
            let page = page_for_offset(pages, &file, offset_utf8).unwrap_or(1);
            toc.push(TocEntry {
                level: section.level,
                number: number.clone(),
                title: section.toc_title.clone(),
                page,
                file: section.file.clone(),
                offset_utf8: section.offset_utf8,
            });
            if section.numbered {
                current_number = Some(number.clone());
                numbered_sections.push((file, offset_utf8, section.level, number));
            }
            continue;
        }
        if kind == 3 {
            let equation = scan
                .equations
                .iter()
                .find(|equation| equation.order == order)
                .unwrap();
            if equation.numbered {
                equation_counter += 1;
                numbered_equations.push((file, offset_utf8, equation_counter.to_string()));
            }
            continue;
        }
        if kind == 4 {
            let float = scan
                .floats
                .iter()
                .find(|float| float.order == order)
                .unwrap();
            if float.numbered {
                let number = if float.kind == FloatKind::Figure {
                    figure_counter += 1;
                    figure_counter.to_string()
                } else if float.kind == FloatKind::Table {
                    table_counter += 1;
                    table_counter.to_string()
                } else {
                    algorithm_counter += 1;
                    algorithm_counter.to_string()
                };
                numbered_floats.push((file, offset_utf8, float.kind, number));
            }
            continue;
        }
        if kind == 5 {
            let block = scan
                .blocks
                .iter()
                .find(|block| block.order == order)
                .unwrap();
            if block.numbered {
                let within_scope = block.within_level.and_then(|within_level| {
                    numbered_sections
                        .iter()
                        .rev()
                        .find(|(section_file, section_offset, section_level, _)| {
                            *section_file == file
                                && *section_offset <= offset_utf8
                                && *section_level == within_level
                        })
                        .map(|(_, _, _, number)| number.clone())
                });
                let counter = block_counters
                    .entry((block.counter_key.clone(), within_scope.clone()))
                    .or_insert(0);
                *counter += 1;
                let number = if let Some(scope) = within_scope {
                    format!("{scope}.{counter}")
                } else {
                    counter.to_string()
                };
                numbered_blocks.push((file, offset_utf8, block.kind.clone(), number));
            }
            continue;
        }
        let label = scan
            .labels
            .iter()
            .find(|label| label.order == order)
            .unwrap();
        label_index += 1;
        let latest_section = numbered_sections
            .iter()
            .rev()
            .find(|(section_file, section_offset, _, _)| {
                *section_file == file && *section_offset <= offset_utf8
            })
            .map(|(_, section_offset, _, number)| (*section_offset, number.clone()));
        let latest_equation = numbered_equations
            .iter()
            .rev()
            .find(|(equation_file, equation_offset, _)| {
                *equation_file == file && *equation_offset <= offset_utf8
            })
            .map(|(_, equation_offset, number)| (*equation_offset, number.clone()));
        let latest_float = numbered_floats
            .iter()
            .rev()
            .find(|(float_file, float_offset, _, _)| {
                *float_file == file && *float_offset <= offset_utf8
            })
            .map(|(_, float_offset, _, number)| (*float_offset, number.clone()));
        let latest_block = numbered_blocks
            .iter()
            .rev()
            .find(|(block_file, block_offset, _, _)| {
                *block_file == file && *block_offset <= offset_utf8
            })
            .map(|(_, block_offset, _, number)| (*block_offset, number.clone()));
        let number = if let Some((_, number)) =
            [latest_section, latest_equation, latest_float, latest_block]
                .into_iter()
                .flatten()
                .max_by_key(|(candidate_offset, _)| *candidate_offset)
        {
            number
        } else {
            current_number
                .clone()
                .unwrap_or_else(|| label_index.to_string())
        };
        labels.push(SemanticLabel {
            key: label.key.clone(),
            number,
            page: page_for_offset(pages, &label.file, label.offset_utf8).unwrap_or(1),
            file: label.file.clone(),
            offset_utf8: label.offset_utf8,
        });
    }
    labels.sort_by(|left, right| left.key.cmp(&right.key));
    let include_all_bibliography = scan
        .citations
        .iter()
        .any(|citation| citation.keys.iter().any(|key| key == "*"));
    let mut citation_keys = if include_all_bibliography {
        bibliography
            .iter()
            .map(|entry| entry.key.clone())
            .collect::<Vec<_>>()
    } else {
        scan.citations
            .iter()
            .flat_map(|citation| citation.keys.iter().cloned())
            .collect::<Vec<_>>()
    };
    citation_keys.sort();
    citation_keys.dedup();
    for alias in &scan.citation_aliases {
        citation_aliases.insert(
            alias.key.clone(),
            CitationAlias {
                key: alias.key.clone(),
                text: alias.text.clone(),
                file: alias.file.clone(),
                offset_utf8: alias.offset_utf8,
            },
        );
    }
    for caption in &scan.captions {
        let number = numbered_floats
            .iter()
            .rev()
            .find(|(float_file, float_offset, float_kind, _)| {
                *float_file == caption.file
                    && *float_kind == caption.kind
                    && *float_offset <= caption.offset_utf8
            })
            .map(|(_, _, _, number)| number.clone())
            .filter(|_| caption.numbered)
            .unwrap_or_default();
        float_captions.push(FloatCaption {
            kind: match caption.kind {
                FloatKind::Figure => "figure".to_string(),
                FloatKind::Table => "table".to_string(),
                FloatKind::Algorithm => "algorithm".to_string(),
            },
            number,
            title: caption.list_title.clone(),
            body_title: caption.body_title.clone(),
            page: page_for_offset(pages, &caption.file, caption.offset_utf8).unwrap_or(1),
            file: caption.file.clone(),
            offset_utf8: caption.offset_utf8,
        });
    }
    SemanticAux {
        labels,
        toc,
        citation_keys,
        bibliography_inputs: scan.bibliography_files.clone(),
        bibliography_style: scan.bibliography_style.clone(),
        citation_mode: infer_project_citation_mode(&scan.files, scan.bibliography_style.as_deref()),
        citation_aliases: citation_aliases.into_values().collect(),
        bibliography,
        bibliography_titles,
        bibliography_authors,
        bibliography_years,
        bibliography_fields,
        bibliography_urls,
        bibliography_dois,
        bibliography_eprints,
        float_captions,
    }
}

pub fn load_semantic_aux(path: &Utf8Path) -> Result<SemanticAux> {
    let payload = fs::read(path).with_context(|| format!("failed to read {path}"))?;
    if let Ok(aux) = serde_json::from_slice(&payload) {
        return Ok(aux);
    }
    parse_concrete_semantic_aux(&payload).with_context(|| format!("failed to parse {path}"))
}

pub fn parse_concrete_semantic_aux(payload: &[u8]) -> Result<SemanticAux> {
    let source = std::str::from_utf8(payload).context("concrete semantic aux is not utf-8")?;
    let mut aux = SemanticAux::default();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('%') {
            continue;
        }
        let Some((command_end, command_name)) = read_command_name(line, 0) else {
            bail!("invalid concrete semantic aux line: {line}");
        };
        let cursor = command_end;
        match command_name.as_str() {
            "newlabel" => {
                let (outer_end, key) =
                    read_braced_argument(line, cursor).context("missing newlabel key")?;
                let (_, value) = read_braced_argument(line, outer_end)
                    .context("missing newlabel tuple payload")?;
                let fields = parse_nested_braced_fields(&value, 4)?;
                aux.labels.push(SemanticLabel {
                    key: decode_aux_text(&key)?,
                    number: decode_aux_text(&fields[0])?,
                    page: fields[1].parse().context("invalid newlabel page")?,
                    file: Utf8PathBuf::from(decode_aux_text(&fields[2])?),
                    offset_utf8: fields[3].parse().context("invalid newlabel offset")?,
                });
            }
            "@writefile" => {
                let target =
                    read_braced_argument(line, cursor).context("missing writefile target")?;
                let payload =
                    read_braced_argument(line, target.0).context("missing writefile payload")?;
                let (kind, number, title, page, file, offset) =
                    parse_writefile_contentsline(&payload.1)?;
                match target.1.as_str() {
                    "toc" => {
                        let level = contentsline_level(&kind)
                            .with_context(|| format!("unsupported toc contentsline kind {kind}"))?;
                        aux.toc.push(TocEntry {
                            level,
                            number,
                            title,
                            page,
                            file,
                            offset_utf8: offset,
                        });
                    }
                    "lof" | "lot" | "loa" => {
                        let Some(float_kind) = float_kind_from_writefile_target(&target.1) else {
                            bail!("unsupported float writefile target {}", target.1);
                        };
                        if let Some(existing) = aux.float_captions.iter_mut().find(|caption| {
                            caption.kind == float_kind
                                && caption.number == number
                                && caption.file == file
                                && caption.offset_utf8 == offset
                        }) {
                            existing.title = title.clone();
                            existing.page = page;
                            if existing.body_title.is_empty() {
                                existing.body_title = title.clone();
                            }
                        } else {
                            aux.float_captions.push(FloatCaption {
                                kind: float_kind.to_string(),
                                number,
                                title: title.clone(),
                                body_title: title,
                                page,
                                file,
                                offset_utf8: offset,
                            });
                        }
                    }
                    other => bail!("unsupported writefile target {other}"),
                }
            }
            "latexdtoc" => {
                let level = read_braced_argument(line, cursor).context("missing toc level")?;
                let number = read_braced_argument(line, level.0).context("missing toc number")?;
                let title = read_braced_argument(line, number.0).context("missing toc title")?;
                let page = read_braced_argument(line, title.0).context("missing toc page")?;
                let file = read_braced_argument(line, page.0).context("missing toc file")?;
                let offset = read_braced_argument(line, file.0).context("missing toc offset")?;
                aux.toc.push(TocEntry {
                    level: level.1.parse().context("invalid toc level")?,
                    number: decode_aux_text(&number.1)?,
                    title: decode_aux_text(&title.1)?,
                    page: page.1.parse().context("invalid toc page")?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                    offset_utf8: offset.1.parse().context("invalid toc offset")?,
                });
            }
            "citation" => {
                let (_, raw_keys) =
                    read_braced_argument(line, cursor).context("missing citation keys")?;
                for key in raw_keys.split(',').filter(|key| !key.is_empty()) {
                    let decoded = decode_aux_text(key)?;
                    if !aux.citation_keys.contains(&decoded) {
                        aux.citation_keys.push(decoded);
                    }
                }
            }
            "bibdata" => {
                let (_, raw_paths) =
                    read_braced_argument(line, cursor).context("missing bibdata inputs")?;
                for path in raw_paths.split(',').filter(|path| !path.is_empty()) {
                    let decoded = Utf8PathBuf::from(decode_aux_text(path)?);
                    if !aux.bibliography_inputs.contains(&decoded) {
                        aux.bibliography_inputs.push(decoded);
                    }
                }
            }
            "bibstyle" => {
                let (_, style) =
                    read_braced_argument(line, cursor).context("missing bibstyle value")?;
                aux.bibliography_style = Some(decode_aux_text(&style)?);
            }
            "latexdcitationmode" => {
                let (_, mode) =
                    read_braced_argument(line, cursor).context("missing citation mode")?;
                aux.citation_mode = CitationMode::parse(&decode_aux_text(&mode)?);
            }
            "bibcite" => {
                let key = read_braced_argument(line, cursor).context("missing bibcite key")?;
                let label = read_braced_argument(line, key.0).context("missing bibcite label")?;
                let decoded_key = decode_aux_text(&key.1)?;
                let decoded_label = decode_aux_text(&label.1)?;
                if let Some(existing) = aux
                    .bibliography
                    .iter_mut()
                    .find(|entry| entry.key == decoded_key)
                {
                    existing.label = Some(decoded_label);
                } else {
                    aux.bibliography.push(BibliographyEntry {
                        key: decoded_key,
                        text: String::new(),
                        label: Some(decoded_label),
                        file: Utf8PathBuf::new(),
                    });
                }
            }
            "latexdcitealias" => {
                let key = read_braced_argument(line, cursor).context("missing citealias key")?;
                let text = read_braced_argument(line, key.0).context("missing citealias text")?;
                let file = read_braced_argument(line, text.0).context("missing citealias file")?;
                let offset =
                    read_braced_argument(line, file.0).context("missing citealias offset")?;
                aux.citation_aliases.push(CitationAlias {
                    key: decode_aux_text(&key.1)?,
                    text: decode_aux_text(&text.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                    offset_utf8: offset.1.parse().context("invalid citealias offset")?,
                });
            }
            "latexdbib" => {
                let key = read_braced_argument(line, cursor).context("missing bib key")?;
                let text = read_braced_argument(line, key.0).context("missing bib text")?;
                let label = read_braced_argument(line, text.0).context("missing bib label")?;
                let file = read_braced_argument(line, label.0).context("missing bib file")?;
                let decoded_key = decode_aux_text(&key.1)?;
                let decoded_text = decode_aux_text(&text.1)?;
                let decoded_label = decode_aux_text(&label.1)?;
                let decoded_file = Utf8PathBuf::from(decode_aux_text(&file.1)?);
                if let Some(existing) = aux
                    .bibliography
                    .iter_mut()
                    .find(|entry| entry.key == decoded_key)
                {
                    existing.text = decoded_text;
                    existing.file = decoded_file;
                    if !decoded_label.is_empty() {
                        existing.label = Some(decoded_label);
                    }
                } else {
                    aux.bibliography.push(BibliographyEntry {
                        key: decoded_key,
                        text: decoded_text,
                        label: (!decoded_label.is_empty()).then_some(decoded_label),
                        file: decoded_file,
                    });
                }
            }
            "latexdbibtitle" => {
                let key = read_braced_argument(line, cursor).context("missing bibtitle key")?;
                let title = read_braced_argument(line, key.0).context("missing bibtitle value")?;
                let file = read_braced_argument(line, title.0).context("missing bibtitle file")?;
                aux.bibliography_titles.push(BibliographyTitle {
                    key: decode_aux_text(&key.1)?,
                    title: decode_aux_text(&title.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdbibauthor" => {
                let key = read_braced_argument(line, cursor).context("missing bibauthor key")?;
                let author =
                    read_braced_argument(line, key.0).context("missing bibauthor value")?;
                let file =
                    read_braced_argument(line, author.0).context("missing bibauthor file")?;
                aux.bibliography_authors.push(BibliographyAuthor {
                    key: decode_aux_text(&key.1)?,
                    author: decode_aux_text(&author.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdbibyear" => {
                let key = read_braced_argument(line, cursor).context("missing bibyear key")?;
                let year = read_braced_argument(line, key.0).context("missing bibyear value")?;
                let file = read_braced_argument(line, year.0).context("missing bibyear file")?;
                aux.bibliography_years.push(BibliographyYear {
                    key: decode_aux_text(&key.1)?,
                    year: decode_aux_text(&year.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdbibfield" => {
                let key = read_braced_argument(line, cursor).context("missing bibfield key")?;
                let field = read_braced_argument(line, key.0).context("missing bibfield name")?;
                let value =
                    read_braced_argument(line, field.0).context("missing bibfield value")?;
                let file = read_braced_argument(line, value.0).context("missing bibfield file")?;
                aux.bibliography_fields.push(BibliographyField {
                    key: decode_aux_text(&key.1)?,
                    field: decode_aux_text(&field.1)?,
                    value: decode_aux_text(&value.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdbiburl" => {
                let key = read_braced_argument(line, cursor).context("missing biburl key")?;
                let url = read_braced_argument(line, key.0).context("missing biburl value")?;
                let file = read_braced_argument(line, url.0).context("missing biburl file")?;
                aux.bibliography_urls.push(BibliographyUrl {
                    key: decode_aux_text(&key.1)?,
                    url: decode_aux_text(&url.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdbibdoi" => {
                let key = read_braced_argument(line, cursor).context("missing bibdoi key")?;
                let doi = read_braced_argument(line, key.0).context("missing bibdoi value")?;
                let file = read_braced_argument(line, doi.0).context("missing bibdoi file")?;
                aux.bibliography_dois.push(BibliographyDoi {
                    key: decode_aux_text(&key.1)?,
                    doi: decode_aux_text(&doi.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdbibeprint" => {
                let key = read_braced_argument(line, cursor).context("missing bibeprint key")?;
                let eprint =
                    read_braced_argument(line, key.0).context("missing bibeprint value")?;
                let file =
                    read_braced_argument(line, eprint.0).context("missing bibeprint file")?;
                aux.bibliography_eprints.push(BibliographyEprint {
                    key: decode_aux_text(&key.1)?,
                    eprint: decode_aux_text(&eprint.1)?,
                    file: Utf8PathBuf::from(decode_aux_text(&file.1)?),
                });
            }
            "latexdfloatcaption" => {
                let kind = read_braced_argument(line, cursor).context("missing float kind")?;
                let number = read_braced_argument(line, kind.0).context("missing float number")?;
                let title = read_braced_argument(line, number.0).context("missing float title")?;
                let body_title =
                    read_braced_argument(line, title.0).context("missing float body title")?;
                let page =
                    read_braced_argument(line, body_title.0).context("missing float page")?;
                let file = read_braced_argument(line, page.0).context("missing float file")?;
                let offset = read_braced_argument(line, file.0).context("missing float offset")?;
                let decoded_kind = decode_aux_text(&kind.1)?;
                let decoded_number = decode_aux_text(&number.1)?;
                let decoded_title = decode_aux_text(&title.1)?;
                let decoded_body_title = decode_aux_text(&body_title.1)?;
                let decoded_page = page.1.parse().context("invalid float page")?;
                let decoded_file = Utf8PathBuf::from(decode_aux_text(&file.1)?);
                let decoded_offset = offset.1.parse().context("invalid float offset")?;
                if let Some(existing) = aux.float_captions.iter_mut().find(|caption| {
                    caption.kind == decoded_kind
                        && caption.number == decoded_number
                        && caption.file == decoded_file
                        && caption.offset_utf8 == decoded_offset
                }) {
                    existing.title = decoded_title;
                    existing.body_title = decoded_body_title;
                    existing.page = decoded_page;
                } else {
                    aux.float_captions.push(FloatCaption {
                        kind: decoded_kind,
                        number: decoded_number,
                        title: decoded_title,
                        body_title: decoded_body_title,
                        page: decoded_page,
                        file: decoded_file,
                        offset_utf8: decoded_offset,
                    });
                }
            }
            _ => bail!("unsupported concrete semantic aux command \\{command_name}"),
        }
    }
    Ok(aux)
}

pub fn derive_semantic_aux_index(scan: &ProjectScan, aux: &SemanticAux) -> SemanticAuxIndex {
    let mut files = BTreeMap::<Utf8PathBuf, SemanticAuxFileSummary>::new();
    for label in &aux.labels {
        files
            .entry(label.file.clone())
            .or_insert_with(|| SemanticAuxFileSummary {
                path: label.file.clone(),
                ..SemanticAuxFileSummary::default()
            })
            .label_keys
            .push(label.key.clone());
    }
    for entry in &aux.toc {
        files
            .entry(entry.file.clone())
            .or_insert_with(|| SemanticAuxFileSummary {
                path: entry.file.clone(),
                ..SemanticAuxFileSummary::default()
            })
            .toc
            .push(SemanticAuxTocSummary {
                level: entry.level,
                number: entry.number.clone(),
                title: entry.title.clone(),
            });
    }
    for citation in &scan.citations {
        let summary =
            files
                .entry(citation.file.clone())
                .or_insert_with(|| SemanticAuxFileSummary {
                    path: citation.file.clone(),
                    ..SemanticAuxFileSummary::default()
                });
        for key in &citation.keys {
            if !summary.citation_keys.contains(key) {
                summary.citation_keys.push(key.clone());
            }
        }
    }
    for entry in &aux.bibliography {
        files
            .entry(entry.file.clone())
            .or_insert_with(|| SemanticAuxFileSummary {
                path: entry.file.clone(),
                ..SemanticAuxFileSummary::default()
            })
            .bibliography_keys
            .push(entry.key.clone());
        files
            .entry(entry.file.clone())
            .or_insert_with(|| SemanticAuxFileSummary {
                path: entry.file.clone(),
                ..SemanticAuxFileSummary::default()
            })
            .bibliography_entries
            .push(SemanticAuxBibliographySummary {
                key: entry.key.clone(),
                url: aux.citation_url(&entry.key).map(ToOwned::to_owned),
                doi: aux.citation_doi(&entry.key).map(ToOwned::to_owned),
                eprint: aux.citation_eprint(&entry.key).map(ToOwned::to_owned),
                fields: aux
                    .bibliography_fields
                    .iter()
                    .filter(|field| field.key == entry.key)
                    .map(|field| SemanticAuxNamedValue {
                        name: field.field.clone(),
                        value: field.value.clone(),
                    })
                    .collect(),
            });
    }
    for caption in &aux.float_captions {
        files
            .entry(caption.file.clone())
            .or_insert_with(|| SemanticAuxFileSummary {
                path: caption.file.clone(),
                ..SemanticAuxFileSummary::default()
            })
            .float_captions
            .push(SemanticAuxFloatSummary {
                kind: caption.kind.clone(),
                number: caption.number.clone(),
                title: caption.title.clone(),
            });
    }
    SemanticAuxIndex {
        has_table_of_contents: scan.has_table_of_contents,
        has_bibliography_heading: scan.has_bibliography_heading,
        bibliography_inputs: aux.bibliography_inputs.clone(),
        bibliography_style: aux.bibliography_style.clone(),
        label_count: aux.labels.len(),
        toc_count: aux.toc.len(),
        citation_key_count: aux.citation_keys.len(),
        bibliography_entry_count: aux.bibliography.len(),
        files: files.into_values().collect(),
    }
}

pub fn serialize_semantic_aux_backdated_with_previous(
    previous_payload: Option<&[u8]>,
    current: &SemanticAux,
) -> Result<Vec<u8>> {
    if let Some(previous_payload) = previous_payload {
        if let Ok(previous) = serde_json::from_slice::<SemanticAux>(previous_payload) {
            if previous.equivalent_to(current) {
                return Ok(previous_payload.to_vec());
            }
        }
    }
    serde_json::to_vec_pretty(current).context("failed to serialize semantic aux")
}

pub fn serialize_semantic_aux_backdated(path: &Utf8Path, current: &SemanticAux) -> Result<Vec<u8>> {
    let previous_payload = if path.exists() {
        Some(fs::read(path).with_context(|| format!("failed to read {path}"))?)
    } else {
        None
    };
    serialize_semantic_aux_backdated_with_previous(previous_payload.as_deref(), current)
}

pub fn serialize_concrete_semantic_aux_backdated_with_previous(
    previous_payload: Option<&[u8]>,
    current: &SemanticAux,
) -> Result<Vec<u8>> {
    if let Some(previous_payload) = previous_payload {
        if let Ok(previous) = parse_concrete_semantic_aux(previous_payload) {
            if previous.equivalent_to(current) {
                return Ok(previous_payload.to_vec());
            }
        }
    }
    render_concrete_semantic_aux(current)
}

pub fn serialize_concrete_semantic_aux_backdated(
    path: &Utf8Path,
    current: &SemanticAux,
) -> Result<Vec<u8>> {
    let previous_payload = if path.exists() {
        Some(fs::read(path).with_context(|| format!("failed to read {path}"))?)
    } else {
        None
    };
    serialize_concrete_semantic_aux_backdated_with_previous(previous_payload.as_deref(), current)
}

pub fn render_concrete_semantic_aux(aux: &SemanticAux) -> Result<Vec<u8>> {
    let mut lines = vec!["% latexd semantic aux v1".to_string()];
    for label in &aux.labels {
        lines.push(format!(
            "\\newlabel{{{}}}{{{{{}}}{{{}}}{{{}}}{{{}}}}}",
            encode_aux_text(&label.key),
            encode_aux_text(&label.number),
            label.page,
            encode_aux_text(label.file.as_str()),
            label.offset_utf8
        ));
    }
    for entry in &aux.toc {
        let kind = contentsline_kind(entry.level)
            .with_context(|| format!("unsupported toc level {}", entry.level))?;
        lines.push(format!(
            "\\@writefile{{toc}}{{{}}}",
            render_writefile_contentsline(
                kind,
                &entry.number,
                &entry.title,
                entry.page,
                &entry.file,
                entry.offset_utf8
            )
        ));
    }
    if !aux.citation_keys.is_empty() {
        lines.push(format!(
            "\\citation{{{}}}",
            aux.citation_keys
                .iter()
                .map(|key| encode_aux_text(key))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if !aux.bibliography_inputs.is_empty() {
        lines.push(format!(
            "\\bibdata{{{}}}",
            aux.bibliography_inputs
                .iter()
                .map(|path| encode_aux_text(path.as_str()))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(style) = aux.bibliography_style.as_deref() {
        lines.push(format!("\\bibstyle{{{}}}", encode_aux_text(style)));
    }
    if aux.citation_mode != CitationMode::Auto {
        lines.push(format!(
            "\\latexdcitationmode{{{}}}",
            encode_aux_text(aux.citation_mode.as_str())
        ));
    }
    for alias in &aux.citation_aliases {
        lines.push(format!(
            "\\latexdcitealias{{{}}}{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&alias.key),
            encode_aux_text(&alias.text),
            encode_aux_text(alias.file.as_str()),
            alias.offset_utf8
        ));
    }
    for entry in &aux.bibliography {
        if let Some(label) = entry.label.as_deref() {
            lines.push(format!(
                "\\bibcite{{{}}}{{{}}}",
                encode_aux_text(&entry.key),
                encode_aux_text(label)
            ));
        }
        lines.push(format!(
            "\\latexdbib{{{}}}{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.text),
            encode_aux_text(entry.label.as_deref().unwrap_or("")),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_titles {
        lines.push(format!(
            "\\latexdbibtitle{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.title),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_authors {
        lines.push(format!(
            "\\latexdbibauthor{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.author),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_years {
        lines.push(format!(
            "\\latexdbibyear{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.year),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_fields {
        lines.push(format!(
            "\\latexdbibfield{{{}}}{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.field),
            encode_aux_text(&entry.value),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_urls {
        lines.push(format!(
            "\\latexdbiburl{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.url),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_dois {
        lines.push(format!(
            "\\latexdbibdoi{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.doi),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for entry in &aux.bibliography_eprints {
        lines.push(format!(
            "\\latexdbibeprint{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&entry.key),
            encode_aux_text(&entry.eprint),
            encode_aux_text(entry.file.as_str())
        ));
    }
    for caption in &aux.float_captions {
        if let Some(target) = writefile_target_for_float_kind(&caption.kind) {
            lines.push(format!(
                "\\@writefile{{{}}}{{{}}}",
                target,
                render_writefile_contentsline(
                    &caption.kind,
                    &caption.number,
                    &caption.title,
                    caption.page,
                    &caption.file,
                    caption.offset_utf8
                )
            ));
        }
        lines.push(format!(
            "\\latexdfloatcaption{{{}}}{{{}}}{{{}}}{{{}}}{{{}}}{{{}}}{{{}}}",
            encode_aux_text(&caption.kind),
            encode_aux_text(&caption.number),
            encode_aux_text(&caption.title),
            encode_aux_text(&caption.body_title),
            caption.page,
            encode_aux_text(caption.file.as_str()),
            caption.offset_utf8
        ));
    }
    Ok(lines.join("\n").into_bytes())
}

fn rewrite_source(
    path: &Utf8Path,
    source: &str,
    aux: &SemanticAux,
    scan: &ProjectScan,
    block_numbers: &BTreeMap<usize, String>,
) -> (String, Vec<MaterializedRewriteSpan>) {
    let mut output = String::with_capacity(source.len());
    let mut rewrite_spans = Vec::new();
    let mut index = 0usize;
    while index < source.len() {
        let Some(backslash_rel) = source[index..].find('\\') else {
            output.push_str(&source[index..]);
            break;
        };
        let command_start = index + backslash_rel;
        output.push_str(&source[index..command_start]);
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            output.push('\\');
            index = command_start + 1;
            continue;
        };
        match command_name.as_str() {
            "chapter" | "section" | "subsection" | "subsubsection" | "paragraph"
            | "subparagraph" => {
                let mut cursor = skip_optional_command_star(source, command_end);
                cursor = skip_whitespace(source, cursor);
                if let Some((option_end, _)) = read_bracket_argument(source, cursor) {
                    cursor = option_end;
                }
                if let Some((argument_end, title)) = read_braced_argument(source, cursor) {
                    let rendered = title.trim().to_string();
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "begin" => {
                if let Some((argument_end, environment)) = read_braced_argument(source, command_end)
                {
                    let environment = environment.trim();
                    if let Some(block) = scan.blocks.iter().find(|block| {
                        block.file == path && block.offset_utf8 == command_start as u32
                    }) {
                        let mut cursor = skip_whitespace(source, argument_end);
                        let mut parsed_title = None;
                        if let Some((title_end, title)) = read_bracket_argument(source, cursor) {
                            parsed_title = Some(title.trim().to_string());
                            cursor = title_end;
                        }
                        let mut rendered = if block.numbered {
                            format!(
                                "{} {}",
                                block.kind.display_name(),
                                block_numbers
                                    .get(&block.order)
                                    .cloned()
                                    .unwrap_or_else(|| "1".to_string())
                            )
                        } else {
                            block.kind.display_name().to_string()
                        };
                        if let Some(title) = block
                            .title
                            .as_deref()
                            .or(parsed_title.as_deref())
                            .filter(|title| !title.is_empty())
                        {
                            rendered.push_str(" (");
                            rendered.push_str(title);
                            rendered.push(')');
                        };
                        rendered.push_str(". ");
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: cursor as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = cursor;
                        continue;
                    }
                    if environment == "proof" || environment == "proof*" {
                        let mut cursor = skip_whitespace(source, argument_end);
                        let rendered = if let Some((title_end, title)) =
                            read_bracket_argument(source, cursor)
                        {
                            cursor = title_end;
                            let title = title.trim();
                            if title.is_empty() {
                                "Proof. ".to_string()
                            } else {
                                format!("Proof ({title}). ")
                            }
                        } else {
                            "Proof. ".to_string()
                        };
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: cursor as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = cursor;
                        continue;
                    }
                    if is_display_math_environment(environment)
                        || environment == "figure"
                        || environment == "figure*"
                        || environment == "table"
                        || environment == "table*"
                        || environment == "algorithm"
                        || environment == "algorithm*"
                    {
                        let mut cursor = argument_end;
                        if matches!(
                            environment,
                            "figure" | "figure*" | "table" | "table*" | "algorithm" | "algorithm*"
                        ) {
                            cursor = skip_whitespace(source, cursor);
                            if let Some((option_end, _)) = read_bracket_argument(source, cursor) {
                                cursor = option_end;
                            }
                        }
                        let rendered = if is_display_math_environment(environment) {
                            "$"
                        } else {
                            ""
                        };
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: cursor as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered: rendered.to_string(),
                        });
                        index = cursor;
                        continue;
                    }
                }
            }
            "newtheorem" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((environment_end, _)) = read_braced_argument(source, cursor) {
                    let mut title_cursor = environment_end;
                    if let Some((shared_counter_end, _)) =
                        read_bracket_argument(source, title_cursor)
                    {
                        title_cursor = shared_counter_end;
                    }
                    if let Some((title_end, _)) = read_braced_argument(source, title_cursor) {
                        let mut argument_end = title_end;
                        if let Some((within_end, _)) = read_bracket_argument(source, title_end) {
                            argument_end = within_end;
                        }
                        let output_start_utf8 = output.len() as u32;
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output_start_utf8,
                            rendered: String::new(),
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "theoremstyle" => {
                if let Some((argument_end, _)) = read_braced_argument(source, command_end) {
                    let output_start_utf8 = output.len() as u32;
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output_start_utf8,
                        rendered: String::new(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "newtheoremstyle" => {
                let mut cursor = command_end;
                let mut consumed_all = true;
                for _ in 0..9 {
                    if let Some((argument_end, _)) = read_braced_argument(source, cursor) {
                        cursor = argument_end;
                    } else {
                        consumed_all = false;
                        break;
                    }
                }
                if consumed_all {
                    let output_start_utf8 = output.len() as u32;
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: cursor as u32,
                        output_start_utf8,
                        output_end_utf8: output_start_utf8,
                        rendered: String::new(),
                    });
                    index = cursor;
                    continue;
                }
            }
            "swapnumbers" => {
                let output_start_utf8 = output.len() as u32;
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: command_end as u32,
                    output_start_utf8,
                    output_end_utf8: output_start_utf8,
                    rendered: String::new(),
                });
                index = command_end;
                continue;
            }
            "end" => {
                if let Some((argument_end, environment)) = read_braced_argument(source, command_end)
                {
                    let environment = environment.trim();
                    if environment == "proof"
                        || environment == "proof*"
                        || is_display_math_environment(environment)
                        || environment == "figure"
                        || environment == "figure*"
                        || environment == "table"
                        || environment == "table*"
                        || environment == "algorithm"
                        || environment == "algorithm*"
                        || BlockKind::from_environment(environment).is_some()
                        || scan
                            .custom_block_environments
                            .iter()
                            .any(|candidate| candidate == environment.trim_end_matches('*'))
                    {
                        let rendered = if is_display_math_environment(environment) {
                            "$"
                        } else {
                            ""
                        };
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered: rendered.to_string(),
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "label" => {
                if let Some((argument_end, _)) = read_braced_argument(source, command_end) {
                    let output_start_utf8 = output.len() as u32;
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output_start_utf8,
                        rendered: String::new(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "ref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let rendered = aux.label_number(&key).unwrap_or("??").to_string();
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "subref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let rendered = aux.label_number(&key).unwrap_or("??").to_string();
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "autoref" | "cref" | "Cref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let references = keys
                        .split(',')
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            let reference_kind = aux
                                .labels
                                .iter()
                                .find(|label| label.key == key)
                                .map(|label| {
                                    let latest_section = scan
                                        .sections
                                        .iter()
                                        .filter(|section| {
                                            section.numbered
                                                && section.file == label.file
                                                && section.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|section| (section.level, section.offset_utf8))
                                        .max_by_key(|(_, offset)| *offset)
                                        .or_else(|| {
                                            aux.toc
                                                .iter()
                                                .filter(|entry| {
                                                    entry.file == label.file
                                                        && entry.offset_utf8 <= label.offset_utf8
                                                })
                                                .map(|entry| (entry.level, entry.offset_utf8))
                                                .max_by_key(|(_, offset)| *offset)
                                        });
                                    let latest_equation = scan
                                        .equations
                                        .iter()
                                        .filter(|equation| {
                                            equation.numbered
                                                && equation.file == label.file
                                                && equation.offset_utf8 <= label.offset_utf8
                                        })
                                        .max_by_key(|equation| equation.offset_utf8);
                                    let latest_float = scan
                                        .floats
                                        .iter()
                                        .filter(|float| {
                                            float.numbered
                                                && float.file == label.file
                                                && float.offset_utf8 <= label.offset_utf8
                                        })
                                        .max_by_key(|float| float.offset_utf8);
                                    let latest_block = scan
                                        .blocks
                                        .iter()
                                        .filter(|block| {
                                            block.numbered
                                                && block.file == label.file
                                                && block.offset_utf8 <= label.offset_utf8
                                        })
                                        .max_by_key(|block| block.offset_utf8);
                                    if latest_float.is_some_and(|float| {
                                        latest_section
                                            .map(|(_, section_offset)| {
                                                float.offset_utf8 > section_offset
                                            })
                                            .unwrap_or(true)
                                            && latest_equation
                                                .map(|equation| {
                                                    float.offset_utf8 > equation.offset_utf8
                                                })
                                                .unwrap_or(true)
                                            && latest_block
                                                .map(|block| float.offset_utf8 > block.offset_utf8)
                                                .unwrap_or(true)
                                    }) {
                                        if latest_float
                                            .is_some_and(|float| float.kind == FloatKind::Figure)
                                        {
                                            "Figure"
                                        } else if latest_float
                                            .is_some_and(|float| float.kind == FloatKind::Table)
                                        {
                                            "Table"
                                        } else {
                                            "Algorithm"
                                        }
                                    } else if latest_equation.is_some_and(|equation| {
                                        latest_section
                                            .map(|(_, section_offset)| {
                                                equation.offset_utf8 > section_offset
                                            })
                                            .unwrap_or(true)
                                            && latest_block
                                                .map(|block| {
                                                    equation.offset_utf8 > block.offset_utf8
                                                })
                                                .unwrap_or(true)
                                    }) {
                                        "Equation"
                                    } else if let Some(block) = latest_block {
                                        block.kind.display_name()
                                    } else if let Some((level, _)) = latest_section {
                                        if aux.label_number(key).is_some_and(|number| {
                                            number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                        }) {
                                            "Appendix"
                                        } else if level == 0 {
                                            "Chapter"
                                        } else if level == 2 {
                                            "Subsection"
                                        } else if level == 3 {
                                            "Subsubsection"
                                        } else if level == 4 {
                                            "Paragraph"
                                        } else if level >= 5 {
                                            "Subparagraph"
                                        } else {
                                            "Section"
                                        }
                                    } else if aux.label_number(key).is_some_and(|number| {
                                        number.chars().all(|ch| ch.is_ascii_digit())
                                    }) {
                                        "Equation"
                                    } else {
                                        "Section"
                                    }
                                })
                                .or_else(|| {
                                    aux.label_number(key).map(|number| {
                                        if number.chars().all(|ch| ch.is_ascii_digit()) {
                                            "Equation"
                                        } else if number
                                            .starts_with(|ch: char| ch.is_ascii_uppercase())
                                        {
                                            "Appendix"
                                        } else {
                                            "Section"
                                        }
                                    })
                                })
                                .unwrap_or("Section");
                            (
                                reference_kind,
                                aux.label_number(key)
                                    .map(ToOwned::to_owned)
                                    .unwrap_or_else(|| "??".to_string()),
                            )
                        })
                        .collect::<Vec<_>>();
                    if references.is_empty() {
                        index = argument_end;
                        continue;
                    }
                    let rendered = if references.len() == 1 {
                        format!("{} {}", references[0].0, references[0].1)
                    } else if references.iter().all(|(kind, _)| *kind == references[0].0) {
                        let plural_kind = match references[0].0 {
                            "Chapter" => "Chapters".to_string(),
                            "Appendix" => "Appendices".to_string(),
                            "Equation" => "Equations".to_string(),
                            "Figure" => "Figures".to_string(),
                            "Table" => "Tables".to_string(),
                            "Algorithm" => "Algorithms".to_string(),
                            "Theorem" => "Theorems".to_string(),
                            "Lemma" => "Lemmas".to_string(),
                            "Proposition" => "Propositions".to_string(),
                            "Corollary" => "Corollaries".to_string(),
                            "Definition" => "Definitions".to_string(),
                            "Remark" => "Remarks".to_string(),
                            "Claim" => "Claims".to_string(),
                            "Example" => "Examples".to_string(),
                            "Assumption" => "Assumptions".to_string(),
                            "Conjecture" => "Conjectures".to_string(),
                            "Axiom" => "Axioms".to_string(),
                            "Fact" => "Facts".to_string(),
                            "Observation" => "Observations".to_string(),
                            "Problem" => "Problems".to_string(),
                            "Exercise" => "Exercises".to_string(),
                            "Question" => "Questions".to_string(),
                            "Notation" => "Notations".to_string(),
                            "Subsection" => "Subsections".to_string(),
                            "Subsubsection" => "Subsubsections".to_string(),
                            "Paragraph" => "Paragraphs".to_string(),
                            "Subparagraph" => "Subparagraphs".to_string(),
                            other => pluralize_kind_name(other),
                        };
                        format!(
                            "{plural_kind} {}",
                            references
                                .iter()
                                .map(|(_, number)| number.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    } else {
                        references
                            .iter()
                            .map(|(kind, number)| format!("{kind} {number}"))
                            .collect::<Vec<_>>()
                            .join("; ")
                    };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "thmref" | "Thmref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let reference_kind = aux
                        .labels
                        .iter()
                        .find(|label| label.key == key)
                        .map(|label| {
                            let latest_section = scan
                                .sections
                                .iter()
                                .filter(|section| {
                                    section.numbered
                                        && section.file == label.file
                                        && section.offset_utf8 <= label.offset_utf8
                                })
                                .map(|section| (section.level, section.offset_utf8))
                                .max_by_key(|(_, offset)| *offset)
                                .or_else(|| {
                                    aux.toc
                                        .iter()
                                        .filter(|entry| {
                                            entry.file == label.file
                                                && entry.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|entry| (entry.level, entry.offset_utf8))
                                        .max_by_key(|(_, offset)| *offset)
                                });
                            let latest_equation = scan
                                .equations
                                .iter()
                                .filter(|equation| {
                                    equation.numbered
                                        && equation.file == label.file
                                        && equation.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|equation| equation.offset_utf8);
                            let latest_float = scan
                                .floats
                                .iter()
                                .filter(|float| {
                                    float.numbered
                                        && float.file == label.file
                                        && float.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|float| float.offset_utf8);
                            let latest_block = scan
                                .blocks
                                .iter()
                                .filter(|block| {
                                    block.numbered
                                        && block.file == label.file
                                        && block.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|block| block.offset_utf8);
                            if latest_float.is_some_and(|float| {
                                latest_section
                                    .map(|(_, section_offset)| float.offset_utf8 > section_offset)
                                    .unwrap_or(true)
                                    && latest_equation
                                        .map(|equation| float.offset_utf8 > equation.offset_utf8)
                                        .unwrap_or(true)
                                    && latest_block
                                        .map(|block| float.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                if latest_float.is_some_and(|float| float.kind == FloatKind::Figure)
                                {
                                    "Figure"
                                } else if latest_float
                                    .is_some_and(|float| float.kind == FloatKind::Table)
                                {
                                    "Table"
                                } else {
                                    "Algorithm"
                                }
                            } else if latest_equation.is_some_and(|equation| {
                                latest_section
                                    .map(|(_, section_offset)| {
                                        equation.offset_utf8 > section_offset
                                    })
                                    .unwrap_or(true)
                                    && latest_block
                                        .map(|block| equation.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                "Equation"
                            } else if let Some(block) = latest_block {
                                block.kind.display_name()
                            } else if let Some((level, _)) = latest_section {
                                if aux.label_number(&key).is_some_and(|number| {
                                    number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                }) {
                                    "Appendix"
                                } else if level == 0 {
                                    "Chapter"
                                } else if level == 2 {
                                    "Subsection"
                                } else if level == 3 {
                                    "Subsubsection"
                                } else if level == 4 {
                                    "Paragraph"
                                } else if level >= 5 {
                                    "Subparagraph"
                                } else {
                                    "Section"
                                }
                            } else if aux
                                .label_number(&key)
                                .is_some_and(|number| number.chars().all(|ch| ch.is_ascii_digit()))
                            {
                                "Equation"
                            } else {
                                "Section"
                            }
                        })
                        .or_else(|| {
                            aux.label_number(&key).map(|number| {
                                if number.chars().all(|ch| ch.is_ascii_digit()) {
                                    "Equation"
                                } else if number.starts_with(|ch: char| ch.is_ascii_uppercase()) {
                                    "Appendix"
                                } else {
                                    "Section"
                                }
                            })
                        })
                        .unwrap_or("Section");
                    let mut rendered = format!(
                        "{reference_kind} {}",
                        aux.label_number(&key).unwrap_or("??")
                    );
                    if command_name == "Thmref" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "fullref" | "Fullref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let reference_kind = aux
                        .labels
                        .iter()
                        .find(|label| label.key == key)
                        .map(|label| {
                            let latest_section = scan
                                .sections
                                .iter()
                                .filter(|section| {
                                    section.numbered
                                        && section.file == label.file
                                        && section.offset_utf8 <= label.offset_utf8
                                })
                                .map(|section| (section.level, section.offset_utf8))
                                .max_by_key(|(_, offset)| *offset)
                                .or_else(|| {
                                    aux.toc
                                        .iter()
                                        .filter(|entry| {
                                            entry.file == label.file
                                                && entry.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|entry| (entry.level, entry.offset_utf8))
                                        .max_by_key(|(_, offset)| *offset)
                                });
                            let latest_equation = scan
                                .equations
                                .iter()
                                .filter(|equation| {
                                    equation.numbered
                                        && equation.file == label.file
                                        && equation.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|equation| equation.offset_utf8);
                            let latest_float = scan
                                .floats
                                .iter()
                                .filter(|float| {
                                    float.numbered
                                        && float.file == label.file
                                        && float.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|float| float.offset_utf8);
                            let latest_block = scan
                                .blocks
                                .iter()
                                .filter(|block| {
                                    block.numbered
                                        && block.file == label.file
                                        && block.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|block| block.offset_utf8);
                            if latest_float.is_some_and(|float| {
                                latest_section
                                    .map(|(_, section_offset)| float.offset_utf8 > section_offset)
                                    .unwrap_or(true)
                                    && latest_equation
                                        .map(|equation| float.offset_utf8 > equation.offset_utf8)
                                        .unwrap_or(true)
                                    && latest_block
                                        .map(|block| float.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                if latest_float.is_some_and(|float| float.kind == FloatKind::Figure)
                                {
                                    "Figure"
                                } else if latest_float
                                    .is_some_and(|float| float.kind == FloatKind::Table)
                                {
                                    "Table"
                                } else {
                                    "Algorithm"
                                }
                            } else if latest_equation.is_some_and(|equation| {
                                latest_section
                                    .map(|(_, section_offset)| {
                                        equation.offset_utf8 > section_offset
                                    })
                                    .unwrap_or(true)
                                    && latest_block
                                        .map(|block| equation.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                "Equation"
                            } else if let Some(block) = latest_block {
                                block.kind.display_name()
                            } else if let Some((level, _)) = latest_section {
                                if aux.label_number(&key).is_some_and(|number| {
                                    number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                }) {
                                    "Appendix"
                                } else if level == 0 {
                                    "Chapter"
                                } else if level == 2 {
                                    "Subsection"
                                } else if level == 3 {
                                    "Subsubsection"
                                } else if level == 4 {
                                    "Paragraph"
                                } else if level >= 5 {
                                    "Subparagraph"
                                } else {
                                    "Section"
                                }
                            } else if aux
                                .label_number(&key)
                                .is_some_and(|number| number.chars().all(|ch| ch.is_ascii_digit()))
                            {
                                "Equation"
                            } else {
                                "Section"
                            }
                        })
                        .or_else(|| {
                            aux.label_number(&key).map(|number| {
                                if number.chars().all(|ch| ch.is_ascii_digit()) {
                                    "Equation"
                                } else if number.starts_with(|ch: char| ch.is_ascii_uppercase()) {
                                    "Appendix"
                                } else {
                                    "Section"
                                }
                            })
                        })
                        .unwrap_or("Section");
                    let title = scan
                        .labels
                        .iter()
                        .find(|label| label.key == key)
                        .and_then(|label| {
                            let latest_section = scan
                                .sections
                                .iter()
                                .filter(|section| {
                                    section.file == label.file
                                        && section.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|section| section.offset_utf8);
                            let latest_caption = scan
                                .captions
                                .iter()
                                .filter(|caption| {
                                    caption.file == label.file
                                        && caption.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|caption| caption.offset_utf8);
                            let latest_block = scan
                                .blocks
                                .iter()
                                .filter(|block| {
                                    block.file == label.file
                                        && block.offset_utf8 <= label.offset_utf8
                                        && block
                                            .title
                                            .as_deref()
                                            .is_some_and(|title| !title.is_empty())
                                })
                                .max_by_key(|block| block.offset_utf8);
                            if latest_caption.is_some_and(|caption| {
                                latest_section
                                    .map(|section| caption.offset_utf8 > section.offset_utf8)
                                    .unwrap_or(true)
                                    && latest_block
                                        .map(|block| caption.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                latest_caption.map(|caption| caption.body_title.as_str())
                            } else if latest_block.is_some_and(|block| {
                                latest_section
                                    .map(|section| block.offset_utf8 > section.offset_utf8)
                                    .unwrap_or(true)
                            }) {
                                latest_block.and_then(|block| block.title.as_deref())
                            } else {
                                latest_section.map(|section| section.body_title.as_str())
                            }
                        })
                        .or_else(|| aux.label_title(&key))
                        .unwrap_or("??");
                    let mut rendered = format!(
                        "{reference_kind} {} ({title})",
                        aux.label_number(&key).unwrap_or("??")
                    );
                    if command_name == "Fullref" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "labelcref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            let number = aux.label_number(key).unwrap_or("??");
                            let is_equation = scan
                                .labels
                                .iter()
                                .find(|label| label.key == key)
                                .map(|label| {
                                    let latest_section = scan
                                        .sections
                                        .iter()
                                        .filter(|section| {
                                            section.numbered
                                                && section.file == label.file
                                                && section.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|section| section.offset_utf8)
                                        .max();
                                    let latest_equation = scan
                                        .equations
                                        .iter()
                                        .filter(|equation| {
                                            equation.numbered
                                                && equation.file == label.file
                                                && equation.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|equation| equation.offset_utf8)
                                        .max();
                                    let latest_float = scan
                                        .floats
                                        .iter()
                                        .filter(|float| {
                                            float.numbered
                                                && float.file == label.file
                                                && float.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|float| float.offset_utf8)
                                        .max();
                                    let latest_block = scan
                                        .blocks
                                        .iter()
                                        .filter(|block| {
                                            block.numbered
                                                && block.file == label.file
                                                && block.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|block| block.offset_utf8)
                                        .max();
                                    latest_equation.is_some_and(|equation_offset| {
                                        latest_section
                                            .map(|section_offset| equation_offset > section_offset)
                                            .unwrap_or(true)
                                            && latest_float
                                                .map(|float_offset| equation_offset > float_offset)
                                                .unwrap_or(true)
                                            && latest_block
                                                .map(|block_offset| equation_offset > block_offset)
                                                .unwrap_or(true)
                                    })
                                })
                                .or_else(|| {
                                    aux.label_number(key)
                                        .map(|stored| stored.chars().all(|ch| ch.is_ascii_digit()))
                                })
                                .unwrap_or(false);
                            if is_equation {
                                format!("({number})")
                            } else {
                                number.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "crefrange" | "Crefrange" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((first_end, first_key)) = read_braced_argument(source, cursor) {
                    if let Some((argument_end, second_key)) =
                        read_braced_argument(source, first_end)
                    {
                        let first_kind = aux
                            .labels
                            .iter()
                            .find(|label| label.key == first_key)
                            .map(|label| {
                                let latest_section = scan
                                    .sections
                                    .iter()
                                    .filter(|section| {
                                        section.numbered
                                            && section.file == label.file
                                            && section.offset_utf8 <= label.offset_utf8
                                    })
                                    .map(|section| (section.level, section.offset_utf8))
                                    .max_by_key(|(_, offset)| *offset)
                                    .or_else(|| {
                                        aux.toc
                                            .iter()
                                            .filter(|entry| {
                                                entry.file == label.file
                                                    && entry.offset_utf8 <= label.offset_utf8
                                            })
                                            .map(|entry| (entry.level, entry.offset_utf8))
                                            .max_by_key(|(_, offset)| *offset)
                                    });
                                let latest_equation = scan
                                    .equations
                                    .iter()
                                    .filter(|equation| {
                                        equation.numbered
                                            && equation.file == label.file
                                            && equation.offset_utf8 <= label.offset_utf8
                                    })
                                    .max_by_key(|equation| equation.offset_utf8);
                                let latest_float = scan
                                    .floats
                                    .iter()
                                    .filter(|float| {
                                        float.numbered
                                            && float.file == label.file
                                            && float.offset_utf8 <= label.offset_utf8
                                    })
                                    .max_by_key(|float| float.offset_utf8);
                                let latest_block = scan
                                    .blocks
                                    .iter()
                                    .filter(|block| {
                                        block.numbered
                                            && block.file == label.file
                                            && block.offset_utf8 <= label.offset_utf8
                                    })
                                    .max_by_key(|block| block.offset_utf8);
                                if latest_float.is_some_and(|float| {
                                    latest_section
                                        .map(|(_, section_offset)| {
                                            float.offset_utf8 > section_offset
                                        })
                                        .unwrap_or(true)
                                        && latest_equation
                                            .map(|equation| {
                                                float.offset_utf8 > equation.offset_utf8
                                            })
                                            .unwrap_or(true)
                                        && latest_block
                                            .map(|block| float.offset_utf8 > block.offset_utf8)
                                            .unwrap_or(true)
                                }) {
                                    if latest_float
                                        .is_some_and(|float| float.kind == FloatKind::Figure)
                                    {
                                        "Figure"
                                    } else if latest_float
                                        .is_some_and(|float| float.kind == FloatKind::Table)
                                    {
                                        "Table"
                                    } else {
                                        "Algorithm"
                                    }
                                } else if latest_equation.is_some_and(|equation| {
                                    latest_section
                                        .map(|(_, section_offset)| {
                                            equation.offset_utf8 > section_offset
                                        })
                                        .unwrap_or(true)
                                        && latest_block
                                            .map(|block| equation.offset_utf8 > block.offset_utf8)
                                            .unwrap_or(true)
                                }) {
                                    "Equation"
                                } else if let Some(block) = latest_block {
                                    block.kind.display_name()
                                } else if let Some((level, _)) = latest_section {
                                    if aux.label_number(&first_key).is_some_and(|number| {
                                        number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                    }) {
                                        "Appendix"
                                    } else if level == 0 {
                                        "Chapter"
                                    } else if level == 2 {
                                        "Subsection"
                                    } else if level == 3 {
                                        "Subsubsection"
                                    } else if level == 4 {
                                        "Paragraph"
                                    } else if level >= 5 {
                                        "Subparagraph"
                                    } else {
                                        "Section"
                                    }
                                } else if aux.label_number(&first_key).is_some_and(|number| {
                                    number.chars().all(|ch| ch.is_ascii_digit())
                                }) {
                                    "Equation"
                                } else {
                                    "Section"
                                }
                            })
                            .or_else(|| {
                                aux.label_number(&first_key).map(|number| {
                                    if number.chars().all(|ch| ch.is_ascii_digit()) {
                                        "Equation"
                                    } else if number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                    {
                                        "Appendix"
                                    } else {
                                        "Section"
                                    }
                                })
                            })
                            .unwrap_or("Section");
                        let second_kind = aux
                            .labels
                            .iter()
                            .find(|label| label.key == second_key)
                            .map(|label| {
                                let latest_section = scan
                                    .sections
                                    .iter()
                                    .filter(|section| {
                                        section.numbered
                                            && section.file == label.file
                                            && section.offset_utf8 <= label.offset_utf8
                                    })
                                    .map(|section| (section.level, section.offset_utf8))
                                    .max_by_key(|(_, offset)| *offset)
                                    .or_else(|| {
                                        aux.toc
                                            .iter()
                                            .filter(|entry| {
                                                entry.file == label.file
                                                    && entry.offset_utf8 <= label.offset_utf8
                                            })
                                            .map(|entry| (entry.level, entry.offset_utf8))
                                            .max_by_key(|(_, offset)| *offset)
                                    });
                                let latest_equation = scan
                                    .equations
                                    .iter()
                                    .filter(|equation| {
                                        equation.numbered
                                            && equation.file == label.file
                                            && equation.offset_utf8 <= label.offset_utf8
                                    })
                                    .max_by_key(|equation| equation.offset_utf8);
                                let latest_float = scan
                                    .floats
                                    .iter()
                                    .filter(|float| {
                                        float.numbered
                                            && float.file == label.file
                                            && float.offset_utf8 <= label.offset_utf8
                                    })
                                    .max_by_key(|float| float.offset_utf8);
                                let latest_block = scan
                                    .blocks
                                    .iter()
                                    .filter(|block| {
                                        block.numbered
                                            && block.file == label.file
                                            && block.offset_utf8 <= label.offset_utf8
                                    })
                                    .max_by_key(|block| block.offset_utf8);
                                if latest_float.is_some_and(|float| {
                                    latest_section
                                        .map(|(_, section_offset)| {
                                            float.offset_utf8 > section_offset
                                        })
                                        .unwrap_or(true)
                                        && latest_equation
                                            .map(|equation| {
                                                float.offset_utf8 > equation.offset_utf8
                                            })
                                            .unwrap_or(true)
                                        && latest_block
                                            .map(|block| float.offset_utf8 > block.offset_utf8)
                                            .unwrap_or(true)
                                }) {
                                    if latest_float
                                        .is_some_and(|float| float.kind == FloatKind::Figure)
                                    {
                                        "Figure"
                                    } else if latest_float
                                        .is_some_and(|float| float.kind == FloatKind::Table)
                                    {
                                        "Table"
                                    } else {
                                        "Algorithm"
                                    }
                                } else if latest_equation.is_some_and(|equation| {
                                    latest_section
                                        .map(|(_, section_offset)| {
                                            equation.offset_utf8 > section_offset
                                        })
                                        .unwrap_or(true)
                                        && latest_block
                                            .map(|block| equation.offset_utf8 > block.offset_utf8)
                                            .unwrap_or(true)
                                }) {
                                    "Equation"
                                } else if let Some(block) = latest_block {
                                    block.kind.display_name()
                                } else if let Some((level, _)) = latest_section {
                                    if aux.label_number(&second_key).is_some_and(|number| {
                                        number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                    }) {
                                        "Appendix"
                                    } else if level == 0 {
                                        "Chapter"
                                    } else if level == 2 {
                                        "Subsection"
                                    } else if level == 3 {
                                        "Subsubsection"
                                    } else if level == 4 {
                                        "Paragraph"
                                    } else if level >= 5 {
                                        "Subparagraph"
                                    } else {
                                        "Section"
                                    }
                                } else if aux.label_number(&second_key).is_some_and(|number| {
                                    number.chars().all(|ch| ch.is_ascii_digit())
                                }) {
                                    "Equation"
                                } else {
                                    "Section"
                                }
                            })
                            .or_else(|| {
                                aux.label_number(&second_key).map(|number| {
                                    if number.chars().all(|ch| ch.is_ascii_digit()) {
                                        "Equation"
                                    } else if number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                    {
                                        "Appendix"
                                    } else {
                                        "Section"
                                    }
                                })
                            })
                            .unwrap_or(first_kind);
                        let range_kind = if first_kind == second_kind {
                            match first_kind {
                                "Chapter" => "Chapters".to_string(),
                                "Appendix" => "Appendices".to_string(),
                                "Equation" => "Equations".to_string(),
                                "Figure" => "Figures".to_string(),
                                "Table" => "Tables".to_string(),
                                "Algorithm" => "Algorithms".to_string(),
                                "Theorem" => "Theorems".to_string(),
                                "Lemma" => "Lemmas".to_string(),
                                "Proposition" => "Propositions".to_string(),
                                "Corollary" => "Corollaries".to_string(),
                                "Definition" => "Definitions".to_string(),
                                "Remark" => "Remarks".to_string(),
                                "Claim" => "Claims".to_string(),
                                "Example" => "Examples".to_string(),
                                "Assumption" => "Assumptions".to_string(),
                                "Conjecture" => "Conjectures".to_string(),
                                "Axiom" => "Axioms".to_string(),
                                "Fact" => "Facts".to_string(),
                                "Observation" => "Observations".to_string(),
                                "Problem" => "Problems".to_string(),
                                "Exercise" => "Exercises".to_string(),
                                "Question" => "Questions".to_string(),
                                "Notation" => "Notations".to_string(),
                                "Subsection" => "Subsections".to_string(),
                                "Subsubsection" => "Subsubsections".to_string(),
                                "Paragraph" => "Paragraphs".to_string(),
                                "Subparagraph" => "Subparagraphs".to_string(),
                                other => pluralize_kind_name(other),
                            }
                        } else {
                            "Sections".to_string()
                        };
                        let mut rendered = format!(
                            "{range_kind} {} to {}",
                            aux.label_number(&first_key).unwrap_or("??"),
                            aux.label_number(&second_key).unwrap_or("??")
                        );
                        if command_name == "Crefrange" {
                            let mut chars = rendered.chars();
                            if let Some(first) = chars.next() {
                                rendered =
                                    first.to_uppercase().collect::<String>() + chars.as_str();
                            }
                        }
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "namecref" | "nameCref" | "lcnamecref" | "namecrefs" | "nameCrefs" | "lcnamecrefs" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let reference_kinds = keys
                        .split(',')
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.labels
                                .iter()
                                .find(|label| label.key == key)
                                .map(|label| {
                                    let latest_section = scan
                                        .sections
                                        .iter()
                                        .filter(|section| {
                                            section.numbered
                                                && section.file == label.file
                                                && section.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|section| (section.level, section.offset_utf8))
                                        .max_by_key(|(_, offset)| *offset)
                                        .or_else(|| {
                                            aux.toc
                                                .iter()
                                                .filter(|entry| {
                                                    entry.file == label.file
                                                        && entry.offset_utf8 <= label.offset_utf8
                                                })
                                                .map(|entry| (entry.level, entry.offset_utf8))
                                                .max_by_key(|(_, offset)| *offset)
                                        });
                                    let latest_equation = scan
                                        .equations
                                        .iter()
                                        .filter(|equation| {
                                            equation.numbered
                                                && equation.file == label.file
                                                && equation.offset_utf8 <= label.offset_utf8
                                        })
                                        .max_by_key(|equation| equation.offset_utf8);
                                    let latest_float = scan
                                        .floats
                                        .iter()
                                        .filter(|float| {
                                            float.numbered
                                                && float.file == label.file
                                                && float.offset_utf8 <= label.offset_utf8
                                        })
                                        .max_by_key(|float| float.offset_utf8);
                                    let latest_block = scan
                                        .blocks
                                        .iter()
                                        .filter(|block| {
                                            block.numbered
                                                && block.file == label.file
                                                && block.offset_utf8 <= label.offset_utf8
                                        })
                                        .max_by_key(|block| block.offset_utf8);
                                    if latest_float.is_some_and(|float| {
                                        latest_section
                                            .map(|(_, section_offset)| {
                                                float.offset_utf8 > section_offset
                                            })
                                            .unwrap_or(true)
                                            && latest_equation
                                                .map(|equation| {
                                                    float.offset_utf8 > equation.offset_utf8
                                                })
                                                .unwrap_or(true)
                                            && latest_block
                                                .map(|block| float.offset_utf8 > block.offset_utf8)
                                                .unwrap_or(true)
                                    }) {
                                        if latest_float
                                            .is_some_and(|float| float.kind == FloatKind::Figure)
                                        {
                                            "figure"
                                        } else if latest_float
                                            .is_some_and(|float| float.kind == FloatKind::Table)
                                        {
                                            "table"
                                        } else {
                                            "algorithm"
                                        }
                                    } else if latest_equation.is_some_and(|equation| {
                                        latest_section
                                            .map(|(_, section_offset)| {
                                                equation.offset_utf8 > section_offset
                                            })
                                            .unwrap_or(true)
                                            && latest_block
                                                .map(|block| {
                                                    equation.offset_utf8 > block.offset_utf8
                                                })
                                                .unwrap_or(true)
                                    }) {
                                        "equation"
                                    } else if let Some(block) = latest_block {
                                        block.kind.lower_name()
                                    } else if let Some((level, _)) = latest_section {
                                        if aux.label_number(key).is_some_and(|number| {
                                            number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                        }) {
                                            "appendix"
                                        } else if level == 0 {
                                            "chapter"
                                        } else if level == 2 {
                                            "subsection"
                                        } else if level == 3 {
                                            "subsubsection"
                                        } else if level == 4 {
                                            "paragraph"
                                        } else if level >= 5 {
                                            "subparagraph"
                                        } else {
                                            "section"
                                        }
                                    } else if aux.label_number(key).is_some_and(|number| {
                                        number.chars().all(|ch| ch.is_ascii_digit())
                                    }) {
                                        "equation"
                                    } else {
                                        "section"
                                    }
                                })
                                .or_else(|| {
                                    aux.label_number(key).map(|number| {
                                        if number.chars().all(|ch| ch.is_ascii_digit()) {
                                            "equation"
                                        } else if number
                                            .starts_with(|ch: char| ch.is_ascii_uppercase())
                                        {
                                            "appendix"
                                        } else {
                                            "section"
                                        }
                                    })
                                })
                                .unwrap_or("section")
                        })
                        .collect::<Vec<_>>();
                    if reference_kinds.is_empty() {
                        index = argument_end;
                        continue;
                    }
                    let same_kind = reference_kinds
                        .iter()
                        .all(|kind| *kind == reference_kinds[0]);
                    let mut rendered = if same_kind {
                        match reference_kinds[0] {
                            "chapter" => {
                                if reference_kinds.len() == 1 {
                                    "chapter".to_string()
                                } else {
                                    "chapters".to_string()
                                }
                            }
                            "equation" => {
                                if reference_kinds.len() == 1 {
                                    "equation".to_string()
                                } else {
                                    "equations".to_string()
                                }
                            }
                            "figure" => {
                                if reference_kinds.len() == 1 {
                                    "figure".to_string()
                                } else {
                                    "figures".to_string()
                                }
                            }
                            "table" => {
                                if reference_kinds.len() == 1 {
                                    "table".to_string()
                                } else {
                                    "tables".to_string()
                                }
                            }
                            "algorithm" => {
                                if reference_kinds.len() == 1 {
                                    "algorithm".to_string()
                                } else {
                                    "algorithms".to_string()
                                }
                            }
                            "theorem" => {
                                if reference_kinds.len() == 1 {
                                    "theorem".to_string()
                                } else {
                                    "theorems".to_string()
                                }
                            }
                            "lemma" => {
                                if reference_kinds.len() == 1 {
                                    "lemma".to_string()
                                } else {
                                    "lemmas".to_string()
                                }
                            }
                            "proposition" => {
                                if reference_kinds.len() == 1 {
                                    "proposition".to_string()
                                } else {
                                    "propositions".to_string()
                                }
                            }
                            "corollary" => {
                                if reference_kinds.len() == 1 {
                                    "corollary".to_string()
                                } else {
                                    "corollaries".to_string()
                                }
                            }
                            "definition" => {
                                if reference_kinds.len() == 1 {
                                    "definition".to_string()
                                } else {
                                    "definitions".to_string()
                                }
                            }
                            "remark" => {
                                if reference_kinds.len() == 1 {
                                    "remark".to_string()
                                } else {
                                    "remarks".to_string()
                                }
                            }
                            "claim" => {
                                if reference_kinds.len() == 1 {
                                    "claim".to_string()
                                } else {
                                    "claims".to_string()
                                }
                            }
                            "example" => {
                                if reference_kinds.len() == 1 {
                                    "example".to_string()
                                } else {
                                    "examples".to_string()
                                }
                            }
                            "assumption" => {
                                if reference_kinds.len() == 1 {
                                    "assumption".to_string()
                                } else {
                                    "assumptions".to_string()
                                }
                            }
                            "conjecture" => {
                                if reference_kinds.len() == 1 {
                                    "conjecture".to_string()
                                } else {
                                    "conjectures".to_string()
                                }
                            }
                            "axiom" => {
                                if reference_kinds.len() == 1 {
                                    "axiom".to_string()
                                } else {
                                    "axioms".to_string()
                                }
                            }
                            "fact" => {
                                if reference_kinds.len() == 1 {
                                    "fact".to_string()
                                } else {
                                    "facts".to_string()
                                }
                            }
                            "observation" => {
                                if reference_kinds.len() == 1 {
                                    "observation".to_string()
                                } else {
                                    "observations".to_string()
                                }
                            }
                            "problem" => {
                                if reference_kinds.len() == 1 {
                                    "problem".to_string()
                                } else {
                                    "problems".to_string()
                                }
                            }
                            "exercise" => {
                                if reference_kinds.len() == 1 {
                                    "exercise".to_string()
                                } else {
                                    "exercises".to_string()
                                }
                            }
                            "question" => {
                                if reference_kinds.len() == 1 {
                                    "question".to_string()
                                } else {
                                    "questions".to_string()
                                }
                            }
                            "notation" => {
                                if reference_kinds.len() == 1 {
                                    "notation".to_string()
                                } else {
                                    "notations".to_string()
                                }
                            }
                            "appendix" => {
                                if reference_kinds.len() == 1 {
                                    "appendix".to_string()
                                } else {
                                    "appendices".to_string()
                                }
                            }
                            "subsection" => {
                                if reference_kinds.len() == 1 {
                                    "subsection".to_string()
                                } else {
                                    "subsections".to_string()
                                }
                            }
                            "subsubsection" => {
                                if reference_kinds.len() == 1 {
                                    "subsubsection".to_string()
                                } else {
                                    "subsubsections".to_string()
                                }
                            }
                            "paragraph" => {
                                if reference_kinds.len() == 1 {
                                    "paragraph".to_string()
                                } else {
                                    "paragraphs".to_string()
                                }
                            }
                            "subparagraph" => {
                                if reference_kinds.len() == 1 {
                                    "subparagraph".to_string()
                                } else {
                                    "subparagraphs".to_string()
                                }
                            }
                            other => {
                                if reference_kinds.len() == 1 {
                                    other.to_string()
                                } else {
                                    pluralize_kind_name(other).to_lowercase()
                                }
                            }
                        }
                    } else {
                        reference_kinds.join("; ")
                    };
                    if command_name == "nameCref" || command_name == "nameCrefs" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    } else if command_name == "lcnamecref" || command_name == "lcnamecrefs" {
                        rendered = rendered.to_lowercase();
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "cpageref" | "Cpageref" | "vpageref" | "autopageref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let mut rendered = aux
                        .label_page(&key)
                        .map(|page| format!("page {page}"))
                        .unwrap_or_else(|| "page ??".to_string());
                    if command_name == "Cpageref" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "labelcpageref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.label_page(key)
                                .map(|page| page.to_string())
                                .unwrap_or_else(|| "??".to_string())
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "pagerefrange" | "cpagerefrange" | "Cpagerefrange" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((first_end, first_key)) = read_braced_argument(source, cursor) {
                    if let Some((argument_end, second_key)) =
                        read_braced_argument(source, first_end)
                    {
                        let mut rendered = format!(
                            "pages {} to {}",
                            aux.label_page(&first_key)
                                .map(|page| page.to_string())
                                .unwrap_or_else(|| "??".to_string()),
                            aux.label_page(&second_key)
                                .map(|page| page.to_string())
                                .unwrap_or_else(|| "??".to_string()),
                        );
                        if command_name == "Cpagerefrange" {
                            let mut chars = rendered.chars();
                            if let Some(first) = chars.next() {
                                rendered =
                                    first.to_uppercase().collect::<String>() + chars.as_str();
                            }
                        }
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "vpagerefrange" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((first_end, first_key)) = read_braced_argument(source, cursor) {
                    if let Some((argument_end, second_key)) =
                        read_braced_argument(source, first_end)
                    {
                        let rendered = format!(
                            "pages {} to {}",
                            aux.label_page(&first_key)
                                .map(|page| page.to_string())
                                .unwrap_or_else(|| "??".to_string()),
                            aux.label_page(&second_key)
                                .map(|page| page.to_string())
                                .unwrap_or_else(|| "??".to_string()),
                        );
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "vref" | "Vref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let reference_kind = aux
                        .labels
                        .iter()
                        .find(|label| label.key == key)
                        .map(|label| {
                            let latest_section = scan
                                .sections
                                .iter()
                                .filter(|section| {
                                    section.numbered
                                        && section.file == label.file
                                        && section.offset_utf8 <= label.offset_utf8
                                })
                                .map(|section| (section.level, section.offset_utf8))
                                .max_by_key(|(_, offset)| *offset)
                                .or_else(|| {
                                    aux.toc
                                        .iter()
                                        .filter(|entry| {
                                            entry.file == label.file
                                                && entry.offset_utf8 <= label.offset_utf8
                                        })
                                        .map(|entry| (entry.level, entry.offset_utf8))
                                        .max_by_key(|(_, offset)| *offset)
                                });
                            let latest_equation = scan
                                .equations
                                .iter()
                                .filter(|equation| {
                                    equation.numbered
                                        && equation.file == label.file
                                        && equation.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|equation| equation.offset_utf8);
                            let latest_float = scan
                                .floats
                                .iter()
                                .filter(|float| {
                                    float.numbered
                                        && float.file == label.file
                                        && float.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|float| float.offset_utf8);
                            let latest_block = scan
                                .blocks
                                .iter()
                                .filter(|block| {
                                    block.numbered
                                        && block.file == label.file
                                        && block.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|block| block.offset_utf8);
                            if latest_float.is_some_and(|float| {
                                latest_section
                                    .map(|(_, section_offset)| float.offset_utf8 > section_offset)
                                    .unwrap_or(true)
                                    && latest_equation
                                        .map(|equation| float.offset_utf8 > equation.offset_utf8)
                                        .unwrap_or(true)
                                    && latest_block
                                        .map(|block| float.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                if latest_float.is_some_and(|float| float.kind == FloatKind::Figure)
                                {
                                    "figure"
                                } else if latest_float
                                    .is_some_and(|float| float.kind == FloatKind::Table)
                                {
                                    "table"
                                } else {
                                    "algorithm"
                                }
                            } else if latest_equation.is_some_and(|equation| {
                                latest_section
                                    .map(|(_, section_offset)| {
                                        equation.offset_utf8 > section_offset
                                    })
                                    .unwrap_or(true)
                                    && latest_block
                                        .map(|block| equation.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                "equation"
                            } else if let Some(block) = latest_block {
                                block.kind.lower_name()
                            } else if let Some((level, _)) = latest_section {
                                if aux.label_number(&key).is_some_and(|number| {
                                    number.starts_with(|ch: char| ch.is_ascii_uppercase())
                                }) {
                                    "appendix"
                                } else if level == 0 {
                                    "chapter"
                                } else if level == 2 {
                                    "subsection"
                                } else if level == 3 {
                                    "subsubsection"
                                } else if level == 4 {
                                    "paragraph"
                                } else if level >= 5 {
                                    "subparagraph"
                                } else {
                                    "section"
                                }
                            } else if aux
                                .label_number(&key)
                                .is_some_and(|number| number.chars().all(|ch| ch.is_ascii_digit()))
                            {
                                "equation"
                            } else {
                                "section"
                            }
                        })
                        .or_else(|| {
                            aux.label_number(&key).map(|number| {
                                if number.chars().all(|ch| ch.is_ascii_digit()) {
                                    "equation"
                                } else if number.starts_with(|ch: char| ch.is_ascii_uppercase()) {
                                    "appendix"
                                } else {
                                    "section"
                                }
                            })
                        })
                        .unwrap_or("section");
                    let number = aux.label_number(&key).unwrap_or("??");
                    let page = aux
                        .label_page(&key)
                        .map(|page| page.to_string())
                        .unwrap_or_else(|| "??".to_string());
                    let mut rendered = format!("{reference_kind} {number} on page {page}");
                    if command_name == "Vref" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "vrefrange" | "Vrefrange" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((first_end, first_key)) = read_braced_argument(source, cursor) {
                    if let Some((argument_end, second_key)) =
                        read_braced_argument(source, first_end)
                    {
                        let references = [&first_key, &second_key]
                            .into_iter()
                            .map(|key| {
                                let reference_kind = aux
                                    .labels
                                    .iter()
                                    .find(|label| label.key == *key)
                                    .map(|label| {
                                        let latest_section = scan
                                            .sections
                                            .iter()
                                            .filter(|section| {
                                                section.numbered
                                                    && section.file == label.file
                                                    && section.offset_utf8 <= label.offset_utf8
                                            })
                                            .map(|section| (section.level, section.offset_utf8))
                                            .max_by_key(|(_, offset)| *offset)
                                            .or_else(|| {
                                                aux.toc
                                                    .iter()
                                                    .filter(|entry| {
                                                        entry.file == label.file
                                                            && entry.offset_utf8
                                                                <= label.offset_utf8
                                                    })
                                                    .map(|entry| (entry.level, entry.offset_utf8))
                                                    .max_by_key(|(_, offset)| *offset)
                                            });
                                        let latest_equation = scan
                                            .equations
                                            .iter()
                                            .filter(|equation| {
                                                equation.numbered
                                                    && equation.file == label.file
                                                    && equation.offset_utf8 <= label.offset_utf8
                                            })
                                            .max_by_key(|equation| equation.offset_utf8);
                                        let latest_float = scan
                                            .floats
                                            .iter()
                                            .filter(|float| {
                                                float.numbered
                                                    && float.file == label.file
                                                    && float.offset_utf8 <= label.offset_utf8
                                            })
                                            .max_by_key(|float| float.offset_utf8);
                                        let latest_block = scan
                                            .blocks
                                            .iter()
                                            .filter(|block| {
                                                block.numbered
                                                    && block.file == label.file
                                                    && block.offset_utf8 <= label.offset_utf8
                                            })
                                            .max_by_key(|block| block.offset_utf8);
                                        if latest_float.is_some_and(|float| {
                                            latest_section
                                                .map(|(_, section_offset)| {
                                                    float.offset_utf8 > section_offset
                                                })
                                                .unwrap_or(true)
                                                && latest_equation
                                                    .map(|equation| {
                                                        float.offset_utf8 > equation.offset_utf8
                                                    })
                                                    .unwrap_or(true)
                                                && latest_block
                                                    .map(|block| {
                                                        float.offset_utf8 > block.offset_utf8
                                                    })
                                                    .unwrap_or(true)
                                        }) {
                                            if latest_float.is_some_and(|float| {
                                                float.kind == FloatKind::Figure
                                            }) {
                                                "figure"
                                            } else if latest_float
                                                .is_some_and(|float| float.kind == FloatKind::Table)
                                            {
                                                "table"
                                            } else {
                                                "algorithm"
                                            }
                                        } else if latest_equation.is_some_and(|equation| {
                                            latest_section
                                                .map(|(_, section_offset)| {
                                                    equation.offset_utf8 > section_offset
                                                })
                                                .unwrap_or(true)
                                                && latest_block
                                                    .map(|block| {
                                                        equation.offset_utf8 > block.offset_utf8
                                                    })
                                                    .unwrap_or(true)
                                        }) {
                                            "equation"
                                        } else if let Some(block) = latest_block {
                                            block.kind.lower_name()
                                        } else if let Some((level, _)) = latest_section {
                                            if aux.label_number(key).is_some_and(|number| {
                                                number
                                                    .starts_with(|ch: char| ch.is_ascii_uppercase())
                                            }) {
                                                "appendix"
                                            } else if level == 0 {
                                                "chapter"
                                            } else if level == 2 {
                                                "subsection"
                                            } else if level == 3 {
                                                "subsubsection"
                                            } else if level == 4 {
                                                "paragraph"
                                            } else if level >= 5 {
                                                "subparagraph"
                                            } else {
                                                "section"
                                            }
                                        } else if aux.label_number(key).is_some_and(|number| {
                                            number.chars().all(|ch| ch.is_ascii_digit())
                                        }) {
                                            "equation"
                                        } else {
                                            "section"
                                        }
                                    })
                                    .or_else(|| {
                                        aux.label_number(key).map(|number| {
                                            if number.chars().all(|ch| ch.is_ascii_digit()) {
                                                "equation"
                                            } else if number
                                                .starts_with(|ch: char| ch.is_ascii_uppercase())
                                            {
                                                "appendix"
                                            } else {
                                                "section"
                                            }
                                        })
                                    })
                                    .unwrap_or("section");
                                let number = aux.label_number(key).unwrap_or("??");
                                let page = aux
                                    .label_page(key)
                                    .map(|page| page.to_string())
                                    .unwrap_or_else(|| "??".to_string());
                                format!("{reference_kind} {number} on page {page}")
                            })
                            .collect::<Vec<_>>();
                        let mut rendered = format!("{} to {}", references[0], references[1]);
                        if command_name == "Vrefrange" {
                            let mut chars = rendered.chars();
                            if let Some(first) = chars.next() {
                                rendered =
                                    first.to_uppercase().collect::<String>() + chars.as_str();
                            }
                        }
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "nameref" | "titleref" | "Titleref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let mut rendered = scan
                        .labels
                        .iter()
                        .find(|label| label.key == key)
                        .and_then(|label| {
                            let latest_section = scan
                                .sections
                                .iter()
                                .filter(|section| {
                                    section.file == label.file
                                        && section.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|section| section.offset_utf8);
                            let latest_caption = scan
                                .captions
                                .iter()
                                .filter(|caption| {
                                    caption.file == label.file
                                        && caption.offset_utf8 <= label.offset_utf8
                                })
                                .max_by_key(|caption| caption.offset_utf8);
                            let latest_block = scan
                                .blocks
                                .iter()
                                .filter(|block| {
                                    block.file == label.file
                                        && block.offset_utf8 <= label.offset_utf8
                                        && block
                                            .title
                                            .as_deref()
                                            .is_some_and(|title| !title.is_empty())
                                })
                                .max_by_key(|block| block.offset_utf8);
                            if latest_caption.is_some_and(|caption| {
                                latest_section
                                    .map(|section| caption.offset_utf8 > section.offset_utf8)
                                    .unwrap_or(true)
                                    && latest_block
                                        .map(|block| caption.offset_utf8 > block.offset_utf8)
                                        .unwrap_or(true)
                            }) {
                                latest_caption.map(|caption| caption.body_title.as_str())
                            } else if latest_block.is_some_and(|block| {
                                latest_section
                                    .map(|section| block.offset_utf8 > section.offset_utf8)
                                    .unwrap_or(true)
                            }) {
                                latest_block.and_then(|block| block.title.as_deref())
                            } else {
                                latest_section.map(|section| section.body_title.as_str())
                            }
                        })
                        .or_else(|| aux.label_title(&key))
                        .unwrap_or("??")
                        .to_string();
                    if command_name == "Titleref" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "eqref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let rendered = format!("({})", aux.label_number(&key).unwrap_or("??"));
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "subeqref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let rendered = format!("({})", aux.label_number(&key).unwrap_or("??"));
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "pageref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let rendered = aux
                        .label_page(&key)
                        .map(|page| page.to_string())
                        .unwrap_or_else(|| "??".to_string());
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "defcitealias" => {
                if let Some((first_end, _)) = read_braced_argument(source, command_end) {
                    if let Some((argument_end, _)) = read_braced_argument(source, first_end) {
                        let output_start_utf8 = output.len() as u32;
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output_start_utf8,
                            rendered: String::new(),
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "citetalias" | "Citetalias" | "citepalias" | "Citepalias" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, key)) = read_braced_argument(source, cursor) {
                    let mut rendered = aux
                        .citation_alias_text(&key)
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| "?".to_string());
                    if matches!(command_name.as_str(), "citepalias" | "Citepalias") {
                        rendered = format!("({rendered})");
                    }
                    if matches!(command_name.as_str(), "Citetalias" | "Citepalias") {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "supercite" | "Supercite" => {
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.bibliography_number(key)
                                .map(|number| number.to_string())
                                .unwrap_or_else(|| "?".to_string())
                        })
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join(", ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    if let Some(post_note) = post_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered.push_str(", ");
                        rendered.push_str(post_note);
                    }
                    let rendered = format!("^{rendered}");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "cite" => {
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.bibliography_number(key)
                                .map(|number| number.to_string())
                                .unwrap_or_else(|| "?".to_string())
                        })
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join(", ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    if let Some(post_note) = post_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered.push_str(", ");
                        rendered.push_str(post_note);
                    }
                    let rendered = format!("[{rendered}]");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citep" | "Citep" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            match (
                                aux.citation_display_author(key, starred),
                                aux.citation_year(key),
                            ) {
                                (Some(author), Some(year)) => format!("{author}, {year}"),
                                _ => aux
                                    .bibliography_number(key)
                                    .map(|number| number.to_string())
                                    .unwrap_or_else(|| "?".to_string()),
                            }
                        })
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join("; ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    if let Some(post_note) = post_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered.push_str(", ");
                        rendered.push_str(post_note);
                    }
                    if command_name == "Citep" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let rendered = format!("({rendered})");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "cites" | "Cites" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, groups)) = read_multicite_groups(source, cursor) {
                    let rendered = groups
                        .into_iter()
                        .map(|(pre_note, post_note, keys)| {
                            let rendered = keys
                                .split(',')
                                .map(|key| key.trim())
                                .filter(|key| !key.is_empty())
                                .map(|key| {
                                    aux.bibliography_number(key)
                                        .map(|number| number.to_string())
                                        .unwrap_or_else(|| "?".to_string())
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            let mut rendered = rendered;
                            if let Some(pre_note) =
                                pre_note.as_deref().filter(|note| !note.is_empty())
                            {
                                rendered = format!("{pre_note} {rendered}");
                            }
                            if let Some(post_note) =
                                post_note.as_deref().filter(|note| !note.is_empty())
                            {
                                rendered.push_str(", ");
                                rendered.push_str(post_note);
                            }
                            rendered
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    let rendered = format!("[{rendered}]");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "supercites" | "Supercites" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, groups)) = read_multicite_groups(source, cursor) {
                    let rendered = groups
                        .into_iter()
                        .map(|(pre_note, post_note, keys)| {
                            let rendered = keys
                                .split(',')
                                .map(|key| key.trim())
                                .filter(|key| !key.is_empty())
                                .map(|key| {
                                    aux.bibliography_number(key)
                                        .map(|number| number.to_string())
                                        .unwrap_or_else(|| "?".to_string())
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            let mut rendered = rendered;
                            if let Some(pre_note) =
                                pre_note.as_deref().filter(|note| !note.is_empty())
                            {
                                rendered = format!("{pre_note} {rendered}");
                            }
                            if let Some(post_note) =
                                post_note.as_deref().filter(|note| !note.is_empty())
                            {
                                rendered.push_str(", ");
                                rendered.push_str(post_note);
                            }
                            rendered
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    let rendered = format!("^{rendered}");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "parencite" | "Parencite" | "autocite" | "Autocite" | "smartcite" | "Smartcite"
            | "footcite" | "Footcite" => {
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(
                            |key| match (aux.citation_author(key), aux.citation_year(key)) {
                                (Some(author), Some(year)) => {
                                    if let Some(post_note) =
                                        post_note.as_deref().filter(|note| !note.is_empty())
                                    {
                                        format!("{author}, {year}, {post_note}")
                                    } else {
                                        format!("{author}, {year}")
                                    }
                                }
                                _ => aux
                                    .bibliography_number(key)
                                    .map(|number| number.to_string())
                                    .unwrap_or_else(|| "?".to_string()),
                            },
                        )
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join("; ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    let rendered = format!("({rendered})");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "parencites" | "Parencites" | "autocites" | "Autocites" | "smartcites"
            | "Smartcites" | "footcites" | "Footcites" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, groups)) = read_multicite_groups(source, cursor) {
                    let rendered = groups
                        .into_iter()
                        .map(|(pre_note, post_note, keys)| {
                            let rendered = keys
                                .split(',')
                                .map(|key| key.trim())
                                .filter(|key| !key.is_empty())
                                .map(|key| {
                                    match (aux.citation_author(key), aux.citation_year(key)) {
                                        (Some(author), Some(year)) => {
                                            if let Some(post_note) =
                                                post_note.as_deref().filter(|note| !note.is_empty())
                                            {
                                                format!("{author}, {year}, {post_note}")
                                            } else {
                                                format!("{author}, {year}")
                                            }
                                        }
                                        _ => aux
                                            .bibliography_number(key)
                                            .map(|number| number.to_string())
                                            .unwrap_or_else(|| "?".to_string()),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            if let Some(pre_note) =
                                pre_note.as_deref().filter(|note| !note.is_empty())
                            {
                                format!("{pre_note} {rendered}")
                            } else {
                                rendered
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    let rendered = format!("({rendered})");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citet" | "Citet" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            match (
                                aux.citation_display_author(key, starred),
                                aux.citation_year(key),
                            ) {
                                (Some(author), Some(year)) => {
                                    if let Some(post_note) =
                                        post_note.as_deref().filter(|note| !note.is_empty())
                                    {
                                        format!("{author} ({year}, {post_note})")
                                    } else {
                                        format!("{author} ({year})")
                                    }
                                }
                                _ => aux
                                    .bibliography_number(key)
                                    .map(|number| format!("[{number}]"))
                                    .unwrap_or_else(|| "[?]".to_string()),
                            }
                        })
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join("; ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    if command_name == "Citet" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citealt" | "citealp" | "Citealt" | "Citealp" | "onlinecite" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            match (
                                aux.citation_display_author(key, starred),
                                aux.citation_year(key),
                            ) {
                                (Some(author), Some(year)) => {
                                    if let Some(post_note) =
                                        post_note.as_deref().filter(|note| !note.is_empty())
                                    {
                                        format!("{author} {year}, {post_note}")
                                    } else {
                                        format!("{author} {year}")
                                    }
                                }
                                _ => aux
                                    .bibliography_number(key)
                                    .map(|number| number.to_string())
                                    .unwrap_or_else(|| "?".to_string()),
                            }
                        })
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join("; ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    if matches!(command_name.as_str(), "Citealt" | "Citealp") {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citetext" => {
                if let Some((argument_end, text)) = read_braced_argument(source, command_end) {
                    let (nested, _) = rewrite_source(path, &text, aux, scan, block_numbers);
                    let rendered = format!("({})", nested.trim());
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "textcite" | "Textcite" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut pre_note = None;
                let mut post_note = None;
                if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
                    pre_note = Some(note.trim().to_string());
                    cursor = note_end;
                    if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                        post_note = Some(note.trim().to_string());
                        cursor = second_note_end;
                    }
                }
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            match (
                                aux.citation_display_author(key, starred),
                                aux.citation_year(key),
                            ) {
                                (Some(author), Some(year)) => {
                                    if let Some(post_note) =
                                        post_note.as_deref().filter(|note| !note.is_empty())
                                    {
                                        format!("{author} ({year}, {post_note})")
                                    } else {
                                        format!("{author} ({year})")
                                    }
                                }
                                _ => aux
                                    .bibliography_number(key)
                                    .map(|number| format!("[{number}]"))
                                    .unwrap_or_else(|| "[?]".to_string()),
                            }
                        })
                        .collect::<Vec<_>>();
                    let mut rendered = rendered.join("; ");
                    if let Some(pre_note) = pre_note.as_deref().filter(|note| !note.is_empty()) {
                        rendered = format!("{pre_note} {rendered}");
                    }
                    if command_name == "Textcite" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            rendered = first.to_uppercase().collect::<String>() + chars.as_str();
                        }
                    }
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "textcites" | "Textcites" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, groups)) = read_multicite_groups(source, cursor) {
                    let rendered = groups
                        .into_iter()
                        .map(|(pre_note, post_note, keys)| {
                            let rendered = keys
                                .split(',')
                                .map(|key| key.trim())
                                .filter(|key| !key.is_empty())
                                .map(|key| {
                                    match (aux.citation_author(key), aux.citation_year(key)) {
                                        (Some(author), Some(year)) => {
                                            if let Some(post_note) =
                                                post_note.as_deref().filter(|note| !note.is_empty())
                                            {
                                                format!("{author} ({year}, {post_note})")
                                            } else {
                                                format!("{author} ({year})")
                                            }
                                        }
                                        _ => aux
                                            .bibliography_number(key)
                                            .map(|number| format!("[{number}]"))
                                            .unwrap_or_else(|| "[?]".to_string()),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            if let Some(pre_note) =
                                pre_note.as_deref().filter(|note| !note.is_empty())
                            {
                                format!("{pre_note} {rendered}")
                            } else {
                                rendered
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    let rendered = if command_name == "Textcites" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            first.to_uppercase().collect::<String>() + chars.as_str()
                        } else {
                            rendered
                        }
                    } else {
                        rendered
                    };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "fullcite" | "footfullcite" | "bibentry" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.bibliography_text(key)
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "?".to_string())
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citenum" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.bibliography_number(key)
                                .map(|number| number.to_string())
                                .unwrap_or_else(|| "?".to_string())
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citeauthor" | "Citeauthor" | "citefullauthor" | "Citefullauthor" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            if matches!(command_name.as_str(), "citefullauthor" | "Citefullauthor")
                            {
                                aux.citation_full_author(key)
                                    .or_else(|| aux.citation_display_author(key, starred))
                                    .unwrap_or_else(|| "?".to_string())
                            } else {
                                aux.citation_display_author(key, starred)
                                    .unwrap_or_else(|| "?".to_string())
                            }
                        })
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let rendered =
                        if matches!(command_name.as_str(), "Citeauthor" | "Citefullauthor") {
                            let mut chars = rendered.chars();
                            if let Some(first) = chars.next() {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            } else {
                                rendered
                            }
                        } else {
                            rendered
                        };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citeyearpar" | "Citeyearpar" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            format!(
                                "({})",
                                aux.citation_year(key).unwrap_or_else(|| "??".to_string())
                            )
                        })
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citeyear" | "Citeyear" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| aux.citation_year(key).unwrap_or_else(|| "??".to_string()))
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let rendered = if command_name == "Citeyear" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            first.to_uppercase().collect::<String>() + chars.as_str()
                        } else {
                            rendered
                        }
                    } else {
                        rendered
                    };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citedate" | "Citedate" | "citeurldate" | "Citeurldate" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| match command_name.as_str() {
                            "citedate" | "Citedate" => aux
                                .citation_field(key, "date")
                                .map(ToOwned::to_owned)
                                .or_else(|| aux.citation_year(key))
                                .unwrap_or_else(|| "??".to_string()),
                            "citeurldate" | "Citeurldate" => aux
                                .citation_field(key, "urldate")
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "??".to_string()),
                            _ => "??".to_string(),
                        })
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let rendered = if matches!(command_name.as_str(), "Citedate" | "Citeurldate") {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            first.to_uppercase().collect::<String>() + chars.as_str()
                        } else {
                            rendered
                        }
                    } else {
                        rendered
                    };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citetitle" | "Citetitle" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.citation_title(key)
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "?".to_string())
                        })
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let rendered = if command_name == "Citetitle" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            first.to_uppercase().collect::<String>() + chars.as_str()
                        } else {
                            rendered
                        }
                    } else {
                        rendered
                    };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citeurl" | "Citeurl" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| {
                            aux.citation_url(key)
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "??".to_string())
                        })
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let rendered = if command_name == "Citeurl" {
                        let mut chars = rendered.chars();
                        if let Some(first) = chars.next() {
                            first.to_uppercase().collect::<String>() + chars.as_str()
                        } else {
                            rendered
                        }
                    } else {
                        rendered
                    };
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citedoi" | "citeeprint" | "citeisbn" | "citeissn" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    let rendered = keys
                        .split(',')
                        .map(|key| key.trim())
                        .filter(|key| !key.is_empty())
                        .map(|key| match command_name.as_str() {
                            "citedoi" => aux
                                .citation_doi(key)
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "??".to_string()),
                            "citeeprint" => aux
                                .citation_eprint(key)
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "??".to_string()),
                            "citeisbn" => aux
                                .citation_field(key, "isbn")
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "??".to_string()),
                            "citeissn" => aux
                                .citation_field(key, "issn")
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| "??".to_string()),
                            _ => "??".to_string(),
                        })
                        .collect::<Vec<_>>();
                    let rendered = rendered.join(", ");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "citefield" => {
                let cursor = skip_optional_bracket_arguments(
                    source,
                    skip_optional_command_star(source, command_end),
                );
                if let Some((key_end, key)) = read_braced_argument(source, cursor) {
                    let field_cursor =
                        if let Some((format_end, _)) = read_bracket_argument(source, key_end) {
                            format_end
                        } else {
                            key_end
                        };
                    if let Some((argument_end, field)) = read_braced_argument(source, field_cursor)
                    {
                        let field_name = field.trim();
                        let rendered = match field_name {
                            "title" => aux.citation_title(&key).map(ToOwned::to_owned),
                            "year" => aux.citation_year(&key),
                            "author" | "labelname" => aux.citation_author(&key),
                            "fullauthor" => aux.citation_full_author(&key),
                            "label" => aux
                                .bibliography
                                .iter()
                                .find(|entry| entry.key == key)
                                .and_then(|entry| entry.label.clone()),
                            "url" => aux.citation_url(&key).map(ToOwned::to_owned),
                            "doi" => aux.citation_doi(&key).map(ToOwned::to_owned),
                            "eprint" => aux.citation_eprint(&key).map(ToOwned::to_owned),
                            "text" => aux.bibliography_text(&key).map(ToOwned::to_owned),
                            _ => aux.citation_field(&key, field_name).map(ToOwned::to_owned),
                        }
                        .unwrap_or_else(|| "??".to_string());
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "nocite" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, _)) = read_braced_argument(source, cursor) {
                    let output_start_utf8 = output.len() as u32;
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output_start_utf8,
                        rendered: String::new(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "caption" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                if let Some((option_end, _)) = read_bracket_argument(source, cursor) {
                    cursor = option_end;
                }
                if let Some((argument_end, title)) = read_braced_argument(source, cursor) {
                    let rendered = aux
                        .float_captions
                        .iter()
                        .find(|caption| {
                            caption.file == path && caption.offset_utf8 == command_start as u32
                        })
                        .map(|caption| {
                            let kind = if caption.kind == "figure" {
                                "Figure"
                            } else if caption.kind == "table" {
                                "Table"
                            } else {
                                "Algorithm"
                            };
                            if caption.number.is_empty() || starred {
                                caption.body_title.clone()
                            } else {
                                format!("{kind} {}: {}", caption.number, caption.body_title)
                            }
                        })
                        .unwrap_or_else(|| title.trim().to_string());
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "captionof" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                if let Some((kind_end, _)) = read_braced_argument(source, cursor) {
                    cursor = kind_end;
                    if let Some((option_end, _)) = read_bracket_argument(source, cursor) {
                        cursor = option_end;
                    }
                    if let Some((argument_end, title)) = read_braced_argument(source, cursor) {
                        let rendered = aux
                            .float_captions
                            .iter()
                            .find(|caption| {
                                caption.file == path && caption.offset_utf8 == command_start as u32
                            })
                            .map(|caption| {
                                let kind = if caption.kind == "figure" {
                                    "Figure"
                                } else if caption.kind == "table" {
                                    "Table"
                                } else {
                                    "Algorithm"
                                };
                                if caption.number.is_empty() || starred {
                                    caption.body_title.clone()
                                } else {
                                    format!("{kind} {}: {}", caption.number, caption.body_title)
                                }
                            })
                            .unwrap_or_else(|| title.trim().to_string());
                        let output_start_utf8 = output.len() as u32;
                        output.push_str(&rendered);
                        rewrite_spans.push(MaterializedRewriteSpan {
                            start_utf8: command_start as u32,
                            end_utf8: argument_end as u32,
                            output_start_utf8,
                            output_end_utf8: output.len() as u32,
                            rendered,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "tableofcontents" => {
                let toc = render_table_of_contents(aux);
                let output_start_utf8 = output.len() as u32;
                output.push_str(&toc);
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: command_end as u32,
                    output_start_utf8,
                    output_end_utf8: output.len() as u32,
                    rendered: toc,
                });
                index = command_end;
                continue;
            }
            "listoffigures" | "listoftables" | "listofalgorithms" => {
                let rendered = if command_name == "listoffigures" {
                    render_float_list(aux, "figure", "List of Figures")
                } else if command_name == "listoftables" {
                    render_float_list(aux, "table", "List of Tables")
                } else {
                    render_float_list(aux, "algorithm", "List of Algorithms")
                };
                let output_start_utf8 = output.len() as u32;
                output.push_str(&rendered);
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: command_end as u32,
                    output_start_utf8,
                    output_end_utf8: output.len() as u32,
                    rendered,
                });
                index = command_end;
                continue;
            }
            "appendix" | "appendices" => {
                let output_start_utf8 = output.len() as u32;
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: command_end as u32,
                    output_start_utf8,
                    output_end_utf8: output_start_utf8,
                    rendered: String::new(),
                });
                index = command_end;
                continue;
            }
            "phantomsection" => {
                let output_start_utf8 = output.len() as u32;
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: command_end as u32,
                    output_start_utf8,
                    output_end_utf8: output_start_utf8,
                    rendered: String::new(),
                });
                index = command_end;
                continue;
            }
            "addcontentsline" => {
                if let Some((target_end, _)) = read_braced_argument(source, command_end) {
                    if let Some((kind_end, _)) = read_braced_argument(source, target_end) {
                        if let Some((title_end, _)) = read_braced_argument(source, kind_end) {
                            let output_start_utf8 = output.len() as u32;
                            rewrite_spans.push(MaterializedRewriteSpan {
                                start_utf8: command_start as u32,
                                end_utf8: title_end as u32,
                                output_start_utf8,
                                output_end_utf8: output_start_utf8,
                                rendered: String::new(),
                            });
                            index = title_end;
                            continue;
                        }
                    }
                }
            }
            "bibliographystyle" => {
                if let Some((argument_end, _)) = read_braced_argument(source, command_end) {
                    let output_start_utf8 = output.len() as u32;
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output_start_utf8,
                        rendered: String::new(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "bibliography" => {
                if let Some((argument_end, stems)) = read_braced_argument(source, command_end) {
                    let inputs = stems
                        .split(',')
                        .map(|stem| stem.trim())
                        .filter(|stem| !stem.is_empty())
                        .filter_map(|stem| {
                            let mut path = normalize_relative_path(Utf8Path::new(stem)).ok()?;
                            if path.extension() == Some("bib") {
                                path = path.with_extension("bbl");
                            }
                            if path.extension().is_none() {
                                path = path.with_extension("bbl");
                            }
                            if scan.files.contains_key(&path) {
                                Some(format!("\\input{{{path}}}"))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let rendered = inputs.join("\n");
                    let output_start_utf8 = output.len() as u32;
                    output.push_str(&rendered);
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output.len() as u32,
                        rendered,
                    });
                    index = argument_end;
                    continue;
                }
            }
            "addbibresource" => {
                let cursor = skip_optional_bracket_arguments(source, command_end);
                if let Some((argument_end, _)) = read_braced_argument(source, cursor) {
                    let output_start_utf8 = output.len() as u32;
                    rewrite_spans.push(MaterializedRewriteSpan {
                        start_utf8: command_start as u32,
                        end_utf8: argument_end as u32,
                        output_start_utf8,
                        output_end_utf8: output_start_utf8,
                        rendered: String::new(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "printbibliography" => {
                let (cursor, options) = if let Some((options_end, options)) =
                    read_bracket_argument(source, command_end)
                {
                    (options_end, Some(options))
                } else {
                    (command_end, None)
                };
                let rendered = scan
                    .bibliography_files
                    .iter()
                    .filter(|path| scan.files.contains_key(path.as_path()))
                    .map(|path| format!("\\input{{{path}}}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let rendered_title = if let Some(numbered) =
                    options.as_deref().and_then(printbibliography_heading)
                {
                    let title = options
                        .as_deref()
                        .and_then(printbibliography_title)
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| "Bibliography".to_string());
                    let toc_entry = aux.toc.iter().find(|entry| {
                        entry.file == path && entry.offset_utf8 == command_start as u32
                    });
                    if numbered {
                        if let Some(number) = toc_entry
                            .map(|entry| entry.number.as_str())
                            .filter(|number| !number.is_empty())
                        {
                            format!("{number} {title}")
                        } else {
                            title
                        }
                    } else {
                        title
                    }
                } else {
                    options
                        .as_deref()
                        .and_then(printbibliography_title)
                        .filter(|title| !title.is_empty())
                        .unwrap_or_default()
                };
                let rendered = if rendered_title.is_empty() {
                    rendered
                } else if rendered.is_empty() {
                    rendered_title
                } else {
                    format!("{rendered_title}\n{rendered}")
                };
                let output_start_utf8 = output.len() as u32;
                output.push_str(&rendered);
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: cursor as u32,
                    output_start_utf8,
                    output_end_utf8: output.len() as u32,
                    rendered,
                });
                index = cursor;
                continue;
            }
            "hspace" | "vspace" | "addvspace" | "hskip" | "vskip" | "kern" | "mkern"
            | "smallskip" | "medskip" | "bigskip" | "noindent" | "indent" | "newpage"
            | "clearpage" | "cleardoublepage" | "pagebreak" | "nopagebreak" | "linebreak"
            | "nolinebreak" | "vfill" | "hfill" => {
                let spacing_end = skip_layout_spacing_command(source, command_end);
                let output_start_utf8 = output.len() as u32;
                let should_keep_gap = output.chars().last().is_some_and(|ch| !ch.is_whitespace())
                    && source[spacing_end..].chars().next().is_some_and(|ch| {
                        !ch.is_whitespace()
                            && ch != '\\'
                            && ch != '.'
                            && ch != ','
                            && ch != ';'
                            && ch != ':'
                    });
                let rendered = if should_keep_gap {
                    output.push(' ');
                    " ".to_string()
                } else {
                    String::new()
                };
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: spacing_end as u32,
                    output_start_utf8,
                    output_end_utf8: output.len() as u32,
                    rendered,
                });
                index = spacing_end;
                continue;
            }
            "printbibheading" => {
                let (cursor, options) = if let Some((options_end, options)) =
                    read_bracket_argument(source, command_end)
                {
                    (options_end, Some(options))
                } else {
                    (command_end, None)
                };
                let rendered = if let Some(numbered) =
                    options.as_deref().and_then(printbibliography_heading)
                {
                    let title = options
                        .as_deref()
                        .and_then(printbibliography_title)
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| "Bibliography".to_string());
                    let toc_entry = aux.toc.iter().find(|entry| {
                        entry.file == path && entry.offset_utf8 == command_start as u32
                    });
                    if numbered {
                        if let Some(number) = toc_entry
                            .map(|entry| entry.number.as_str())
                            .filter(|number| !number.is_empty())
                        {
                            format!("{number} {title}")
                        } else {
                            title
                        }
                    } else {
                        title
                    }
                } else {
                    options
                        .as_deref()
                        .and_then(printbibliography_title)
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| "Bibliography".to_string())
                };
                let output_start_utf8 = output.len() as u32;
                output.push_str(&rendered);
                rewrite_spans.push(MaterializedRewriteSpan {
                    start_utf8: command_start as u32,
                    end_utf8: cursor as u32,
                    output_start_utf8,
                    output_end_utf8: output.len() as u32,
                    rendered,
                });
                index = cursor;
                continue;
            }
            _ => {}
        }
        output.push_str(&source[command_start..command_end]);
        index = command_end;
    }
    (output, rewrite_spans)
}

fn is_display_math_environment(environment: &str) -> bool {
    matches!(
        environment,
        "equation"
            | "equation*"
            | "displaymath"
            | "align"
            | "align*"
            | "flalign"
            | "flalign*"
            | "alignat"
            | "alignat*"
            | "gather"
            | "gather*"
            | "multline"
            | "multline*"
            | "eqnarray"
            | "eqnarray*"
    )
}

fn render_table_of_contents(aux: &SemanticAux) -> String {
    if aux.toc.is_empty() {
        return String::new();
    }
    let mut output = String::from("Contents\n");
    for entry in &aux.toc {
        let indent = "  ".repeat(entry.level.saturating_sub(1) as usize);
        if entry.number.is_empty() {
            output.push_str(&format!("{indent}{} .... {}\n", entry.title, entry.page));
        } else {
            output.push_str(&format!(
                "{indent}{} {} .... {}\n",
                entry.number, entry.title, entry.page
            ));
        }
    }
    output
}

fn render_float_list(aux: &SemanticAux, kind: &str, heading: &str) -> String {
    let entries = aux
        .float_captions
        .iter()
        .filter(|caption| caption.kind == kind && !caption.number.is_empty())
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return String::new();
    }
    let mut output = format!("{heading}\n");
    for entry in entries {
        if entry.number.is_empty() {
            output.push_str(&format!("{} .... {}\n", entry.title, entry.page));
        } else {
            output.push_str(&format!(
                "{} {} .... {}\n",
                entry.number, entry.title, entry.page
            ));
        }
    }
    output
}

fn bibliography_text_by_file(scan: &ProjectScan) -> BTreeMap<Utf8PathBuf, String> {
    let mut bibliography = BTreeMap::new();
    let mut next_number = 1usize;
    for path in &scan.bibliography_files {
        let Some(source) = scan.files.get(path) else {
            continue;
        };
        let entries = parse_bibliography_entries(path, source);
        if entries.is_empty() {
            continue;
        }
        let rendered = entries
            .iter()
            .map(|entry| {
                let rendered = format!("[{}] {}", next_number, entry.text);
                next_number += 1;
                rendered
            })
            .collect::<Vec<_>>()
            .join("\n");
        bibliography.insert(path.clone(), rendered);
    }
    bibliography
}

fn parse_bibliography_entries(path: &Utf8Path, source: &str) -> Vec<ParsedBibliographyEntry> {
    let mut entries = Vec::new();
    let mut index = 0usize;
    while let Some(command_rel) = source[index..].find("\\bibitem") {
        let command_start = index + command_rel;
        let Some((command_end, _)) = read_command_name(source, command_start) else {
            break;
        };
        let (cursor, label) =
            if let Some((label_end, label)) = read_bracket_argument(source, command_end) {
                (label_end, Some(clean_bibliography_text(&label)))
            } else {
                (command_end, None)
            };
        let Some((item_end, key)) = read_braced_argument(source, cursor) else {
            index = command_end;
            continue;
        };
        let next_start = source[item_end..]
            .find("\\bibitem")
            .map(|next| item_end + next)
            .unwrap_or(source.len());
        let raw_entry = &source[item_end..next_start];
        let mut fields = Vec::new();
        let mut field_scan_index = 0usize;
        while let Some(command_rel) = raw_entry[field_scan_index..].find('\\') {
            let command_start = field_scan_index + command_rel;
            let Some((command_end, command_name)) = read_command_name(raw_entry, command_start)
            else {
                break;
            };
            if matches!(command_name.as_str(), "bibinfo" | "bibfield")
                && let Some((field_end, field_name)) = read_braced_argument(raw_entry, command_end)
                && let Some((value_end, value)) = read_braced_argument(raw_entry, field_end)
            {
                let field_name = field_name.trim().to_string();
                let value = clean_bibliography_text(&value);
                if !field_name.is_empty() && !value.is_empty() {
                    fields.push((field_name, value));
                }
                field_scan_index = value_end;
                continue;
            }
            field_scan_index = command_end;
        }
        let text = clean_bibliography_text(raw_entry);
        entries.push(ParsedBibliographyEntry {
            key,
            text,
            label,
            title: extract_bibliography_named_field(raw_entry, "title"),
            author: extract_bibliography_named_field(raw_entry, "author"),
            year: extract_bibliography_named_field(raw_entry, "year"),
            fields,
            url: extract_bibliography_url(raw_entry),
            doi: extract_bibliography_doi(raw_entry),
            eprint: extract_bibliography_eprint(raw_entry),
            file: path.to_path_buf(),
        });
        index = next_start;
    }
    entries
}

fn extract_bibliography_url(source: &str) -> Option<String> {
    if let Some(url) = extract_bibliography_named_field(source, "url") {
        return Some(url);
    }
    let mut index = 0usize;
    while let Some(command_rel) = source[index..].find('\\') {
        let command_start = index + command_rel;
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            break;
        };
        if command_name == "href" {
            if let Some((_, url)) = read_braced_argument(source, command_end) {
                let url = url.trim().to_string();
                if !url.is_empty() {
                    return Some(url);
                }
            }
        } else if command_name == "url"
            && let Some((_, url)) = read_braced_argument(source, command_end)
        {
            let url = url.trim().to_string();
            if !url.is_empty() {
                return Some(url);
            }
        }
        index = command_end;
    }
    None
}

fn extract_bibliography_named_field(source: &str, field_name: &str) -> Option<String> {
    let mut index = 0usize;
    while let Some(command_rel) = source[index..].find('\\') {
        let command_start = index + command_rel;
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            break;
        };
        if matches!(command_name.as_str(), "bibinfo" | "bibfield")
            && let Some((field_end, field)) = read_braced_argument(source, command_end)
            && field.trim() == field_name
            && let Some((_, value)) = read_braced_argument(source, field_end)
        {
            let value = clean_bibliography_text(&value);
            if !value.is_empty() {
                return Some(value);
            }
        }
        index = command_end;
    }
    None
}

fn extract_bibliography_doi(source: &str) -> Option<String> {
    if let Some(doi) = extract_bibliography_named_field(source, "doi") {
        return Some(doi);
    }
    let mut index = 0usize;
    while let Some(command_rel) = source[index..].find('\\') {
        let command_start = index + command_rel;
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            break;
        };
        if command_name == "doi"
            && let Some((_, doi)) = read_braced_argument(source, command_end)
        {
            let doi = doi.trim().to_string();
            if !doi.is_empty() {
                return Some(doi);
            }
        }
        index = command_end;
    }
    None
}

fn extract_bibliography_eprint(source: &str) -> Option<String> {
    if let Some(eprint) = extract_bibliography_named_field(source, "eprint") {
        return Some(eprint);
    }
    let mut index = 0usize;
    while let Some(command_rel) = source[index..].find('\\') {
        let command_start = index + command_rel;
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            break;
        };
        if command_name == "eprint"
            && let Some((_, eprint)) = read_braced_argument(source, command_end)
        {
            let eprint = eprint.trim().to_string();
            if !eprint.is_empty() {
                return Some(eprint);
            }
        }
        index = command_end;
    }
    None
}

fn clean_bibliography_text(source: &str) -> String {
    let source = normalize_bibliography_inline_markup(source)
        .replace("\\newblock", " ")
        .replace("\\newunit", " ")
        .replace("\\finentry", " ")
        .replace("\\begin{thebibliography}", " ")
        .replace("\\end{thebibliography}", " ");
    let mut text = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < source.len() {
        let Some(ch) = source[index..].chars().next() else {
            break;
        };
        if ch == '\\' {
            if let Some((command_end, command_name)) = read_command_name(&source, index) {
                let starred_end = if source[command_end..].starts_with('*') {
                    command_end + 1
                } else {
                    command_end
                };
                let combining_mark = match command_name.as_str() {
                    "'" => Some('\u{0301}'),
                    "`" => Some('\u{0300}'),
                    "\"" => Some('\u{0308}'),
                    "^" => Some('\u{0302}'),
                    "~" => Some('\u{0303}'),
                    "=" => Some('\u{0304}'),
                    "." => Some('\u{0307}'),
                    "c" => Some('\u{0327}'),
                    "v" => Some('\u{030c}'),
                    "u" => Some('\u{0306}'),
                    "H" => Some('\u{030b}'),
                    "k" => Some('\u{0328}'),
                    "r" => Some('\u{030a}'),
                    "b" => Some('\u{0331}'),
                    "d" => Some('\u{0323}'),
                    _ => None,
                };
                if let Some(combining_mark) = combining_mark {
                    let argument_start =
                        if command_name.chars().next().is_some_and(char::is_alphabetic) {
                            skip_whitespace(&source, command_end)
                        } else {
                            command_end
                        };
                    let argument = if let Some((argument_end, argument)) =
                        read_braced_argument(&source, argument_start)
                    {
                        Some((argument_end, clean_bibliography_text(&argument)))
                    } else if source[argument_start..].starts_with('\\') {
                        read_command_name(&source, argument_start).and_then(
                            |(argument_end, argument_command)| {
                                matches!(argument_command.as_str(), "i" | "j")
                                    .then_some((argument_end, argument_command))
                            },
                        )
                    } else {
                        source[argument_start..].chars().next().map(|argument| {
                            (argument_start + argument.len_utf8(), argument.to_string())
                        })
                    };
                    if let Some((argument_end, argument)) = argument {
                        let mut argument_chars = argument.chars();
                        if let Some(base) = argument_chars.next() {
                            text.extend([base, combining_mark].into_iter().nfc());
                            text.extend(argument_chars);
                        }
                        index = argument_end;
                        continue;
                    }
                }
                if matches!(command_name.as_str(), "bibinfo" | "bibfield") {
                    if let Some((field_end, _)) = read_braced_argument(&source, command_end) {
                        if let Some((value_end, value)) = read_braced_argument(&source, field_end) {
                            text.push_str(&clean_bibliography_text(&value));
                            index = value_end;
                            continue;
                        }
                    }
                }
                if matches!(command_name.as_str(), "protect" | "relax" | "leavevmode") {
                    index = command_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "hspace"
                        | "vspace"
                        | "addvspace"
                        | "hskip"
                        | "vskip"
                        | "kern"
                        | "mkern"
                        | "smallskip"
                        | "medskip"
                        | "bigskip"
                        | "noindent"
                        | "indent"
                        | "newpage"
                        | "clearpage"
                        | "cleardoublepage"
                        | "pagebreak"
                        | "nopagebreak"
                        | "linebreak"
                        | "nolinebreak"
                        | "vfill"
                        | "hfill"
                ) {
                    if text.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
                        text.push(' ');
                    }
                    index = skip_layout_spacing_command(&source, command_end);
                    continue;
                }
                if command_name == "ignorespaces" {
                    index = command_end;
                    while index < source.len()
                        && source[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += source[index..]
                            .chars()
                            .next()
                            .expect("whitespace char")
                            .len_utf8();
                    }
                    continue;
                }
                if command_name == "urlprefix" {
                    index = command_end;
                    continue;
                }
                if command_name == "urlstyle"
                    && let Some((style_end, _)) = read_braced_argument(&source, command_end)
                {
                    index = style_end;
                    continue;
                }
                if command_name == "addspace" {
                    text.push(' ');
                    index = command_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "addabbrvspace"
                        | "addnbspace"
                        | "addthinspace"
                        | "addlowpenspace"
                        | "addhighpenspace"
                ) {
                    text.push(' ');
                    index = command_end;
                    continue;
                }
                if matches!(command_name.as_str(), "space" | "," | ";" | ":" | " ") {
                    text.push(' ');
                    index = command_end;
                    continue;
                }
                if command_name == "!" {
                    index = command_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "textquotesingle"
                        | "textquotedbl"
                        | "textless"
                        | "textgreater"
                        | "textbar"
                        | "slash"
                ) {
                    let rendered = match command_name.as_str() {
                        "textquotesingle" => "'",
                        "textquotedbl" => "\"",
                        "textless" => "<",
                        "textgreater" => ">",
                        "textbar" => "|",
                        "slash" => "/",
                        _ => unreachable!("matched bibliography text-symbol helper"),
                    };
                    text.push_str(rendered);
                    index = command_end;
                    while index < source.len()
                        && source[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += source[index..]
                            .chars()
                            .next()
                            .expect("whitespace char")
                            .len_utf8();
                    }
                    continue;
                }
                if command_name == "addcomma" {
                    text.push(',');
                    index = command_end;
                    continue;
                }
                if command_name == "addslash" {
                    text.push('/');
                    index = command_end;
                    while index < source.len()
                        && source[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += source[index..]
                            .chars()
                            .next()
                            .expect("whitespace char")
                            .len_utf8();
                    }
                    continue;
                }
                if command_name == "addhyphen" {
                    text.push('-');
                    index = command_end;
                    while index < source.len()
                        && source[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += source[index..]
                            .chars()
                            .next()
                            .expect("whitespace char")
                            .len_utf8();
                    }
                    continue;
                }
                if command_name == "adddot" {
                    text.push('.');
                    index = command_end;
                    continue;
                }
                if command_name == "adddotspace" {
                    if !text.chars().last().is_some_and(|ch| ch == '.') {
                        text.push('.');
                    }
                    text.push(' ');
                    index = command_end;
                    continue;
                }
                if command_name == "addcolon" {
                    text.push(':');
                    index = command_end;
                    continue;
                }
                if command_name == "addsemicolon" {
                    text.push(';');
                    index = command_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "bibopenparen" | "bibopenbracket" | "bibopenbrace"
                ) {
                    let punctuation = match command_name.as_str() {
                        "bibopenparen" => "(",
                        "bibopenbracket" => "[",
                        "bibopenbrace" => "{",
                        _ => unreachable!("matched opening punctuation helper"),
                    };
                    text.push_str(punctuation);
                    index = command_end;
                    while index < source.len()
                        && source[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += source[index..]
                            .chars()
                            .next()
                            .expect("whitespace char")
                            .len_utf8();
                    }
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "bibcloseparen" | "bibclosebracket" | "bibclosebrace"
                ) {
                    while text.chars().last().is_some_and(char::is_whitespace) {
                        text.pop();
                    }
                    let punctuation = match command_name.as_str() {
                        "bibcloseparen" => ")",
                        "bibclosebracket" => "]",
                        "bibclosebrace" => "}",
                        _ => unreachable!("matched closing punctuation helper"),
                    };
                    text.push_str(punctuation);
                    index = command_end;
                    continue;
                }
                if command_name == "bibnamedash" {
                    text.push_str("---");
                    index = command_end;
                    continue;
                }
                if matches!(command_name.as_str(), "bibrangedash" | "textendash") {
                    text.push('-');
                    index = command_end;
                    while index < source.len()
                        && source[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += source[index..]
                            .chars()
                            .next()
                            .expect("whitespace char")
                            .len_utf8();
                    }
                    continue;
                }
                if command_name == "textemdash" {
                    text.push_str("---");
                    index = command_end;
                    continue;
                }
                if command_name == "isdot" {
                    if !text
                        .chars()
                        .rev()
                        .find(|ch| !ch.is_whitespace())
                        .is_some_and(|ch| ch == '.')
                    {
                        text.push('.');
                    }
                    index = command_end;
                    continue;
                }
                if command_name == "nopunct" {
                    index = command_end;
                    continue;
                }
                if command_name == "unspace" {
                    while text.chars().last().is_some_and(char::is_whitespace) {
                        text.pop();
                    }
                    index = command_end;
                    continue;
                }
                if command_name == "unskip" {
                    while text.chars().last().is_some_and(char::is_whitespace) {
                        text.pop();
                    }
                    index = command_end;
                    continue;
                }
                if command_name == "href" {
                    if let Some((url_end, _)) = read_braced_argument(&source, command_end) {
                        if let Some((label_end, label)) = read_braced_argument(&source, url_end) {
                            text.push_str(&clean_bibliography_text(&label));
                            index = label_end;
                            continue;
                        }
                    }
                }
                if command_name == "url" {
                    if let Some((label_end, label)) = read_braced_argument(&source, command_end) {
                        text.push_str(label.trim());
                        index = label_end;
                        continue;
                    }
                }
                if matches!(command_name.as_str(), "nolinkurl" | "path" | "detokenize")
                    && let Some((label_end, label)) = read_braced_argument(&source, command_end)
                {
                    text.push_str(label.trim());
                    index = label_end;
                    continue;
                }
                if command_name == "bibstring"
                    && let Some((value_end, value)) = read_braced_argument(&source, command_end)
                {
                    let rendered = match value.trim() {
                        "andothers" => "et al",
                        "editor" => "editor",
                        "editors" => "editors",
                        "in" => "in",
                        "page" => "page",
                        "pages" => "pages",
                        "chapter" => "chapter",
                        "chapters" => "chapters",
                        "urlseen" => "accessed",
                        "available" => "available",
                        "from" => "from",
                        other => other,
                    };
                    text.push_str(rendered);
                    index = value_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "mkbibacro"
                        | "mkbibnamefamily"
                        | "mkbibnamegiven"
                        | "mkbibnameprefix"
                        | "mkbibnamesuffix"
                        | "mkbibnameaffix"
                ) && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push_str(&clean_bibliography_text(&value));
                    index = value_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "mkbibemph" | "mkbibitalic" | "mkbibbold"
                ) && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push_str(&clean_bibliography_text(&value));
                    index = value_end;
                    continue;
                }
                if matches!(
                    command_name.as_str(),
                    "emph"
                        | "mbox"
                        | "hbox"
                        | "fbox"
                        | "texttt"
                        | "textsf"
                        | "textsc"
                        | "textbf"
                        | "textit"
                        | "textrm"
                        | "textup"
                        | "textmd"
                        | "textnormal"
                ) && let Some((value_end, value)) = read_braced_argument(&source, command_end)
                {
                    text.push_str(&clean_bibliography_text(&value));
                    index = value_end;
                    continue;
                }
                if matches!(command_name.as_str(), "phantom" | "hphantom" | "vphantom")
                    && let Some((value_end, _)) = read_braced_argument(&source, command_end)
                {
                    index = value_end;
                    continue;
                }
                if command_name == "framebox" {
                    let cursor = skip_optional_bracket_arguments(&source, command_end);
                    if let Some((value_end, value)) = read_braced_argument(&source, cursor) {
                        text.push_str(&clean_bibliography_text(&value));
                        index = value_end;
                        continue;
                    }
                }
                if command_name == "raisebox"
                    && let Some((lift_end, _)) = read_braced_argument(&source, command_end)
                {
                    let cursor = skip_optional_bracket_arguments(&source, lift_end);
                    if let Some((value_end, value)) = read_braced_argument(&source, cursor) {
                        text.push_str(&clean_bibliography_text(&value));
                        index = value_end;
                        continue;
                    }
                }
                if command_name == "parbox" {
                    let cursor = skip_optional_bracket_arguments(&source, command_end);
                    if let Some((width_end, _)) = read_braced_argument(&source, cursor) {
                        let cursor = skip_optional_bracket_arguments(&source, width_end);
                        if let Some((value_end, value)) = read_braced_argument(&source, cursor) {
                            text.push_str(&clean_bibliography_text(&value));
                            index = value_end;
                            continue;
                        }
                    }
                }
                if command_name == "makebox" {
                    let cursor = skip_optional_bracket_arguments(&source, command_end);
                    if let Some((value_end, value)) = read_braced_argument(&source, cursor) {
                        text.push_str(&clean_bibliography_text(&value));
                        index = value_end;
                        continue;
                    }
                }
                if matches!(
                    command_name.as_str(),
                    "NoCaseChange" | "MakeSentenceCase" | "MakeTitleCase"
                ) {
                    if let Some((value_end, value)) = read_braced_argument(&source, starred_end) {
                        text.push_str(&clean_bibliography_text(&value));
                        index = value_end;
                        continue;
                    }
                }
                if matches!(
                    command_name.as_str(),
                    "mkbibsuperscript" | "mkbibsubscript" | "textsuperscript" | "textsubscript"
                ) && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push_str(&clean_bibliography_text(&value));
                    index = value_end;
                    continue;
                }
                if matches!(command_name.as_str(), "mkbibquote" | "enquote")
                    && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push('"');
                    text.push_str(&clean_bibliography_text(&value));
                    text.push('"');
                    index = value_end;
                    continue;
                }
                if command_name == "parentext"
                    && let Some((value_end, value)) = read_braced_argument(&source, command_end)
                {
                    if text
                        .chars()
                        .last()
                        .is_some_and(|ch| !ch.is_whitespace() && ch != '(')
                    {
                        text.push(' ');
                    }
                    text.push('(');
                    text.push_str(&clean_bibliography_text(&value));
                    text.push(')');
                    index = value_end;
                    continue;
                }
                if command_name == "mkbibparens"
                    && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push('(');
                    text.push_str(&clean_bibliography_text(&value));
                    text.push(')');
                    index = value_end;
                    continue;
                }
                if command_name == "mkbibbrackets"
                    && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push('[');
                    text.push_str(&clean_bibliography_text(&value));
                    text.push(']');
                    index = value_end;
                    continue;
                }
                if command_name == "mkbibbraces"
                    && let Some((value_end, value)) = read_braced_argument(&source, starred_end)
                {
                    text.push('{');
                    text.push_str(&clean_bibliography_text(&value));
                    text.push('}');
                    index = value_end;
                    continue;
                }
                if command_name.len() == 1 {
                    match command_name.as_str() {
                        "\\" => text.push(' '),
                        "{" | "}" => {}
                        command => text.push_str(command),
                    }
                }
                index = command_end;
                continue;
            }
        }
        if ch != '{' && ch != '}' {
            text.push(ch);
        }
        index += ch.len_utf8();
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn page_for_offset(pages: &[PageSourceSlice], file: &Utf8Path, offset_utf8: u32) -> Option<u32> {
    pages.iter().find_map(|page| {
        page.source_spans.iter().find_map(|span| {
            if span.file == file && span.start_utf8 <= offset_utf8 && span.end_utf8 >= offset_utf8 {
                Some(page.page_index as u32 + 1)
            } else {
                None
            }
        })
    })
}

fn scan_source(source: &str) -> Vec<CommandEvent> {
    let mut events = Vec::new();
    let mut index = 0usize;
    let mut environment_stack = Vec::<(String, Option<(FloatKind, bool)>)>::new();
    while index < source.len() {
        let Some(backslash_rel) = source[index..].find('\\') else {
            break;
        };
        let command_start = index + backslash_rel;
        if is_comment_start(source, command_start) {
            index = skip_comment(source, command_start);
            continue;
        }
        let Some((command_end, command_name)) = read_command_name(source, command_start) else {
            index = command_start + 1;
            continue;
        };
        match command_name.as_str() {
            "chapter" | "section" | "subsection" | "subsubsection" | "paragraph"
            | "subparagraph" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                cursor = skip_whitespace(source, cursor);
                let mut toc_title = None;
                if let Some((option_end, title)) = read_bracket_argument(source, cursor) {
                    toc_title = Some(title.trim().to_string());
                    cursor = option_end;
                }
                if let Some((argument_end, title)) = read_braced_argument(source, cursor) {
                    if !starred {
                        events.push(CommandEvent::Section {
                            offset: command_start as u32,
                            level: match command_name.as_str() {
                                "chapter" => 0,
                                "section" => 1,
                                "subsection" => 2,
                                "subsubsection" => 3,
                                "paragraph" => 4,
                                _ => 5,
                            },
                            toc_title: toc_title.unwrap_or_else(|| title.trim().to_string()),
                            body_title: title.trim().to_string(),
                            numbered: true,
                        });
                    }
                    index = argument_end;
                    continue;
                }
            }
            "label" => {
                if let Some((argument_end, key)) = read_braced_argument(source, command_end) {
                    events.push(CommandEvent::Label {
                        offset: command_start as u32,
                        key: key.trim().to_string(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "begin" => {
                if let Some((argument_end, environment)) = read_braced_argument(source, command_end)
                {
                    let environment = environment.trim();
                    if is_display_math_environment(environment) {
                        events.push(CommandEvent::Equation {
                            offset: command_start as u32,
                            numbered: environment != "displaymath" && !environment.ends_with('*'),
                        });
                    } else if environment == "figure"
                        || environment == "figure*"
                        || environment == "table"
                        || environment == "table*"
                        || environment == "algorithm"
                        || environment == "algorithm*"
                    {
                        events.push(CommandEvent::Float {
                            offset: command_start as u32,
                            kind: if environment.starts_with("figure") {
                                FloatKind::Figure
                            } else if environment.starts_with("table") {
                                FloatKind::Table
                            } else {
                                FloatKind::Algorithm
                            },
                            numbered: !environment.ends_with('*'),
                        });
                    } else if BlockKind::from_environment(environment).is_some() {
                        let title =
                            read_bracket_argument(source, skip_whitespace(source, argument_end))
                                .map(|(_, title)| title.trim().to_string())
                                .filter(|title| !title.is_empty());
                        events.push(CommandEvent::Block {
                            offset: command_start as u32,
                            environment: environment.to_string(),
                            title,
                        });
                    } else {
                        let title =
                            read_bracket_argument(source, skip_whitespace(source, argument_end))
                                .map(|(_, title)| title.trim().to_string())
                                .filter(|title| !title.is_empty());
                        events.push(CommandEvent::Block {
                            offset: command_start as u32,
                            environment: environment.to_string(),
                            title,
                        });
                    }
                    environment_stack.push((
                        environment.to_string(),
                        if environment == "figure"
                            || environment == "figure*"
                            || environment == "table"
                            || environment == "table*"
                            || environment == "algorithm"
                            || environment == "algorithm*"
                        {
                            Some((
                                if environment.starts_with("figure") {
                                    FloatKind::Figure
                                } else if environment.starts_with("table") {
                                    FloatKind::Table
                                } else {
                                    FloatKind::Algorithm
                                },
                                !environment.ends_with('*'),
                            ))
                        } else {
                            None
                        },
                    ));
                    index = argument_end;
                    continue;
                }
            }
            "end" => {
                if let Some((argument_end, environment)) = read_braced_argument(source, command_end)
                {
                    let environment = environment.trim();
                    if environment_stack
                        .last()
                        .is_some_and(|(name, _)| name == environment)
                    {
                        environment_stack.pop();
                    } else if let Some(stack_index) = environment_stack
                        .iter()
                        .rposition(|(name, _)| name == environment)
                    {
                        environment_stack.remove(stack_index);
                    }
                    index = argument_end;
                    continue;
                }
            }
            "caption" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                let mut list_title = None;
                if let Some((option_end, title)) = read_bracket_argument(source, cursor) {
                    list_title = Some(title.trim().to_string());
                    cursor = option_end;
                }
                if let Some((argument_end, title)) = read_braced_argument(source, cursor) {
                    if let Some((kind, numbered)) = environment_stack
                        .iter()
                        .rev()
                        .find_map(|(_, current_float)| *current_float)
                    {
                        let body_title = title.trim().to_string();
                        events.push(CommandEvent::Caption {
                            offset: command_start as u32,
                            kind,
                            numbered: numbered && !starred,
                            list_title: list_title.unwrap_or_else(|| body_title.clone()),
                            body_title,
                        });
                    }
                    index = argument_end;
                    continue;
                }
            }
            "captionof" => {
                let starred = skip_optional_command_star(source, command_end) != command_end;
                let mut cursor = skip_optional_command_star(source, command_end);
                if let Some((kind_end, kind)) = read_braced_argument(source, cursor) {
                    let float_kind = match kind.trim() {
                        "figure" => Some(FloatKind::Figure),
                        "table" => Some(FloatKind::Table),
                        "algorithm" => Some(FloatKind::Algorithm),
                        _ => None,
                    };
                    cursor = kind_end;
                    let mut list_title = None;
                    if let Some((option_end, title)) = read_bracket_argument(source, cursor) {
                        list_title = Some(title.trim().to_string());
                        cursor = option_end;
                    }
                    if let Some((argument_end, title)) = read_braced_argument(source, cursor) {
                        if let Some(kind) = float_kind {
                            let body_title = title.trim().to_string();
                            events.push(CommandEvent::Float {
                                offset: command_start as u32,
                                kind,
                                numbered: !starred,
                            });
                            events.push(CommandEvent::Caption {
                                offset: command_start as u32,
                                kind,
                                numbered: !starred,
                                list_title: list_title.unwrap_or_else(|| body_title.clone()),
                                body_title,
                            });
                        }
                        index = argument_end;
                        continue;
                    }
                }
            }
            "ref" | "subref" | "autoref" | "thmref" | "Thmref" | "fullref" | "Fullref" | "cref"
            | "Cref" | "crefrange" | "Crefrange" | "namecref" | "nameCref" | "lcnamecref"
            | "namecrefs" | "nameCrefs" | "lcnamecrefs" | "labelcref" | "cpageref" | "Cpageref"
            | "vpageref" | "autopageref" | "labelcpageref" | "pagerefrange" | "cpagerefrange"
            | "Cpagerefrange" | "vpagerefrange" | "vref" | "Vref" | "vrefrange" | "Vrefrange"
            | "nameref" | "titleref" | "Titleref" | "eqref" | "subeqref" | "pageref" => {
                let cursor = skip_optional_command_star(source, command_end);
                if command_name == "crefrange"
                    || command_name == "Crefrange"
                    || command_name == "pagerefrange"
                    || command_name == "cpagerefrange"
                    || command_name == "Cpagerefrange"
                    || command_name == "vpagerefrange"
                    || command_name == "vrefrange"
                    || command_name == "Vrefrange"
                {
                    if let Some((first_end, _)) = read_braced_argument(source, cursor) {
                        if let Some((argument_end, _)) = read_braced_argument(source, first_end) {
                            index = argument_end;
                            continue;
                        }
                    }
                } else if let Some((argument_end, _)) = read_braced_argument(source, cursor) {
                    index = argument_end;
                    continue;
                }
            }
            "defcitealias" => {
                if let Some((first_end, key)) = read_braced_argument(source, command_end) {
                    if let Some((argument_end, text)) = read_braced_argument(source, first_end) {
                        events.push(CommandEvent::CitationAlias {
                            offset: command_start as u32,
                            key: key.trim().to_string(),
                            text: text.trim().to_string(),
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "cite" | "citep" | "Citep" | "parencite" | "Parencite" | "autocite" | "Autocite"
            | "smartcite" | "Smartcite" | "supercite" | "Supercite" | "footcite" | "Footcite"
            | "citet" | "Citet" | "textcite" | "Textcite" | "fullcite" | "footfullcite"
            | "bibentry" | "citealt" | "citealp" | "Citealt" | "Citealp" | "onlinecite"
            | "citeauthor" | "Citeauthor" | "citefullauthor" | "Citefullauthor" | "citenum"
            | "citeyear" | "Citeyear" | "citeyearpar" | "Citeyearpar" | "citedate" | "Citedate"
            | "citeurldate" | "Citeurldate" | "citetitle" | "Citetitle" | "citeurl" | "Citeurl"
            | "citedoi" | "citeeprint" | "citeisbn" | "citeissn" | "citefield" | "citetalias"
            | "Citetalias" | "citepalias" | "Citepalias" | "nocite" => {
                let cursor = skip_optional_bracket_arguments(
                    source,
                    skip_optional_command_star(source, command_end),
                );
                if let Some((argument_end, keys)) = read_braced_argument(source, cursor) {
                    events.push(CommandEvent::Cite {
                        offset: command_start as u32,
                        keys: keys
                            .split(',')
                            .map(str::trim)
                            .filter(|key| !key.is_empty())
                            .map(ToOwned::to_owned)
                            .collect(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "cites" | "Cites" | "parencites" | "Parencites" | "autocites" | "Autocites"
            | "smartcites" | "Smartcites" | "supercites" | "Supercites" | "footcites"
            | "Footcites" | "textcites" | "Textcites" => {
                let cursor = skip_optional_command_star(source, command_end);
                if let Some((argument_end, groups)) = read_multicite_groups(source, cursor) {
                    for (_, _, keys) in groups {
                        events.push(CommandEvent::Cite {
                            offset: command_start as u32,
                            keys: keys
                                .split(',')
                                .map(str::trim)
                                .filter(|key| !key.is_empty())
                                .map(ToOwned::to_owned)
                                .collect(),
                        });
                    }
                    index = argument_end;
                    continue;
                }
            }
            "citetext" => {
                if let Some((argument_end, text)) = read_braced_argument(source, command_end) {
                    for event in scan_source(&text) {
                        match event {
                            CommandEvent::Cite { keys, .. } => events.push(CommandEvent::Cite {
                                offset: command_start as u32,
                                keys,
                            }),
                            CommandEvent::CitationAlias { key, text, .. } => {
                                events.push(CommandEvent::CitationAlias {
                                    offset: command_start as u32,
                                    key,
                                    text,
                                })
                            }
                            _ => {}
                        }
                    }
                    index = argument_end;
                    continue;
                }
            }
            "listoffigures" | "listoftables" | "listofalgorithms" => {
                events.push(CommandEvent::FloatList);
                index = command_end;
                continue;
            }
            "tableofcontents" => {
                events.push(CommandEvent::TableOfContents);
                index = command_end;
                continue;
            }
            "appendix" | "appendices" => {
                events.push(CommandEvent::Appendix {
                    offset: command_start as u32,
                });
                index = command_end;
                continue;
            }
            "phantomsection" => {
                index = command_end;
                continue;
            }
            "addcontentsline" => {
                if let Some((target_end, target)) = read_braced_argument(source, command_end) {
                    if let Some((kind_end, kind)) = read_braced_argument(source, target_end) {
                        if let Some((title_end, title)) = read_braced_argument(source, kind_end) {
                            if target.trim() == "toc" {
                                let level = match kind.trim() {
                                    "chapter" => Some(0),
                                    "section" => Some(1),
                                    "subsection" => Some(2),
                                    "subsubsection" => Some(3),
                                    "paragraph" => Some(4),
                                    "subparagraph" => Some(5),
                                    _ => None,
                                };
                                if let Some(level) = level {
                                    events.push(CommandEvent::Section {
                                        offset: command_start as u32,
                                        level,
                                        toc_title: title.trim().to_string(),
                                        body_title: title.trim().to_string(),
                                        numbered: false,
                                    });
                                }
                            }
                            index = title_end;
                            continue;
                        }
                    }
                }
            }
            "bibliography" => {
                if let Some((argument_end, stems)) = read_braced_argument(source, command_end) {
                    events.push(CommandEvent::Bibliography {
                        stems: stems
                            .split(',')
                            .map(str::trim)
                            .filter(|stem| !stem.is_empty())
                            .map(ToOwned::to_owned)
                            .collect(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "addbibresource" => {
                let cursor = skip_optional_bracket_arguments(source, command_end);
                if let Some((argument_end, resource)) = read_braced_argument(source, cursor) {
                    events.push(CommandEvent::Bibliography {
                        stems: vec![resource.trim().to_string()],
                    });
                    index = argument_end;
                    continue;
                }
            }
            "printbibliography" => {
                let (cursor, options) = if let Some((options_end, options)) =
                    read_bracket_argument(source, command_end)
                {
                    (options_end, Some(options))
                } else {
                    (command_end, None)
                };
                events.push(CommandEvent::BibliographyHeading);
                if let Some(heading) = options.as_deref().and_then(printbibliography_heading) {
                    events.push(CommandEvent::Section {
                        offset: command_start as u32,
                        level: 1,
                        toc_title: options
                            .as_deref()
                            .and_then(printbibliography_title)
                            .filter(|title| !title.is_empty())
                            .unwrap_or_else(|| "Bibliography".to_string()),
                        body_title: options
                            .as_deref()
                            .and_then(printbibliography_title)
                            .filter(|title| !title.is_empty())
                            .unwrap_or_else(|| "Bibliography".to_string()),
                        numbered: heading,
                    });
                }
                index = cursor;
                continue;
            }
            "printbibheading" => {
                let (cursor, options) = if let Some((options_end, options)) =
                    read_bracket_argument(source, command_end)
                {
                    (options_end, Some(options))
                } else {
                    (command_end, None)
                };
                events.push(CommandEvent::BibliographyHeading);
                if let Some(heading) = options.as_deref().and_then(printbibliography_heading) {
                    events.push(CommandEvent::Section {
                        offset: command_start as u32,
                        level: 1,
                        toc_title: options
                            .as_deref()
                            .and_then(printbibliography_title)
                            .filter(|title| !title.is_empty())
                            .unwrap_or_else(|| "Bibliography".to_string()),
                        body_title: options
                            .as_deref()
                            .and_then(printbibliography_title)
                            .filter(|title| !title.is_empty())
                            .unwrap_or_else(|| "Bibliography".to_string()),
                        numbered: heading,
                    });
                }
                index = cursor;
                continue;
            }
            "bibliographystyle" => {
                if let Some((argument_end, style)) = read_braced_argument(source, command_end) {
                    events.push(CommandEvent::BibliographyStyle {
                        style: style.trim().to_string(),
                    });
                    index = argument_end;
                    continue;
                }
            }
            "newtheorem" => {
                let cursor = skip_optional_command_star(source, command_end);
                let numbered = cursor == command_end;
                if let Some((environment_end, environment)) = read_braced_argument(source, cursor) {
                    let mut title_cursor = environment_end;
                    let mut shared_counter = None;
                    if let Some((shared_counter_end, shared_counter_value)) =
                        read_bracket_argument(source, title_cursor)
                    {
                        shared_counter = Some(shared_counter_value.trim().to_string());
                        title_cursor = shared_counter_end;
                    }
                    if let Some((title_end, display_name)) =
                        read_braced_argument(source, title_cursor)
                    {
                        let mut argument_end = title_end;
                        let mut within_counter = None;
                        if let Some((within_end, within_value)) =
                            read_bracket_argument(source, title_end)
                        {
                            within_counter = Some(within_value.trim().to_string());
                            argument_end = within_end;
                        }
                        events.push(CommandEvent::NewTheoremDefinition {
                            environment: environment.trim().to_string(),
                            display_name: display_name.trim().to_string(),
                            numbered,
                            shared_counter,
                            within_counter,
                        });
                        index = argument_end;
                        continue;
                    }
                }
            }
            "includeonly" => {
                if let Some((argument_end, raw_paths)) = read_braced_argument(source, command_end) {
                    let paths = raw_paths
                        .split(',')
                        .map(str::trim)
                        .filter(|path| !path.is_empty())
                        .filter_map(|path| normalize_relative_path(Utf8Path::new(path)).ok())
                        .map(|mut path| {
                            if path.extension().is_none() {
                                path = path.with_extension("tex");
                            }
                            path
                        })
                        .collect::<Vec<_>>();
                    events.push(CommandEvent::IncludeOnly { paths });
                    index = argument_end;
                    continue;
                }
            }
            "input" | "include" => {
                if let Some((argument_end, raw_path)) = read_braced_argument(source, command_end) {
                    let mut path = normalize_relative_path(Utf8Path::new(raw_path.trim())).ok();
                    if let Some(path_value) = path.as_mut() {
                        if path_value.extension().is_none() {
                            *path_value = path_value.with_extension("tex");
                        }
                    }
                    if let Some(path_value) = path {
                        events.push(CommandEvent::Input {
                            path: path_value,
                            is_include: command_name == "include",
                        });
                    }
                    index = argument_end;
                    continue;
                }
            }
            _ => {}
        }
        index = command_end;
    }
    events
}

fn is_comment_start(source: &str, index: usize) -> bool {
    let line_start = source[..index]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(0);
    source[line_start..index].contains('%')
}

fn skip_comment(source: &str, index: usize) -> usize {
    source[index..]
        .find('\n')
        .map(|next| index + next + 1)
        .unwrap_or(source.len())
}

fn read_command_name(source: &str, command_start: usize) -> Option<(usize, String)> {
    let mut chars = source[command_start..].char_indices();
    let (_, slash) = chars.next()?;
    if slash != '\\' {
        return None;
    }
    let (first_rel, first_char) = chars.next()?;
    if first_char.is_ascii_alphabetic() || first_char == '@' {
        let mut end = command_start + first_rel + first_char.len_utf8();
        let mut name = String::from(first_char);
        for (offset, ch) in chars {
            if !ch.is_ascii_alphabetic() && ch != '@' {
                return Some((command_start + offset, name));
            }
            name.push(ch);
            end = command_start + offset + ch.len_utf8();
        }
        return Some((end, name));
    }
    Some((
        command_start + first_rel + first_char.len_utf8(),
        first_char.to_string(),
    ))
}

fn read_braced_argument(source: &str, start: usize) -> Option<(usize, String)> {
    let start = skip_whitespace(source, start);
    if source.get(start..=start)? != "{" {
        return None;
    }
    let mut depth = 0i32;
    let mut end = start;
    for (offset, ch) in source[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + offset + 1;
                    return Some((end, source[start + 1..start + offset].to_string()));
                }
            }
            _ => {}
        }
    }
    Some((end, source[start + 1..].to_string()))
}

fn read_bracket_argument(source: &str, start: usize) -> Option<(usize, String)> {
    let start = skip_whitespace(source, start);
    if source.get(start..=start)? != "[" {
        return None;
    }
    let mut depth = 0i32;
    let mut end = start;
    for (offset, ch) in source[start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = start + offset + 1;
                    return Some((end, source[start + 1..start + offset].to_string()));
                }
            }
            _ => {}
        }
    }
    Some((end, source[start + 1..].to_string()))
}

fn read_multicite_groups(
    source: &str,
    start: usize,
) -> Option<(usize, Vec<(Option<String>, Option<String>, String)>)> {
    let mut cursor = skip_whitespace(source, start);
    let mut groups = Vec::new();
    loop {
        let mut pre_note = None;
        let mut post_note = None;
        if let Some((note_end, note)) = read_bracket_argument(source, cursor) {
            pre_note = Some(note.trim().to_string());
            cursor = note_end;
            if let Some((second_note_end, note)) = read_bracket_argument(source, cursor) {
                post_note = Some(note.trim().to_string());
                cursor = second_note_end;
            }
        }
        let Some((argument_end, keys)) = read_braced_argument(source, cursor) else {
            break;
        };
        groups.push((pre_note, post_note, keys));
        cursor = argument_end;
    }
    if groups.is_empty() {
        None
    } else {
        Some((cursor, groups))
    }
}

fn split_option_items(options: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut item_start = 0usize;
    for (index, ch) in options.char_indices() {
        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                items.push(options[item_start..index].trim());
                item_start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    items.push(options[item_start..].trim());
    items
}

fn named_option_value(options: &str, key: &str) -> Option<String> {
    split_option_items(options).into_iter().find_map(|item| {
        let (name, value) = item.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim();
        if value.starts_with('{') && value.ends_with('}') && value.len() >= 2 {
            return Some(value[1..value.len() - 1].trim().to_string());
        }
        Some(value.to_string())
    })
}

fn printbibliography_heading(options: &str) -> Option<bool> {
    match named_option_value(options, "heading").as_deref() {
        Some("bibintoc") => Some(false),
        Some("bibnumbered") => Some(true),
        _ => None,
    }
}

fn printbibliography_title(options: &str) -> Option<String> {
    named_option_value(options, "title")
}

fn pluralize_kind_name(name: &str) -> String {
    if name.ends_with("y")
        && !name.ends_with("ay")
        && !name.ends_with("ey")
        && !name.ends_with("iy")
        && !name.ends_with("oy")
        && !name.ends_with("uy")
    {
        format!("{}ies", &name[..name.len() - 1])
    } else {
        format!("{name}s")
    }
}

fn skip_optional_command_star(source: &str, start: usize) -> usize {
    let start = skip_whitespace(source, start);
    if source.get(start..=start) == Some("*") {
        start + 1
    } else {
        start
    }
}

fn skip_optional_bracket_arguments(source: &str, mut start: usize) -> usize {
    while let Some((argument_end, _)) = read_bracket_argument(source, start) {
        start = argument_end;
    }
    start
}

fn skip_whitespace(source: &str, mut index: usize) -> usize {
    while let Some(ch) = source[index..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn skip_layout_spacing_command(source: &str, command_end: usize) -> usize {
    let mut cursor = skip_optional_command_star(source, command_end);
    cursor = skip_optional_bracket_arguments(source, cursor);
    if let Some((argument_end, _)) = read_braced_argument(source, cursor) {
        return argument_end;
    }

    cursor = skip_whitespace(source, cursor);
    loop {
        let before = cursor;
        cursor = skip_whitespace(source, cursor);
        if cursor >= source.len() {
            return cursor;
        }
        if let Some((next_command_end, next_command_name)) = read_command_name(source, cursor) {
            if next_command_name == "relax" {
                return next_command_end;
            }
            if matches!(
                next_command_name.as_str(),
                "@plus"
                    | "@minus"
                    | "baselineskip"
                    | "columnwidth"
                    | "dimexpr"
                    | "font"
                    | "fontdimen"
                    | "hsize"
                    | "linewidth"
                    | "paperheight"
                    | "paperwidth"
                    | "parindent"
                    | "p@"
                    | "textheight"
                    | "textwidth"
                    | "z@"
            ) {
                cursor = next_command_end;
                continue;
            }
            return before;
        }

        let Some(ch) = source[cursor..].chars().next() else {
            return cursor;
        };
        if ch == '+' || ch == '-' || ch == '.' || ch.is_ascii_digit() {
            cursor += ch.len_utf8();
            while let Some(next) = source[cursor..].chars().next() {
                if next == '.' || next.is_ascii_digit() {
                    cursor += next.len_utf8();
                } else {
                    break;
                }
            }
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '@' {
            let word_start = cursor;
            cursor += ch.len_utf8();
            while let Some(next) = source[cursor..].chars().next() {
                if next.is_ascii_alphabetic() || next == '@' {
                    cursor += next.len_utf8();
                } else {
                    break;
                }
            }
            if matches!(
                &source[word_start..cursor],
                "plus"
                    | "minus"
                    | "fil"
                    | "fill"
                    | "filll"
                    | "pt"
                    | "pc"
                    | "in"
                    | "bp"
                    | "cm"
                    | "mm"
                    | "dd"
                    | "cc"
                    | "sp"
                    | "em"
                    | "ex"
                    | "mu"
                    | "truept"
                    | "truepc"
                    | "truein"
                    | "truebp"
                    | "truecm"
                    | "truemm"
                    | "truedd"
                    | "truecc"
                    | "truesp"
            ) {
                continue;
            }
            return word_start;
        }
        return before;
    }
}

fn contentsline_level(kind: &str) -> Option<u8> {
    match kind.trim() {
        "chapter" => Some(0),
        "section" => Some(1),
        "subsection" => Some(2),
        "subsubsection" => Some(3),
        "paragraph" => Some(4),
        "subparagraph" => Some(5),
        _ => None,
    }
}

fn contentsline_kind(level: u8) -> Option<&'static str> {
    match level {
        0 => Some("chapter"),
        1 => Some("section"),
        2 => Some("subsection"),
        3 => Some("subsubsection"),
        4 => Some("paragraph"),
        5 => Some("subparagraph"),
        _ => None,
    }
}

fn writefile_target_for_float_kind(kind: &str) -> Option<&'static str> {
    match kind.trim() {
        "figure" => Some("lof"),
        "table" => Some("lot"),
        "algorithm" => Some("loa"),
        _ => None,
    }
}

fn float_kind_from_writefile_target(target: &str) -> Option<&'static str> {
    match target.trim() {
        "lof" => Some("figure"),
        "lot" => Some("table"),
        "loa" => Some("algorithm"),
        _ => None,
    }
}

fn parse_contentsline_title(source: &str) -> Result<(String, String)> {
    let source = source.trim();
    if source.is_empty() {
        return Ok((String::new(), String::new()));
    }
    if let Some((command_end, command_name)) = read_command_name(source, 0) {
        if command_name == "numberline" {
            let (number_end, number) =
                read_braced_argument(source, command_end).context("missing numberline value")?;
            return Ok((
                decode_aux_text(&number)?,
                decode_aux_text(source[number_end..].trim()).context("invalid contents title")?,
            ));
        }
    }
    Ok((String::new(), decode_aux_text(source)?))
}

fn render_contentsline_title(number: &str, title: &str) -> String {
    if number.is_empty() {
        encode_aux_text(title)
    } else {
        format!(
            "\\numberline{{{}}}{}",
            encode_aux_text(number),
            encode_aux_text(title)
        )
    }
}

fn parse_writefile_contentsline(
    payload: &str,
) -> Result<(String, String, String, u32, Utf8PathBuf, u32)> {
    let Some((command_end, command_name)) = read_command_name(payload, 0) else {
        bail!("invalid writefile payload");
    };
    if command_name != "contentsline" {
        bail!("unsupported writefile payload command \\{command_name}");
    }
    let kind = read_braced_argument(payload, command_end).context("missing contentsline kind")?;
    let title = read_braced_argument(payload, kind.0).context("missing contentsline title")?;
    let page = read_braced_argument(payload, title.0).context("missing contentsline page")?;
    let file = read_braced_argument(payload, page.0).context("missing contentsline file")?;
    let offset = read_braced_argument(payload, file.0).context("missing contentsline offset")?;
    let (number, decoded_title) = parse_contentsline_title(&title.1)?;
    Ok((
        kind.1.trim().to_string(),
        number,
        decoded_title,
        page.1.parse().context("invalid contentsline page")?,
        Utf8PathBuf::from(decode_aux_text(&file.1)?),
        offset.1.parse().context("invalid contentsline offset")?,
    ))
}

fn render_writefile_contentsline(
    kind: &str,
    number: &str,
    title: &str,
    page: u32,
    file: &Utf8Path,
    offset_utf8: u32,
) -> String {
    format!(
        "\\contentsline{{{}}}{{{}}}{{{}}}{{{}}}{{{}}}",
        kind,
        render_contentsline_title(number, title),
        page,
        encode_aux_text(file.as_str()),
        offset_utf8
    )
}

fn parse_nested_braced_fields(source: &str, count: usize) -> Result<Vec<String>> {
    let mut cursor = 0usize;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let (next_cursor, value) =
            read_braced_argument(source, cursor).context("missing nested braced field")?;
        values.push(value);
        cursor = next_cursor;
    }
    if skip_whitespace(source, cursor) != source.len() {
        bail!("unexpected trailing payload in nested braced field sequence");
    }
    Ok(values)
}

fn encode_aux_text(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 2);
    for byte in text.as_bytes() {
        encoded.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        encoded.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    encoded
}

fn decode_aux_text(encoded: &str) -> Result<String> {
    if encoded.len() % 2 != 0 {
        bail!("invalid hex payload length");
    }
    let mut bytes = Vec::with_capacity(encoded.len() / 2);
    let mut index = 0usize;
    while index < encoded.len() {
        let byte = u8::from_str_radix(&encoded[index..index + 2], 16)
            .with_context(|| format!("invalid hex payload at byte {}", index / 2))?;
        bytes.push(byte);
        index += 2;
    }
    String::from_utf8(bytes).context("decoded aux payload is not utf-8")
}

#[cfg(test)]
mod tests {
    use super::{
        PageSourceSlice, SemanticAux, SourceSpan, derive_semantic_aux, derive_semantic_aux_index,
        load_semantic_aux, materialize_project, parse_bibliography_entries,
        parse_concrete_semantic_aux, render_concrete_semantic_aux, scan_project,
        serialize_concrete_semantic_aux_backdated,
        serialize_concrete_semantic_aux_backdated_with_previous, serialize_semantic_aux_backdated,
        serialize_semantic_aux_backdated_with_previous,
    };
    use camino::{Utf8Path, Utf8PathBuf};
    use std::fs;
    use tempfile::tempdir;
    use tex_render_model::{AuxView, CitationLabelForm, CitationStyleHint};

    #[test]
    fn label_parse_write_roundtrip() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}See \\ref{sec:intro}.",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 64,
                }],
            }],
        );
        let path = root.join("aux.json");
        fs::write(
            &path,
            serialize_semantic_aux_backdated(&path, &aux).expect("serialize"),
        )
        .expect("write aux");
        let loaded = load_semantic_aux(&path).expect("load");
        assert_eq!(loaded, aux);
        assert_eq!(loaded.labels[0].number, "1");
    }

    #[test]
    fn concrete_aux_roundtrip_preserves_semantic_fields() {
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 11,
            }],
            toc: vec![super::TocEntry {
                level: 2,
                number: "1.1".to_string(),
                title: "Intro & Scope".to_string(),
                page: 2,
                file: Utf8PathBuf::from("sections/intro.tex"),
                offset_utf8: 19,
            }],
            citation_keys: vec!["alpha".to_string(), "beta".to_string()],
            bibliography_inputs: vec![
                Utf8PathBuf::from("refs-a.bbl"),
                Utf8PathBuf::from("refs-b.bbl"),
            ],
            bibliography_style: Some("plainnat".to_string()),
            citation_mode: super::CitationMode::AuthorYear,
            citation_aliases: vec![super::CitationAlias {
                key: "alpha".to_string(),
                text: "Paper A".to_string(),
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 37,
            }],
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: Some("Alpha et al.(2024)Alpha, Beta".to_string()),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_titles: vec![super::BibliographyTitle {
                key: "alpha".to_string(),
                title: "Alpha Title".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_authors: vec![super::BibliographyAuthor {
                key: "alpha".to_string(),
                author: "Alpha".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_years: vec![super::BibliographyYear {
                key: "alpha".to_string(),
                year: "2024".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_fields: vec![super::BibliographyField {
                key: "alpha".to_string(),
                field: "title".to_string(),
                value: "Alpha Title".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_urls: vec![super::BibliographyUrl {
                key: "alpha".to_string(),
                url: "https://example.com/a".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_dois: vec![super::BibliographyDoi {
                key: "alpha".to_string(),
                doi: "10.1000/alpha".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            bibliography_eprints: vec![super::BibliographyEprint {
                key: "alpha".to_string(),
                eprint: "arXiv:2401.00001".to_string(),
                file: Utf8PathBuf::from("refs-a.bbl"),
            }],
            float_captions: vec![super::FloatCaption {
                kind: "figure".to_string(),
                number: "1".to_string(),
                title: "Short cap".to_string(),
                body_title: "Long caption".to_string(),
                page: 3,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 52,
            }],
        };

        let payload = render_concrete_semantic_aux(&aux).expect("render concrete aux");
        let rendered = String::from_utf8(payload.clone()).expect("utf8 payload");
        assert!(rendered.contains("\\@writefile{toc}{\\contentsline{subsection}{\\numberline{312e31}496e74726f20262053636f7065}{2}{73656374696f6e732f696e74726f2e746578}{19}}"));
        assert!(rendered.contains(
            "\\bibcite{616c706861}{416c70686120657420616c2e283230323429416c7068612c2042657461}"
        ));
        assert!(rendered.contains("\\@writefile{lof}{\\contentsline{figure}{\\numberline{31}53686f727420636170}{3}{6d61696e2e746578}{52}}"));
        let loaded = parse_concrete_semantic_aux(&payload).expect("parse concrete aux");

        assert_eq!(loaded, aux);
    }

    #[test]
    fn load_semantic_aux_falls_back_to_concrete_aux() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let path = root.join("semantic.aux");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
            bibliography_style: Some("plain".to_string()),
            citation_keys: vec!["alpha".to_string()],
            ..SemanticAux::default()
        };
        fs::write(
            &path,
            render_concrete_semantic_aux(&aux).expect("render concrete aux"),
        )
        .expect("write aux");

        let loaded = load_semantic_aux(&path).expect("load aux");
        assert_eq!(loaded, aux);
    }

    #[test]
    fn toc_equality_backdates_serialized_payload() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let path = root.join("aux.json");
        let payload = br#"{
  "labels": [],
  "toc": [
    {
      "level": 1,
      "number": "1",
      "title": "Intro",
      "page": 1,
      "file": "main.tex",
      "offset_utf8": 0
    }
  ],
  "citation_keys": [],
  "bibliography": []
}"#;
        fs::write(&path, payload).expect("write existing");
        let current = SemanticAux {
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };
        let serialized = serialize_semantic_aux_backdated(&path, &current).expect("serialize");
        assert_eq!(serialized, payload);
    }

    #[test]
    fn backdating_can_reuse_previous_revision_payload() {
        let previous = br#"{
  "labels": [],
  "toc": [
    {
      "level": 1,
      "number": "1",
      "title": "Intro",
      "page": 1,
      "file": "main.tex",
      "offset_utf8": 0
    }
  ],
  "citation_keys": [],
  "bibliography": []
}"#;
        let current = SemanticAux {
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let serialized = serialize_semantic_aux_backdated_with_previous(Some(previous), &current)
            .expect("serialize");
        assert_eq!(serialized, previous);
    }

    #[test]
    fn concrete_backdating_can_reuse_previous_revision_payload() {
        let previous = br#"% latexd semantic aux v1
\newlabel{7365633a696e74726f}{{31}{1}{6d61696e2e746578}{0}}
\citation{616c706861}
\bibdata{726566732e62626c}
\bibstyle{706c61696e}
"#;
        let current = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            citation_keys: vec!["alpha".to_string()],
            bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
            bibliography_style: Some("plain".to_string()),
            ..SemanticAux::default()
        };
        let serialized =
            serialize_concrete_semantic_aux_backdated_with_previous(Some(previous), &current)
                .expect("serialize concrete");
        assert_eq!(serialized, previous);
    }

    #[test]
    fn concrete_backdating_reads_previous_file_payload() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let path = root.join("semantic.aux");
        let current = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
            bibliography_style: Some("plain".to_string()),
            ..SemanticAux::default()
        };
        let previous =
            render_concrete_semantic_aux(&current).expect("render previous concrete aux");
        fs::write(&path, &previous).expect("write previous");

        let serialized =
            serialize_concrete_semantic_aux_backdated(&path, &current).expect("serialize");
        assert_eq!(serialized, previous);
    }

    #[test]
    fn derive_semantic_aux_index_groups_semantic_items_by_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\n\\input{sections/intro}\n\\cite{knuth}\n\\bibliography{refs}\n",
        )
        .expect("write main");
        fs::write(
            root.join("sections/intro.tex"),
            "\\section{Intro}\\label{sec:intro}\n",
        )
        .expect("write intro");
        fs::write(
            root.join("refs.bbl"),
            "\\bibitem[Knuth(1984)]{knuth}Donald Knuth. \\bibinfo{journal}{Journal of Testing}. \\bibinfo{pages}{10--20}. \\href{https://example.test/knuth}{Paper Link}. \\doi{10.1000/knuth}. \\eprint{arXiv:2401.00001}.",
        )
        .expect("write bbl");
        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read bbl"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 76,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("sections/intro.tex"),
                        start_utf8: 0,
                        end_utf8: 32,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 44,
                    },
                ],
            }],
        );
        let index = derive_semantic_aux_index(&scan, &aux);
        assert!(index.has_table_of_contents);
        assert!(!index.has_bibliography_heading);
        assert_eq!(index.label_count, 1);
        assert_eq!(index.toc_count, 1);
        assert_eq!(index.citation_key_count, 1);
        assert_eq!(index.bibliography_entry_count, 1);
        assert_eq!(index.files.len(), 3);
        let main = index
            .files
            .iter()
            .find(|file| file.path == Utf8PathBuf::from("main.tex"))
            .expect("main summary");
        assert_eq!(main.citation_keys, vec![String::from("knuth")]);
        let intro = index
            .files
            .iter()
            .find(|file| file.path == Utf8PathBuf::from("sections/intro.tex"))
            .expect("intro summary");
        assert_eq!(intro.label_keys, vec![String::from("sec:intro")]);
        assert_eq!(intro.toc[0].title, "Intro");
        let bibliography = index
            .files
            .iter()
            .find(|file| file.path == Utf8PathBuf::from("refs.bbl"))
            .expect("bibliography summary");
        assert_eq!(bibliography.bibliography_keys, vec![String::from("knuth")]);
        assert_eq!(bibliography.bibliography_entries.len(), 1);
        assert_eq!(bibliography.bibliography_entries[0].key, "knuth");
        assert_eq!(
            bibliography.bibliography_entries[0].url.as_deref(),
            Some("https://example.test/knuth")
        );
        assert_eq!(
            bibliography.bibliography_entries[0].doi.as_deref(),
            Some("10.1000/knuth")
        );
        assert_eq!(
            bibliography.bibliography_entries[0].eprint.as_deref(),
            Some("arXiv:2401.00001")
        );
        assert_eq!(
            bibliography.bibliography_entries[0].fields,
            vec![
                super::SemanticAuxNamedValue {
                    name: "journal".to_string(),
                    value: "Journal of Testing".to_string(),
                },
                super::SemanticAuxNamedValue {
                    name: "pages".to_string(),
                    value: "10--20".to_string(),
                },
            ]
        );
    }

    #[test]
    fn scan_project_resolves_case_insensitive_input_paths() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\input{localhamiltonian}",
        )
        .expect("write main");
        fs::write(
            root.join("localHamiltonian.tex"),
            "\\section{Nested}\\label{sec:nested}",
        )
        .expect("write nested");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert!(scan.files.contains_key(&Utf8PathBuf::from("main.tex")));
        assert!(
            scan.files
                .contains_key(&Utf8PathBuf::from("localHamiltonian.tex"))
        );
        assert!(scan.labels.iter().any(|label| label.file
            == Utf8PathBuf::from("localHamiltonian.tex")
            && label.key == "sec:nested"));
    }

    #[test]
    fn scan_project_accepts_legacy_non_utf8_source_bytes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            b"\\section{Legacy}\nText before\xa0after.",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.sections[0].body_title, "Legacy");
        assert!(scan.files[&Utf8PathBuf::from("main.tex")].contains('\u{fffd}'));
    }

    #[test]
    fn scan_project_uses_jobname_bbl_and_ignores_escaped_database_paths() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            r"\section{Body}\bibliography{../private,refs}",
        )
        .expect("write main");
        fs::write(root.join("main.bbl"), r"\bibitem{key} Jobname entry").expect("write main bbl");
        fs::write(root.join("refs.bbl"), r"\bibitem{wrong} Database entry")
            .expect("write database bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.bibliography_files, vec![Utf8PathBuf::from("main.bbl")]);
        assert!(scan.files[&Utf8PathBuf::from("main.bbl")].contains("Jobname entry"));
        assert!(!scan.files.contains_key(&Utf8PathBuf::from("refs.bbl")));
    }

    #[test]
    fn scan_project_continues_past_missing_runtime_support_inputs() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            r"\section{Before}\input{epsf}\section{After}",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(
            scan.sections
                .iter()
                .map(|section| section.body_title.as_str())
                .collect::<Vec<_>>(),
            vec!["Before", "After"]
        );
        assert!(!scan.files.contains_key(&Utf8PathBuf::from("epsf.tex")));
    }

    #[test]
    fn citation_key_set_equality_is_order_insensitive_on_derivation() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cite{beta,alpha} and \\cite{alpha}.",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 40,
                }],
            }],
        );
        assert_eq!(
            aux.citation_keys,
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn derive_semantic_aux_numbers_subsubsections_and_labels() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\subsection{Scope}\\subsubsection{Detail}\\label{sec:detail}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 80,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 3);
        assert_eq!(aux.toc[0].title, "Intro");
        assert_eq!(aux.toc[0].number, "1");
        assert_eq!(aux.toc[1].title, "Scope");
        assert_eq!(aux.toc[1].number, "1.1");
        assert_eq!(aux.toc[2].title, "Detail");
        assert_eq!(aux.toc[2].level, 3);
        assert_eq!(aux.toc[2].number, "1.1.1");
        assert_eq!(aux.labels.len(), 1);
        assert_eq!(aux.labels[0].key, "sec:detail");
        assert_eq!(aux.labels[0].number, "1.1.1");
    }

    #[test]
    fn derive_semantic_aux_numbers_chapters_and_nested_sections() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\chapter{Intro}\\section{Scope}\\subsection{Detail}\\label{sec:detail}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 72,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 3);
        assert_eq!(aux.toc[0].number, "1");
        assert_eq!(aux.toc[1].number, "1.1");
        assert_eq!(aux.toc[2].number, "1.1.1");
        assert_eq!(aux.labels[0].number, "1.1.1");
    }

    #[test]
    fn derive_semantic_aux_uses_optional_section_titles_for_toc() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section[Short Intro]{Long Introduction}\\label{sec:intro}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 64,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 1);
        assert_eq!(aux.toc[0].title, "Short Intro");
        assert_eq!(aux.labels[0].number, "1");
    }

    #[test]
    fn scan_project_keeps_long_title_for_nameref_even_with_optional_toc_title() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section[Short Intro]{Long Introduction}\\label{sec:intro}",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.sections.len(), 1);
        assert_eq!(scan.sections[0].toc_title, "Short Intro");
        assert_eq!(scan.sections[0].body_title, "Long Introduction");
    }

    #[test]
    fn derive_semantic_aux_preserves_bibliography_stem_order_across_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cite{beta} then \\cite{alpha}.\\bibliography{refsb,refsa}",
        )
        .expect("write main");
        fs::write(
            root.join("refsb.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write refsb");
        fs::write(
            root.join("refsa.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write refsa");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        assert_eq!(
            scan.bibliography_files,
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl")
            ]
        );

        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 64,
                }],
            }],
        );

        assert_eq!(aux.bibliography.len(), 2);
        assert_eq!(aux.bibliography[0].key, "beta");
        assert_eq!(aux.bibliography[1].key, "alpha");
    }

    #[test]
    fn derive_semantic_aux_ignores_starred_sections_for_toc_and_numbering() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section*{Prelude}\\section{Intro}\\label{sec:intro}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 64,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 1);
        assert_eq!(aux.toc[0].title, "Intro");
        assert_eq!(aux.toc[0].number, "1");
        assert_eq!(aux.labels[0].number, "1");
    }

    #[test]
    fn derive_semantic_aux_switches_to_appendix_letter_numbering() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\appendix\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 75,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 3);
        assert_eq!(aux.toc[0].number, "1");
        assert_eq!(aux.toc[1].number, "A");
        assert_eq!(aux.toc[2].number, "A.1");
        assert_eq!(aux.labels[0].number, "A.1");
    }

    #[test]
    fn derive_semantic_aux_switches_to_appendices_letter_numbering() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\appendices\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 77,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 3);
        assert_eq!(aux.toc[1].number, "A");
        assert_eq!(aux.toc[2].number, "A.1");
        assert_eq!(aux.labels[0].number, "A.1");
    }

    #[test]
    fn derive_semantic_aux_switches_chapter_appendices_to_letter_numbering() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\chapter{Intro}\\appendix\\chapter{Proofs}\\section{Lemma}\\label{sec:lemma}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 76,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 3);
        assert_eq!(aux.toc[0].number, "1");
        assert_eq!(aux.toc[1].number, "A");
        assert_eq!(aux.toc[2].number, "A.1");
        assert_eq!(aux.labels[0].number, "A.1");
    }

    #[test]
    fn derive_semantic_aux_follows_include_files_for_sections_and_labels() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("chapters")).expect("chapters dir");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\include{chapters/intro}See \\ref{sec:intro}.",
        )
        .expect("write main");
        fs::write(
            root.join("chapters/intro.tex"),
            "\\section{Intro}\\label{sec:intro}",
        )
        .expect("write intro");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 64,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("chapters/intro.tex"),
                        start_utf8: 0,
                        end_utf8: 32,
                    },
                ],
            }],
        );

        assert_eq!(aux.toc.len(), 1);
        assert_eq!(aux.toc[0].title, "Intro");
        assert_eq!(aux.labels.len(), 1);
        assert_eq!(aux.labels[0].key, "sec:intro");
        assert_eq!(aux.labels[0].number, "1");
    }

    #[test]
    fn derive_semantic_aux_honors_includeonly_for_include_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("chapters")).expect("chapters dir");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\includeonly{chapters/intro}\\include{chapters/intro}\\include{chapters/extra}",
        )
        .expect("write main");
        fs::write(
            root.join("chapters/intro.tex"),
            "\\section{Intro}\\label{sec:intro}",
        )
        .expect("write intro");
        fs::write(
            root.join("chapters/extra.tex"),
            "\\section{Extra}\\label{sec:extra}",
        )
        .expect("write extra");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 96,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("chapters/intro.tex"),
                        start_utf8: 0,
                        end_utf8: 32,
                    },
                ],
            }],
        );

        assert_eq!(aux.toc.len(), 1);
        assert_eq!(aux.toc[0].title, "Intro");
        assert_eq!(aux.labels.len(), 1);
        assert_eq!(aux.labels[0].key, "sec:intro");
    }

    #[test]
    fn derive_semantic_aux_tracks_manual_toc_entries_from_addcontentsline() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section*{Prelude}\\phantomsection\\addcontentsline{toc}{section}{Prelude}\\section{Intro}\\label{sec:intro}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 120,
                }],
            }],
        );

        assert_eq!(aux.toc.len(), 2);
        assert_eq!(aux.toc[0].title, "Prelude");
        assert_eq!(aux.toc[0].number, "");
        assert_eq!(aux.toc[1].title, "Intro");
        assert_eq!(aux.toc[1].number, "1");
        assert_eq!(aux.labels[0].number, "1");
    }

    #[test]
    fn nocite_star_expands_to_all_bibliography_keys() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\nocite{*}\\bibliography{refs}").expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{2}\\bibitem{beta} Beta entry.\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 28,
                }],
            }],
        );

        assert_eq!(
            aux.citation_keys,
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn materialize_project_strips_nocite_from_output() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "Before \\nocite{alpha} cite \\cite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");
        let aux = SemanticAux {
            citation_keys: vec!["alpha".to_string()],
            citation_aliases: Vec::new(),
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: None,
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(!main.contains("\\nocite"));
        assert!(main.contains("cite [1]."));
        assert!(main.contains("\\input{refs.bbl}"));
    }

    #[test]
    fn materialize_project_strips_layout_spacing_commands() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "Before\\vspace*{-10pt}After\\hspace{2mm}Gap.\\pagebreak[4]Next.\\smallskip Done.\\hskip 1em plus 0.5em minus 0.4em\\relax Tail.",
        )
        .expect("write main");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Before After Gap. Next. Done. Tail."));
        for hidden in [
            "\\vspace",
            "-10pt",
            "\\hspace",
            "2mm",
            "\\pagebreak",
            "[4]",
            "\\smallskip",
            "\\hskip",
            "1em",
            "minus",
            "\\relax",
        ] {
            assert!(!main.contains(hidden), "{hidden} leaked into {main:?}");
        }
    }

    #[test]
    fn bibliography_cleaner_strips_glue_spacing_commands() {
        let entries = parse_bibliography_entries(
            Utf8Path::new("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Proc.\\hskip 1em plus 0.5em minus 0.4em\\relax PMLR.\\vspace{2pt}\\newblock Done.\\end{thebibliography}",
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "Proc. PMLR. Done.");
        for hidden in ["\\hskip", "1em", "minus", "\\vspace", "2pt", "\\relax"] {
            assert!(
                !entries[0].text.contains(hidden),
                "{hidden} leaked into {:?}",
                entries[0].text
            );
        }
    }

    #[test]
    fn materialize_project_ingests_bbl_and_rewrites_bibliography_and_cites() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\section{Intro}\\label{sec:intro}See \\ref{sec:intro}, page \\pageref{sec:intro}, cite \\cite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 24,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 16,
            }],
            citation_keys: vec!["alpha".to_string()],
            bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
            bibliography_style: None,
            citation_mode: super::CitationMode::Auto,
            citation_aliases: Vec::new(),
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: None,
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_titles: Vec::new(),
            bibliography_authors: Vec::new(),
            bibliography_years: Vec::new(),
            bibliography_fields: Vec::new(),
            bibliography_urls: Vec::new(),
            bibliography_dois: Vec::new(),
            bibliography_eprints: Vec::new(),
            float_captions: Vec::new(),
        };
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .unwrap();
        assert!(main.contains("Contents"));
        assert!(main.contains("See 1, page 2, cite [1]."));
        assert!(main.contains("\\input{refs.bbl}"));
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .unwrap();
        assert_eq!(bibliography, "[1] Alpha entry.");
        assert!(
            materialized
                .tracked_inputs
                .contains(&Utf8PathBuf::from("refs.bbl"))
        );
    }

    #[test]
    fn materialize_project_supports_starred_refs_and_optional_cite_notes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}See \\ref*{sec:intro}, page \\pageref*{sec:intro}, cite \\cite[see][chap.~2]{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 3,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 16,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 3,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            citation_keys: vec!["alpha".to_string()],
            bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
            bibliography_style: None,
            citation_mode: super::CitationMode::Auto,
            citation_aliases: Vec::new(),
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: Some("Alpha 2024".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_titles: Vec::new(),
            bibliography_authors: Vec::new(),
            bibliography_years: Vec::new(),
            bibliography_fields: Vec::new(),
            bibliography_urls: Vec::new(),
            bibliography_dois: Vec::new(),
            bibliography_eprints: Vec::new(),
            float_captions: Vec::new(),
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert!(main.contains("See 1, page 3, cite [see 1, chap.~2]."));
        assert!(!main.contains("\\ref*"));
        assert!(!main.contains("\\pageref*"));
        assert!(!main.contains("\\cite[see][chap.~2]"));
        assert_eq!(bibliography, "[1] Alpha entry.");
    }

    #[test]
    fn materialize_project_supports_citeauthor_and_citeyear_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citeauthor{alpha} (\\citeyear{alpha}) and \\citeauthor*{beta} (\\citeyear*{beta}).",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Alpha (2024) and Beta and Gamma (2023)."));
        assert!(!main.contains("\\citeauthor"));
        assert!(!main.contains("\\citeyear"));
    }

    #[test]
    fn materialize_project_supports_citenum_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citenum{alpha} and \\citenum{alpha,beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta 2023".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See 1 and 1, 2."));
        assert!(!main.contains("\\citenum"));
    }

    #[test]
    fn materialize_project_supports_capitalized_citeauthor_and_citeyear_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\Citeauthor{alpha} (\\Citeyear{alpha}) and \\Citeauthor*{beta} (\\Citeyear*{beta}).",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Alpha (2024) and Beta and Gamma (2023)."));
        assert!(!main.contains("\\Citeauthor"));
        assert!(!main.contains("\\Citeyear"));
    }

    #[test]
    fn materialize_project_supports_citefullauthor_and_capitalized_citeyearpar_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citefullauthor{alpha} and \\Citefullauthor*{beta} in \\Citeyearpar{alpha}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            bibliography_authors: vec![super::BibliographyAuthor {
                key: "alpha".to_string(),
                author: "Alpha and Beta".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Alpha and Beta and Beta and Gamma in (2024)."));
        assert!(!main.contains("\\citefullauthor"));
        assert!(!main.contains("\\Citefullauthor"));
        assert!(!main.contains("\\Citeyearpar"));
    }

    #[test]
    fn materialize_project_supports_citetitle_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citetitle{alpha} and \\Citetitle{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "alpha entry title.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "beta study heading.".to_string(),
                    label: Some("Beta 2023".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See alpha entry title and Beta study heading."));
        assert!(!main.contains("\\citetitle"));
        assert!(!main.contains("\\Citetitle"));
    }

    #[test]
    fn materialize_project_supports_citefield_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citefield{alpha}{author}, \\citefield{alpha}{year}, \\citefield{alpha}{title}, and \\citefield{alpha}{label}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "alpha entry title.".to_string(),
                label: Some("Alpha 2024".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Alpha, 2024, alpha entry title, and Alpha 2024."));
        assert!(!main.contains("\\citefield"));
    }

    #[test]
    fn materialize_project_supports_citeurl_and_url_field_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citeurl{alpha} and \\citefield{alpha}{url}.\\bibliography{refs}",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: Some("Alpha 2024".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_urls: vec![super::BibliographyUrl {
                key: "alpha".to_string(),
                url: "https://example.test/paper".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See https://example.test/paper and https://example.test/paper."));
        assert!(!main.contains("\\citeurl"));
        assert!(!main.contains("\\citefield"));
    }

    #[test]
    fn materialize_project_prefers_bibfield_url_for_citeurl_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citeurl{alpha} and \\citefield{alpha}{url}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibfield{url}{https://example.test/bibfield}.\\end{thebibliography}",
        )
        .expect("write bbl");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read bbl"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 65,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 103,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert_eq!(
            aux.citation_url("alpha"),
            Some("https://example.test/bibfield")
        );
        assert!(
            main.contains("See https://example.test/bibfield and https://example.test/bibfield.")
        );
        assert!(!main.contains("\\citeurl"));
        assert!(!main.contains("\\citefield"));
    }

    #[test]
    fn materialize_project_supports_doi_and_eprint_citefield_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citefield{alpha}{doi} and \\citefield{alpha}{eprint}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: Some("Alpha 2024".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_dois: vec![super::BibliographyDoi {
                key: "alpha".to_string(),
                doi: "10.1000/example".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_eprints: vec![super::BibliographyEprint {
                key: "alpha".to_string(),
                eprint: "arXiv:2401.00001".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See 10.1000/example and arXiv:2401.00001."));
        assert!(!main.contains("\\citefield"));
    }

    #[test]
    fn materialize_project_supports_direct_identifier_citation_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citedoi{alpha}, \\citeeprint{alpha}, \\citeisbn{alpha}, and \\citeissn{alpha}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: Some("Alpha 2024".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_dois: vec![super::BibliographyDoi {
                key: "alpha".to_string(),
                doi: "10.1000/example".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_eprints: vec![super::BibliographyEprint {
                key: "alpha".to_string(),
                eprint: "arXiv:2401.00001".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_fields: vec![
                super::BibliographyField {
                    key: "alpha".to_string(),
                    field: "isbn".to_string(),
                    value: "978-1-4028-9462-6".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyField {
                    key: "alpha".to_string(),
                    field: "issn".to_string(),
                    value: "2049-3630".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(
            main.contains(
                "See 10.1000/example, arXiv:2401.00001, 978-1-4028-9462-6, and 2049-3630."
            )
        );
        assert!(!main.contains("\\citedoi"));
        assert!(!main.contains("\\citeeprint"));
        assert!(!main.contains("\\citeisbn"));
        assert!(!main.contains("\\citeissn"));
    }

    #[test]
    fn materialize_project_supports_citedate_and_citeurldate_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citedate{alpha}, \\Citedate{beta}, \\citeurldate{alpha}, and \\Citeurldate{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta 2023".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            bibliography_fields: vec![
                super::BibliographyField {
                    key: "alpha".to_string(),
                    field: "date".to_string(),
                    value: "March 2024".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyField {
                    key: "alpha".to_string(),
                    field: "urldate".to_string(),
                    value: "2024-03-01".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyField {
                    key: "beta".to_string(),
                    field: "urldate".to_string(),
                    value: "2023-08-15".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See March 2024, 2023, 2024-03-01, and 2023-08-15."));
        assert!(!main.contains("\\citedate"));
        assert!(!main.contains("\\Citedate"));
        assert!(!main.contains("\\citeurldate"));
        assert!(!main.contains("\\Citeurldate"));
    }

    #[test]
    fn materialize_project_prefers_bibinfo_metadata_for_citation_fields() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citeauthor{alpha} (\\citeyear{alpha}), \\citetitle{alpha}, \\citefield{alpha}{doi}, and \\citefield{alpha}{eprint}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Fallback bibliography text.".to_string(),
                label: None,
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_titles: vec![super::BibliographyTitle {
                key: "alpha".to_string(),
                title: "Exact Title".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_authors: vec![super::BibliographyAuthor {
                key: "alpha".to_string(),
                author: "Alpha and Beta".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_years: vec![super::BibliographyYear {
                key: "alpha".to_string(),
                year: "2024".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_dois: vec![super::BibliographyDoi {
                key: "alpha".to_string(),
                doi: "10.1000/example".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_eprints: vec![super::BibliographyEprint {
                key: "alpha".to_string(),
                eprint: "arXiv:2401.00001".to_string(),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "See Alpha and Beta (2024), Exact Title, 10.1000/example, and arXiv:2401.00001."
        ));
        assert!(!main.contains("\\citeauthor"));
        assert!(!main.contains("\\citeyear"));
        assert!(!main.contains("\\citetitle"));
        assert!(!main.contains("\\citefield"));
    }

    #[test]
    fn materialize_project_supports_generic_bibinfo_citefield_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citefield{alpha}{journal} and \\citefield{alpha}{pages}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: None,
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            bibliography_fields: vec![
                super::BibliographyField {
                    key: "alpha".to_string(),
                    field: "journal".to_string(),
                    value: "Journal of Testing".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyField {
                    key: "alpha".to_string(),
                    field: "pages".to_string(),
                    value: "10--20".to_string(),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Journal of Testing and 10--20."));
        assert!(!main.contains("\\citefield"));
    }

    #[test]
    fn derive_semantic_aux_ingests_bibfield_metadata_for_citation_fields() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\citeauthor{alpha}\\citeyear{alpha}\\citetitle{alpha}\\citefield{alpha}{journal}\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibfield{author}{Alpha and Beta}. \\bibfield{year}{2024}. \\bibfield{title}{Field Title}. \\bibfield{journal}{Journal of Fields}.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 97,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 194,
                    },
                ],
            }],
        );

        assert_eq!(
            aux.citation_author("alpha"),
            Some("Alpha and Beta".to_string())
        );
        assert_eq!(aux.citation_year("alpha"), Some("2024".to_string()));
        assert_eq!(aux.citation_title("alpha"), Some("Field Title"));
        assert_eq!(
            aux.citation_field("alpha", "journal"),
            Some("Journal of Fields")
        );
    }

    #[test]
    fn materialize_project_supports_textual_natbib_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citet{alpha} and \\citealt{beta} / \\citealp{beta} / \\onlinecite{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023 / Beta et al. 2023."
        ));
        assert!(!main.contains("\\citet"));
        assert!(!main.contains("\\citealt"));
        assert!(!main.contains("\\citealp"));
        assert!(!main.contains("\\onlinecite"));
    }

    #[test]
    fn materialize_project_supports_capitalized_textual_citation_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\Citet{alpha} and \\Citealt{beta} / \\Citealp{beta}. \\Textcite{alpha}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("beta et al. 2023".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(
            main.contains(
                "See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023. Alpha (2024)."
            )
        );
        assert!(!main.contains("\\Citet"));
        assert!(!main.contains("\\Citealt"));
        assert!(!main.contains("\\Citealp"));
        assert!(!main.contains("\\Textcite"));
    }

    #[test]
    fn materialize_project_supports_textual_natbib_notes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\citet[see][chap.~2]{alpha} and \\citealt[e.g.][pp.~1--2]{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("see Alpha (2024, chap.~2) and e.g. Beta et al. 2023, pp.~1--2."));
        assert!(!main.contains("\\citet"));
        assert!(!main.contains("\\citealt"));
    }

    #[test]
    fn materialize_project_supports_citetext_with_nested_citations() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citetext{compare \\citealp{beta} with \\citeyearpar{alpha}}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See (compare Beta et al. 2023 with (2024))."));
        assert!(!main.contains("\\citetext"));
        assert!(!main.contains("\\citealp"));
        assert!(!main.contains("\\citeyearpar"));
    }

    #[test]
    fn materialize_project_supports_starred_textual_natbib_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citet*{beta}, \\citep*{beta}, \\citealt*{beta} / \\citealp*{beta}, and \\Textcite*{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![super::BibliographyEntry {
                key: "beta".to_string(),
                text: "Beta entry.".to_string(),
                label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "See Beta and Gamma (2023), (Beta and Gamma, 2023), Beta and Gamma 2023 / Beta and Gamma 2023, and Beta and Gamma (2023)."
        ));
        assert!(!main.contains("\\citet*"));
        assert!(!main.contains("\\citep*"));
        assert!(!main.contains("\\citealt*"));
        assert!(!main.contains("\\citealp*"));
        assert!(!main.contains("\\Textcite*"));
    }

    #[test]
    fn materialize_project_supports_parenthetical_natbib_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citep[see][chap.~2]{alpha} and \\Citep{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("beta et al. 2023".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See (see Alpha, 2024, chap.~2) and (Beta et al., 2023)."));
        assert!(!main.contains("\\citep"));
        assert!(!main.contains("\\Citep"));
    }

    #[test]
    fn materialize_project_supports_biblatex_textcite_parencite_and_printbibliography() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\addbibresource{refs.bib}\\textcite{alpha} and \\parencite[see][pp.~1--2]{beta}.\\printbibliography",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write bbl");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Alpha (2024) and (see Beta et al., 2023, pp.~1--2)."));
        assert!(main.contains("\\input{refs.bbl}"));
        assert!(!main.contains("\\textcite"));
        assert!(!main.contains("\\parencite"));
        assert!(!main.contains("\\printbibliography"));
        assert!(!main.contains("\\addbibresource"));
    }

    #[test]
    fn materialize_project_numbers_split_bibliography_files_globally() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\bibliography{refs-a,refs-b}").expect("write main");
        fs::write(
            root.join("refs-a.bbl"),
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.",
        )
        .expect("write refs-a");
        fs::write(
            root.join("refs-b.bbl"),
            "\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write refs-b");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let refs_a = materialized
            .files
            .get(&Utf8PathBuf::from("refs-a.bbl"))
            .expect("materialized refs-a");
        let refs_b = materialized
            .files
            .get(&Utf8PathBuf::from("refs-b.bbl"))
            .expect("materialized refs-b");

        assert_eq!(refs_a, "[1] Alpha entry.");
        assert_eq!(refs_b, "[2] Beta entry.");
    }

    #[test]
    fn materialize_project_supports_smartcite_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\smartcite{alpha} and \\smartcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "(Alpha, 2024) and (see Alpha, 2024, chap.~2; cf. Beta et al., 2023, pp.~1--2)."
        ));
        assert!(!main.contains("\\smartcite"));
        assert!(!main.contains("\\smartcites"));
    }

    #[test]
    fn materialize_project_supports_supercite_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\supercite{alpha} and \\supercites[see]{alpha}[cf.][pp.~1--2]{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("^1 and ^see 1; cf. 2, pp.~1--2."));
        assert!(!main.contains("\\supercite"));
        assert!(!main.contains("\\supercites"));
    }

    #[test]
    fn materialize_project_supports_fullcite_and_bibentry_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha} and \\bibentry{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta 2023".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Alpha entry. and Beta entry.."));
        assert!(!main.contains("\\fullcite"));
        assert!(!main.contains("\\bibentry"));
    }

    #[test]
    fn materialize_project_supports_biblatex_multicite_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\textcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta} and \\parencites{alpha}[cf.]{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al.(2023)Beta and Gamma".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "see Alpha (2024, chap.~2); cf. Beta et al. (2023, pp.~1--2) and (Alpha, 2024; cf. Beta et al., 2023)."
        ));
        assert!(!main.contains("\\textcites"));
        assert!(!main.contains("\\parencites"));
    }

    #[test]
    fn derive_semantic_aux_tracks_printbibliography_bibintoc_heading() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\addbibresource{refs.bib}\\printbibliography[heading=bibintoc,title={References}]",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);

        assert_eq!(aux.toc.len(), 1);
        assert_eq!(aux.toc[0].title, "References");
        assert_eq!(aux.toc[0].number, "");
    }

    #[test]
    fn derive_semantic_aux_tracks_printbibheading_bibintoc_heading() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\printbibheading[heading=bibintoc,title={References}]",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);

        assert!(scan.has_bibliography_heading);
        assert_eq!(aux.toc.len(), 1);
        assert_eq!(aux.toc[0].title, "References");
        assert_eq!(aux.toc[0].number, "");
    }

    #[test]
    fn derive_semantic_aux_index_tracks_printbibheading_flag() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\printbibheading[heading=bibintoc,title={References}]",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);
        let index = derive_semantic_aux_index(&scan, &aux);

        assert!(index.has_table_of_contents);
        assert!(index.has_bibliography_heading);
        assert_eq!(index.toc_count, 1);
    }

    #[test]
    fn materialize_project_supports_printbibliography_bibnumbered_heading() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\addbibresource{refs.bib}\\printbibliography[heading=bibnumbered,title={References}]",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("2 References\n\\input{refs.bbl}"));
        assert!(!main.contains("\\printbibliography"));
    }

    #[test]
    fn materialize_project_supports_printbibheading_bibnumbered_heading() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\printbibheading[heading=bibnumbered,title={References}]",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(scan.has_bibliography_heading);
        assert!(main.contains("2 References"));
        assert!(!main.contains("\\printbibheading"));
    }

    #[test]
    fn materialize_project_supports_citeyearpar_and_year_suffixes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citeyear{alpha} and \\citeyearpar*{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some("Alpha 2024a".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some("Beta et al., 2023b".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See 2024a and (2023b)."));
        assert!(!main.contains("\\citeyear"));
        assert!(!main.contains("\\citeyearpar"));
    }

    #[test]
    fn materialize_project_supports_citeyear_suffixes_from_natexlab_markup() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\citeyear{alpha} and \\citeyearpar{beta}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            bibliography: vec![
                super::BibliographyEntry {
                    key: "alpha".to_string(),
                    text: "Alpha entry.".to_string(),
                    label: Some(r"Alpha 2024\natexlab{a}".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
                super::BibliographyEntry {
                    key: "beta".to_string(),
                    text: "Beta entry.".to_string(),
                    label: Some(r"Beta et al., 2023\NAT@exlab{b}".to_string()),
                    file: Utf8PathBuf::from("refs.bbl"),
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See 2024a and (2023b)."));
        assert!(!main.contains("\\natexlab"));
        assert!(!main.contains("\\NAT@exlab"));
    }

    #[test]
    fn derive_semantic_aux_tracks_citation_aliases() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\defcitealias{alpha}{Paper I}\\citetalias{alpha}\\citepalias{alpha}",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);

        assert_eq!(aux.citation_keys, vec!["alpha".to_string()]);
        assert_eq!(aux.citation_alias_text("alpha"), Some("Paper I"));
    }

    #[test]
    fn numeric_natbib_mode_uses_bibliography_positions_for_author_year_labels() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            r"\usepackage[numbers]{natbib}\begin{document}\citep{bengio}\citet{bengio}\end{document}",
        )
        .expect("write main");
        fs::write(
            root.join("main.bbl"),
            r"\begin{thebibliography}{1}\bibitem[Bengio(2009)Bengio]{bengio} Yoshua Bengio. Learning deep architectures.\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);

        assert_eq!(aux.citation_mode, super::CitationMode::Numeric);
        assert_eq!(
            aux.citation_label("bengio", CitationStyleHint::Parenthetical),
            Some(tex_render_model::CitationLabel {
                text: "1".to_string(),
                form: CitationLabelForm::Numeric,
            })
        );
        assert_eq!(
            aux.citation_label("bengio", CitationStyleHint::Textual),
            Some(tex_render_model::CitationLabel {
                text: "Bengio [1]".to_string(),
                form: CitationLabelForm::Textual,
            })
        );
    }

    #[test]
    fn author_year_natbib_mode_preserves_textual_and_parenthetical_forms() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            r"\usepackage[authoryear]{natbib}\begin{document}\citep{bengio}\citet{bengio}\end{document}",
        )
        .expect("write main");
        fs::write(
            root.join("main.bbl"),
            r"\begin{thebibliography}{1}\bibitem[Bengio(2009)Bengio]{bengio} Yoshua Bengio. Learning deep architectures.\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(&scan, &[]);

        assert_eq!(aux.citation_mode, super::CitationMode::AuthorYear);
        assert_eq!(
            aux.citation_label("bengio", CitationStyleHint::Parenthetical),
            Some(tex_render_model::CitationLabel {
                text: "Bengio, 2009".to_string(),
                form: CitationLabelForm::Parenthetical,
            })
        );
        assert_eq!(
            aux.citation_label("bengio", CitationStyleHint::Textual),
            Some(tex_render_model::CitationLabel {
                text: "Bengio (2009)".to_string(),
                form: CitationLabelForm::Textual,
            })
        );
    }

    #[test]
    fn materialize_project_supports_citation_alias_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\defcitealias{alpha}{Paper I}See \\citetalias{alpha}, \\citepalias{alpha}, and \\Citetalias{alpha}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            citation_aliases: vec![super::CitationAlias {
                key: "alpha".to_string(),
                text: "Paper I".to_string(),
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Paper I, (Paper I), and Paper I."));
        assert!(!main.contains("\\citetalias"));
        assert!(!main.contains("\\citepalias"));
        assert!(!main.contains("\\Citetalias"));
    }

    #[test]
    fn materialize_project_redacts_unresolved_citation_keys() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            concat!(
                "See \\cite{missing}, \\citep{missing,other}, \\citet{missing}, ",
                "\\parencite{missing}, \\citetalias{alias}, \\citepalias{alias}, ",
                "\\fullcite{missing}, \\citeauthor{missing}, \\citetitle{missing}, ",
                "and \\citenum{missing}."
            ),
        )
        .expect("write main");
        let aux = SemanticAux::default();

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        for leaked in ["missing", "other", "alias"] {
            assert!(
                !main.contains(leaked),
                "{leaked} leaked into materialized source: {main:?}"
            );
        }
        assert!(
            main.matches('?').count() >= 10,
            "expected citation placeholders in materialized source: {main:?}"
        );
    }

    #[test]
    fn scan_project_tracks_starred_refs_and_optional_cite_notes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}\\ref*{sec:intro}\\cite[see]{beta,alpha}\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{2}\\bibitem[Beta 2024]{beta} Beta entry.\\bibitem[Alpha 2024]{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.labels.len(), 1);
        assert_eq!(scan.labels[0].key, "sec:intro");
        assert_eq!(
            scan.citations[0].keys,
            vec!["beta".to_string(), "alpha".to_string()]
        );
        assert_eq!(scan.bibliography_files, vec![Utf8PathBuf::from("refs.bbl")]);
    }

    #[test]
    fn scan_project_tracks_citeauthor_and_citeyear_keys() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\citeauthor{alpha}\\citeyear{alpha}\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.citations.len(), 2);
        assert_eq!(scan.citations[0].keys, vec!["alpha".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["alpha".to_string()]);
    }

    #[test]
    fn scan_project_tracks_citefullauthor_and_capitalized_citeyearpar_keys() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\citefullauthor{alpha}\\Citefullauthor*{beta}\\Citeyearpar{alpha}\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.citations.len(), 3);
        assert_eq!(scan.citations[0].keys, vec!["alpha".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["beta".to_string()]);
        assert_eq!(scan.citations[2].keys, vec!["alpha".to_string()]);
    }

    #[test]
    fn scan_project_tracks_addbibresource_and_biblatex_citations() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\addbibresource{refs.bib}\\textcite{alpha}\\parencite[see]{beta}\\printbibliography",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta 2023]{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.bibliography_files, vec![Utf8PathBuf::from("refs.bbl")]);
        assert_eq!(scan.citations.len(), 2);
        assert_eq!(scan.citations[0].keys, vec!["alpha".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["beta".to_string()]);
    }

    #[test]
    fn scan_project_tracks_fullcite_and_bibentry_keys() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\fullcite{alpha}\\bibentry{beta}").expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.citations.len(), 2);
        assert_eq!(scan.citations[0].keys, vec!["alpha".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["beta".to_string()]);
    }

    #[test]
    fn scan_project_tracks_citation_alias_keys_and_definitions() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\defcitealias{alpha}{Paper I}\\citetalias{alpha}\\citepalias{beta}",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.citation_aliases.len(), 1);
        assert_eq!(scan.citation_aliases[0].key, "alpha");
        assert_eq!(scan.citation_aliases[0].text, "Paper I");
        assert_eq!(scan.citations.len(), 2);
        assert_eq!(scan.citations[0].keys, vec!["alpha".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["beta".to_string()]);
    }

    #[test]
    fn scan_project_tracks_nested_citations_inside_citetext() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\citetext{compare \\citealp{beta} with \\citeyearpar{alpha}}\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta 2023]{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.citations.len(), 2);
        assert_eq!(scan.citations[0].keys, vec!["beta".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["alpha".to_string()]);
    }

    #[test]
    fn scan_project_tracks_biblatex_multicite_keys() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\textcites[see]{alpha}[cf.]{beta}\\parencites{gamma}[pp.~1]{delta}",
        )
        .expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.citations.len(), 4);
        assert_eq!(scan.citations[0].keys, vec!["alpha".to_string()]);
        assert_eq!(scan.citations[1].keys, vec!["beta".to_string()]);
        assert_eq!(scan.citations[2].keys, vec!["gamma".to_string()]);
        assert_eq!(scan.citations[3].keys, vec!["delta".to_string()]);
    }

    #[test]
    fn scan_project_tracks_printbibliography_heading_as_toc_section() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\addbibresource{refs.bib}\\printbibliography[heading=bibintoc,title={References}]",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");

        assert_eq!(scan.sections.len(), 1);
        assert_eq!(scan.sections[0].toc_title, "References");
        assert!(!scan.sections[0].numbered);
    }

    #[test]
    fn materialized_bibliography_preserves_braced_text_and_strips_formatting_commands() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\bibliography{refs}").expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha {Entry} \\emph{Title} and \\& extras.\\end{thebibliography}",
        )
        .expect("write bbl");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert_eq!(bibliography, "[1] Alpha Entry Title and & extras.");
    }

    #[test]
    fn materialized_bibliography_prefers_href_label_over_raw_href_target() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\bibliography{refs}").expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha \\href{https://example.test/paper}{Paper Link}.\\end{thebibliography}",
        )
        .expect("write bbl");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert_eq!(bibliography, "[1] Alpha Paper Link.");
    }

    #[test]
    fn materialized_bibliography_strips_urlprefix_and_renders_bibnamedash() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibnamedash. \\urlprefix\\url{https://example.test/paper}.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 34,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 128,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert!(main.contains("---. https://example.test/paper."));
        assert_eq!(bibliography, "[1] ---. https://example.test/paper.");
        assert!(!main.contains("\\urlprefix"));
        assert!(!main.contains("\\bibnamedash"));
    }

    #[test]
    fn materialized_bibliography_composes_tex_accent_control_symbols() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\bibliography{refs}").expect("write main");
        fs::write(
            root.join("refs.bbl"),
            r#"\begin{thebibliography}{1}\bibitem{alpha}P{\'e}rez, Szepesv\'ari, Universit{\"a}t M{\"u}nchen, Fran\c{c}ois, Dvo\v{r}ak, M\'{\i}ra.\end{thebibliography}"#,
        )
        .expect("write bbl");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert_eq!(
            bibliography,
            "[1] P\u{00e9}rez, Szepesv\u{00e1}ri, Universit\u{00e4}t M\u{00fc}nchen, Fran\u{00e7}ois, Dvo\u{0159}ak, M\u{00ed}ra."
        );
    }

    #[test]
    fn materialized_bibliography_strips_common_biblatex_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibquote{Alpha Title}. \\mkbibparens{2024}. \\mkbibbrackets{note}. \\mkbibemph{Emph}. \\mkbibbold{Bold}. \\mkbibitalic{Italic}. \\enquote{Nested}.\\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 34,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 228,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert_eq!(
            bibliography,
            "[1] \"Alpha Title\". (2024). [note]. Emph. Bold. Italic. \"Nested\"."
        );
        assert!(!bibliography.contains("\\mkbibquote"));
        assert!(!bibliography.contains("\\mkbibparens"));
        assert!(!bibliography.contains("\\mkbibbrackets"));
        assert!(!bibliography.contains("\\mkbibemph"));
        assert!(!bibliography.contains("\\mkbibbold"));
        assert!(!bibliography.contains("\\mkbibitalic"));
        assert!(!bibliography.contains("\\enquote"));
    }

    #[test]
    fn materialized_bibliography_strips_newunit_finentry_and_renders_addpunct_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\addcomma\\addspace Beta\\newunit Gamma\\addcolon\\addspace Delta\\addsemicolon\\addspace Epsilon\\adddot\\finentry\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 177,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("Alpha, Beta Gamma: Delta; Epsilon."));
        assert!(!main.contains("\\addcomma"));
        assert!(!main.contains("\\addspace"));
        assert!(!main.contains("\\addcolon"));
        assert!(!main.contains("\\addsemicolon"));
        assert!(!main.contains("\\adddot"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(bibliography, "[1] Alpha, Beta Gamma: Delta; Epsilon.");
        assert!(!bibliography.contains("\\newunit"));
        assert!(!bibliography.contains("\\finentry"));
    }

    #[test]
    fn materialized_bibliography_supports_bibstring_and_mkbibacro_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibnamefamily{Alpha} \\bibstring{andothers}. \\mkbibacro{URL}: \\url{https://example.test/paper}.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 155,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("Alpha et al. URL: https://example.test/paper."));
        assert!(!main.contains("\\bibstring"));
        assert!(!main.contains("\\mkbibacro"));
        assert!(!main.contains("\\mkbibnamefamily"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] Alpha et al. URL: https://example.test/paper."
        );
        assert!(!bibliography.contains("\\bibstring"));
        assert!(!bibliography.contains("\\mkbibacro"));
        assert!(!bibliography.contains("\\mkbibnamefamily"));
    }

    #[test]
    fn materialized_bibliography_supports_parentext_and_spacing_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\addabbrvspace Beta\\addnbspace Gamma\\addthinspace Delta\\addlowpenspace Epsilon\\addhighpenspace Zeta\\parentext{Supplement}.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 200,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("Alpha Beta Gamma Delta Epsilon Zeta (Supplement)."));
        assert!(!main.contains("\\addabbrvspace"));
        assert!(!main.contains("\\addnbspace"));
        assert!(!main.contains("\\addthinspace"));
        assert!(!main.contains("\\addlowpenspace"));
        assert!(!main.contains("\\addhighpenspace"));
        assert!(!main.contains("\\parentext"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] Alpha Beta Gamma Delta Epsilon Zeta (Supplement)."
        );
        assert!(!bibliography.contains("\\parentext"));
    }

    #[test]
    fn materialized_bibliography_supports_dash_and_slash_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}Pages 10\\bibrangedash20\\addcomma\\addspace Vol\\adddot 2\\addslash Issue 3\\addhyphen4\\textendash5\\textemdash appendix.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 190,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(
            main.contains("Pages 10-20, Vol. 2/Issue 3-4-5--- appendix."),
            "materialized main: {main}"
        );
        assert!(!main.contains("\\bibrangedash"));
        assert!(!main.contains("\\addslash"));
        assert!(!main.contains("\\addhyphen"));
        assert!(!main.contains("\\textendash"));
        assert!(!main.contains("\\textemdash"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] Pages 10-20, Vol. 2/Issue 3-4-5--- appendix."
        );
    }

    #[test]
    fn materialized_bibliography_supports_low_level_punctuation_helpers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\adddotspace Beta\\unspace\\isdot\\nopunct Gamma\\isdot \\bibopenparen Delta\\bibcloseparen \\bibopenbracket Epsilon\\bibclosebracket \\bibopenbrace Zeta\\bibclosebrace\\end{thebibliography}";
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), refs).expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: refs.len().try_into().expect("refs length fits in u32"),
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("Alpha. Beta. Gamma. (Delta) [Epsilon] {Zeta}"));
        assert!(!main.contains("\\adddotspace"));
        assert!(!main.contains("\\unspace"));
        assert!(!main.contains("\\isdot"));
        assert!(!main.contains("\\nopunct"));
        assert!(!main.contains("\\bibopenparen"));
        assert!(!main.contains("\\bibcloseparen"));
        assert!(!main.contains("\\bibopenbracket"));
        assert!(!main.contains("\\bibclosebracket"));
        assert!(!main.contains("\\bibopenbrace"));
        assert!(!main.contains("\\bibclosebrace"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] Alpha. Beta. Gamma. (Delta) [Epsilon] {Zeta}"
        );
        assert!(!bibliography.contains("\\adddotspace"));
        assert!(!bibliography.contains("\\unspace"));
        assert!(!bibliography.contains("\\isdot"));
        assert!(!bibliography.contains("\\nopunct"));
        assert!(!bibliography.contains("\\bibopenparen"));
        assert!(!bibliography.contains("\\bibcloseparen"));
        assert!(!bibliography.contains("\\bibopenbracket"));
        assert!(!bibliography.contains("\\bibclosebracket"));
        assert!(!bibliography.contains("\\bibopenbrace"));
        assert!(!bibliography.contains("\\bibclosebrace"));
    }

    #[test]
    fn materialized_bibliography_supports_superscript_subscript_and_braces_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}Edition\\mkbibsuperscript{2}\\mkbibsubscript{a} \\mkbibbraces{Supplement}.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 136,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("Edition2a {Supplement}."));
        assert!(!main.contains("\\mkbibsuperscript"));
        assert!(!main.contains("\\mkbibsubscript"));
        assert!(!main.contains("\\mkbibbraces"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(bibliography, "[1] Edition2a {Supplement}.");
        assert!(!bibliography.contains("\\mkbibsuperscript"));
        assert!(!bibliography.contains("\\mkbibsubscript"));
        assert!(!bibliography.contains("\\mkbibbraces"));
    }

    #[test]
    fn materialized_bibliography_supports_nolinkurl_path_and_detokenize_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}Source: \\nolinkurl{https://example.test/paper} at \\path{/tmp/archive} via \\detokenize{arXiv:2401.01234}.\\end{thebibliography}";
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), refs).expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: refs.len().try_into().expect("refs length fits in u32"),
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(
            main.contains(
                "Source: https://example.test/paper at /tmp/archive via arXiv:2401.01234."
            )
        );
        assert!(!main.contains("\\nolinkurl"));
        assert!(!main.contains("\\path"));
        assert!(!main.contains("\\detokenize"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] Source: https://example.test/paper at /tmp/archive via arXiv:2401.01234."
        );
        assert!(!bibliography.contains("\\nolinkurl"));
        assert!(!bibliography.contains("\\path"));
        assert!(!bibliography.contains("\\detokenize"));
    }

    #[test]
    fn materialized_bibliography_supports_case_textstyle_and_textsuper_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}\\NoCaseChange{NASA}. \\MakeSentenceCase{alpha title}. \\MakeTitleCase{beta title}. \\protect\\relax\\leavevmode\\ignorespaces   \\emph{Emph}. Trimmed \\unskip. \\phantom{Ghost}\\hphantom{Wide}\\vphantom{Tall}Visible. Tight\\!Join. Soft\\,Gap. Wide\\;Gap. Colon\\:Gap. Named\\space Gap. Backslash\\ Gap. Quote\\textquotesingle s. Double\\textquotedbl q. Angles\\textless x\\textgreater. Pipe\\textbar join. Path\\slash name. \\mbox{Stable}. \\hbox{Fixed}. \\fbox{Framed}. \\framebox[2em][c]{Wide}. \\raisebox{0.5ex}[1ex][0ex]{Raised}. \\parbox[t]{4em}{Paragraph}. \\makebox[3em][l]{Inline}. \\texttt{Code}. \\textsf{Sans}. \\textsc{Caps}. \\textbf{Bold}. \\textit{Italic}. \\textrm{Roman}. \\textup{Upright}. \\textmd{Medium}. \\textnormal{Normal}. Edition\\textsuperscript{2}\\textsubscript{a}.\\end{thebibliography}";
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), refs).expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: refs.len().try_into().expect("refs length fits in u32"),
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("NASA. alpha title. beta title. Emph. Trimmed. Visible. TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap. Quote's. Double\"q. Angles<x>. Pipe|join. Path/name. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a."));
        assert!(!main.contains("\\NoCaseChange"));
        assert!(!main.contains("\\MakeSentenceCase"));
        assert!(!main.contains("\\MakeTitleCase"));
        assert!(!main.contains("\\protect"));
        assert!(!main.contains("\\relax"));
        assert!(!main.contains("\\leavevmode"));
        assert!(!main.contains("\\ignorespaces"));
        assert!(!main.contains("\\unskip"));
        assert!(!main.contains("\\emph"));
        assert!(!main.contains("\\mbox"));
        assert!(!main.contains("\\hbox"));
        assert!(!main.contains("\\fbox"));
        assert!(!main.contains("\\framebox"));
        assert!(!main.contains("\\raisebox"));
        assert!(!main.contains("\\parbox"));
        assert!(!main.contains("\\makebox"));
        assert!(!main.contains("\\phantom"));
        assert!(!main.contains("\\hphantom"));
        assert!(!main.contains("\\vphantom"));
        assert!(!main.contains("\\!"));
        assert!(!main.contains("\\,"));
        assert!(!main.contains("\\;"));
        assert!(!main.contains("\\:"));
        assert!(!main.contains("\\space"));
        assert!(!main.contains("\\ Gap"));
        assert!(!main.contains("\\textquotesingle"));
        assert!(!main.contains("\\textquotedbl"));
        assert!(!main.contains("\\textless"));
        assert!(!main.contains("\\textgreater"));
        assert!(!main.contains("\\textbar"));
        assert!(!main.contains("\\slash"));
        assert!(!main.contains("\\texttt"));
        assert!(!main.contains("\\textsf"));
        assert!(!main.contains("\\textsc"));
        assert!(!main.contains("\\textbf"));
        assert!(!main.contains("\\textit"));
        assert!(!main.contains("\\textrm"));
        assert!(!main.contains("\\textup"));
        assert!(!main.contains("\\textmd"));
        assert!(!main.contains("\\textnormal"));
        assert!(!main.contains("\\textsuperscript"));
        assert!(!main.contains("\\textsubscript"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] NASA. alpha title. beta title. Emph. Trimmed. Visible. TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap. Quote's. Double\"q. Angles<x>. Pipe|join. Path/name. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a."
        );
        assert!(!bibliography.contains("\\NoCaseChange"));
        assert!(!bibliography.contains("\\MakeSentenceCase"));
        assert!(!bibliography.contains("\\MakeTitleCase"));
        assert!(!bibliography.contains("\\protect"));
        assert!(!bibliography.contains("\\relax"));
        assert!(!bibliography.contains("\\leavevmode"));
        assert!(!bibliography.contains("\\ignorespaces"));
        assert!(!bibliography.contains("\\unskip"));
        assert!(!bibliography.contains("\\emph"));
        assert!(!bibliography.contains("\\mbox"));
        assert!(!bibliography.contains("\\hbox"));
        assert!(!bibliography.contains("\\fbox"));
        assert!(!bibliography.contains("\\framebox"));
        assert!(!bibliography.contains("\\raisebox"));
        assert!(!bibliography.contains("\\parbox"));
        assert!(!bibliography.contains("\\makebox"));
        assert!(!bibliography.contains("\\phantom"));
        assert!(!bibliography.contains("\\hphantom"));
        assert!(!bibliography.contains("\\vphantom"));
        assert!(!bibliography.contains("\\!"));
        assert!(!bibliography.contains("\\,"));
        assert!(!bibliography.contains("\\;"));
        assert!(!bibliography.contains("\\:"));
        assert!(!bibliography.contains("\\space"));
        assert!(!bibliography.contains("\\ Gap"));
        assert!(!bibliography.contains("\\textquotesingle"));
        assert!(!bibliography.contains("\\textquotedbl"));
        assert!(!bibliography.contains("\\textless"));
        assert!(!bibliography.contains("\\textgreater"));
        assert!(!bibliography.contains("\\textbar"));
        assert!(!bibliography.contains("\\slash"));
        assert!(!bibliography.contains("\\texttt"));
        assert!(!bibliography.contains("\\textsf"));
        assert!(!bibliography.contains("\\textsc"));
        assert!(!bibliography.contains("\\textbf"));
        assert!(!bibliography.contains("\\textit"));
        assert!(!bibliography.contains("\\textrm"));
        assert!(!bibliography.contains("\\textup"));
        assert!(!bibliography.contains("\\textmd"));
        assert!(!bibliography.contains("\\textnormal"));
        assert!(!bibliography.contains("\\textsuperscript"));
        assert!(!bibliography.contains("\\textsubscript"));
    }

    #[test]
    fn materialized_bibliography_strips_urlstyle_wrapper() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\urlstyle{same}\\url{https://example.test/paper}.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 112,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("https://example.test/paper."));
        assert!(!main.contains("\\urlstyle"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(bibliography, "[1] https://example.test/paper.");
        assert!(!bibliography.contains("\\urlstyle"));
    }

    #[test]
    fn materialized_bibliography_supports_starred_formatting_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibquote*{Alpha Title}. \\mkbibparens*{2024}. \\mkbibbrackets*{note}. \\mkbibbraces*{Supplement}. \\mkbibemph*{Emph}. \\mkbibbold*{Bold}. \\mkbibitalic*{Italic}.\\end{thebibliography}";
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), refs).expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: refs.len().try_into().expect("refs length fits in u32"),
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(
            main.contains("\"Alpha Title\". (2024). [note]. {Supplement}. Emph. Bold. Italic.")
        );
        assert!(!main.contains("\\mkbibquote*"));
        assert!(!main.contains("\\mkbibparens*"));
        assert!(!main.contains("\\mkbibbrackets*"));
        assert!(!main.contains("\\mkbibbraces*"));
        assert!(!main.contains("\\mkbibemph*"));
        assert!(!main.contains("\\mkbibbold*"));
        assert!(!main.contains("\\mkbibitalic*"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(
            bibliography,
            "[1] \"Alpha Title\". (2024). [note]. {Supplement}. Emph. Bold. Italic."
        );
        assert!(!bibliography.contains("\\mkbibquote*"));
        assert!(!bibliography.contains("\\mkbibparens*"));
        assert!(!bibliography.contains("\\mkbibbrackets*"));
        assert!(!bibliography.contains("\\mkbibbraces*"));
        assert!(!bibliography.contains("\\mkbibemph*"));
        assert!(!bibliography.contains("\\mkbibbold*"));
        assert!(!bibliography.contains("\\mkbibitalic*"));
    }

    #[test]
    fn materialized_bibliography_supports_name_affix_wrapper() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibnamefamily{Doe}, \\mkbibnameaffix{Jr.}.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 109,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("Doe, Jr.."));
        assert!(!main.contains("\\mkbibnamefamily"));
        assert!(!main.contains("\\mkbibnameaffix"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(bibliography, "[1] Doe, Jr..");
        assert!(!bibliography.contains("\\mkbibnamefamily"));
        assert!(!bibliography.contains("\\mkbibnameaffix"));
    }

    #[test]
    fn materialized_bibliography_supports_starred_case_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\fullcite{alpha}.\\bibliography{refs}",
        )
        .expect("write main");
        fs::write(
            root.join("refs.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha}\\MakeSentenceCase*{alpha title}. \\MakeTitleCase*{beta title}.\\end{thebibliography}",
        )
        .expect("write refs");

        let mut scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        scan.files.insert(
            Utf8PathBuf::from("refs.bbl"),
            fs::read_to_string(root.join("refs.bbl")).expect("read refs"),
        );
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![
                    SourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    },
                    SourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 126,
                    },
                ],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");

        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");
        assert!(main.contains("alpha title. beta title."));
        assert!(!main.contains("\\MakeSentenceCase*"));
        assert!(!main.contains("\\MakeTitleCase*"));

        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");
        assert_eq!(bibliography, "[1] alpha title. beta title.");
        assert!(!bibliography.contains("\\MakeSentenceCase*"));
        assert!(!bibliography.contains("\\MakeTitleCase*"));
    }

    #[test]
    fn materialized_bibliography_strips_natexlab_suffix_markup() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\bibliography{refs}").expect("write main");
        fs::write(
            root.join("refs.bbl"),
            r"\begin{thebibliography}{1}\bibitem[Alpha 2024\natexlab{a}]{alpha} Alpha \newblock 2024\NAT@exlab{a}.\end{thebibliography}",
        )
        .expect("write bbl");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("refs.bbl"),
                    start_utf8: 0,
                    end_utf8: 112,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let bibliography = materialized
            .files
            .get(&Utf8PathBuf::from("refs.bbl"))
            .expect("materialized bibliography");

        assert_eq!(aux.citation_year("alpha"), Some("2024a".to_string()));
        assert!(bibliography.contains("[1] Alpha 2024a."));
        assert!(!bibliography.contains("\\natexlab"));
        assert!(!bibliography.contains("\\NAT@exlab"));
    }

    #[test]
    fn materialize_project_supports_eqref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}See \\eqref{sec:intro} and \\eqref*{sec:intro}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1.2".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 16,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See (1.2) and (1.2)."));
        assert!(!main.contains("\\eqref"));
    }

    #[test]
    fn materialize_project_supports_subref_and_subeqref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\subref{fig:panel} and \\subeqref{eq:panel}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "fig:panel".to_string(),
                    number: "1a".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 20,
                },
                super::SemanticLabel {
                    key: "eq:panel".to_string(),
                    number: "2b".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 44,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See 1a and (2b)."));
        assert!(!main.contains("\\subref"));
        assert!(!main.contains("\\subeqref"));
    }

    #[test]
    fn derive_semantic_aux_numbers_equation_labels() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\begin{equation}\\label{eq:second}b\\end{equation}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 128,
                }],
            }],
        );

        assert_eq!(aux.labels.len(), 2);
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "eq:first")
                .expect("first equation")
                .number,
            "1"
        );
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "eq:second")
                .expect("second equation")
                .number,
            "2"
        );
    }

    #[test]
    fn derive_semantic_aux_numbers_align_labels_like_equations() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{align}\\label{eq:first}a\\end{align}\\begin{gather}\\label{eq:second}b\\end{gather}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 128,
                }],
            }],
        );

        assert_eq!(aux.labels.len(), 2);
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "eq:first")
                .expect("first align equation")
                .number,
            "1"
        );
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "eq:second")
                .expect("second gather equation")
                .number,
            "2"
        );
    }

    #[test]
    fn derive_semantic_aux_numbers_figure_and_table_labels_separately() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{figure}\\label{fig:first}a\\end{figure}\\begin{table}\\label{tab:first}b\\end{table}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 128,
                }],
            }],
        );

        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "fig:first")
                .expect("figure label")
                .number,
            "1"
        );
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "tab:first")
                .expect("table label")
                .number,
            "1"
        );
    }

    #[test]
    fn derive_semantic_aux_numbers_algorithm_labels_separately() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{algorithm}\\label{alg:first}a\\end{algorithm}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 96,
                }],
            }],
        );

        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "alg:first")
                .expect("algorithm label")
                .number,
            "1"
        );
    }

    #[test]
    fn derive_semantic_aux_tracks_float_captions_for_lists() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\begin{figure}\\caption[Short Figure]{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{table}\\caption{Long Table Title}\\label{tab:first}b\\end{table}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 192,
                }],
            }],
        );

        assert_eq!(aux.float_captions.len(), 2);
        assert_eq!(aux.float_captions[0].kind, "figure");
        assert_eq!(aux.float_captions[0].number, "1");
        assert_eq!(aux.float_captions[0].title, "Short Figure");
        assert_eq!(aux.float_captions[0].body_title, "Long Figure Title");
        assert_eq!(aux.float_captions[1].kind, "table");
        assert_eq!(aux.float_captions[1].number, "1");
        assert_eq!(aux.float_captions[1].title, "Long Table Title");
    }

    #[test]
    fn derive_semantic_aux_numbers_theorem_and_lemma_labels_separately() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 160,
                }],
            }],
        );

        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "thm:first")
                .expect("theorem label")
                .number,
            "1"
        );
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "lem:first")
                .expect("lemma label")
                .number,
            "1"
        );
    }

    #[test]
    fn derive_semantic_aux_numbers_claim_and_example_labels_separately() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{claim}\\label{clm:first}a\\end{claim}\\begin{example}\\label{ex:first}b\\end{example}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 160,
                }],
            }],
        );

        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "clm:first")
                .expect("claim label")
                .number,
            "1"
        );
        assert_eq!(
            aux.labels
                .iter()
                .find(|label| label.key == "ex:first")
                .expect("example label")
                .number,
            "1"
        );
    }

    #[test]
    fn derive_semantic_aux_tracks_captionof_entries_for_lists() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\captionof{figure}[Short Figure]{Long Figure Title}\\label{fig:first}",
        )
        .expect("write main");
        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 96,
                }],
            }],
        );

        assert_eq!(aux.labels.len(), 1);
        assert_eq!(aux.labels[0].number, "1");
        assert_eq!(aux.float_captions.len(), 1);
        assert_eq!(aux.float_captions[0].kind, "figure");
        assert_eq!(aux.float_captions[0].title, "Short Figure");
        assert_eq!(aux.float_captions[0].body_title, "Long Figure Title");
    }

    #[test]
    fn materialize_project_supports_align_labels_for_equation_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{align}\\label{eq:first}a\\end{align}\\begin{gather}\\label{eq:second}b\\end{gather}See \\eqref{eq:first}, \\autoref{eq:first}, and \\crefrange{eq:first}{eq:second}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let first_offset = source
            .find("\\label{eq:first}")
            .expect("first label offset") as u32;
        let second_offset = source
            .find("\\label{eq:second}")
            .expect("second label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "eq:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: first_offset,
                },
                super::SemanticLabel {
                    key: "eq:second".to_string(),
                    number: "2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: second_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See (1), Equation 1, and Equations 1 to 2."));
        assert!(!main.contains("\\eqref"));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\crefrange"));
    }

    #[test]
    fn materialize_project_supports_nameref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}See \\nameref{sec:intro} and \\nameref*{sec:intro}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 16,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Intro and Intro."));
        assert!(!main.contains("\\nameref"));
    }

    #[test]
    fn materialize_project_supports_titleref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\titleref{sec:intro} and \\Titleref{thm:first}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 16,
                },
                super::SemanticLabel {
                    key: "thm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 70,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Intro and Pythagoras."));
        assert!(!main.contains("\\titleref"));
        assert!(!main.contains("\\Titleref"));
    }

    #[test]
    fn materialize_project_prefers_long_title_for_nameref_even_with_short_toc_title() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section[Short Intro]{Long Introduction}\\label{sec:intro}See \\nameref{sec:intro}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 40,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Short Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Long IntroductionSee Long Introduction."));
        assert!(!main.contains("\\nameref"));
    }

    #[test]
    fn materialize_project_prefers_float_caption_title_for_nameref() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{figure}\\caption{Long Figure Title}\\label{fig:first}a\\end{figure}See \\nameref{fig:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let label_offset = source.find("\\label{fig:first}").expect("label offset") as u32;
        let caption_offset = source
            .find("\\caption{Long Figure Title}")
            .expect("caption offset") as u32;
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "fig:first".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: label_offset,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            float_captions: vec![super::FloatCaption {
                kind: "figure".to_string(),
                number: "1".to_string(),
                title: "Long Figure Title".to_string(),
                body_title: "Long Figure Title".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: caption_offset,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Long Figure Title."));
        assert!(!main.contains("See Intro."));
        assert!(!main.contains("\\nameref"));
    }

    #[test]
    fn materialize_project_supports_autoref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}See \\autoref{sec:intro} and \\autoref*{sec:intro}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 16,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Section 1 and Section 1."));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_supports_namecref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\namecref{sec:intro}, \\nameCref{sub:scope}, and \\lcnamecref{subsub:detail}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 28,
                },
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 54,
                },
                super::SemanticLabel {
                    key: "subsub:detail".to_string(),
                    number: "1.1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 82,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 32,
                },
                super::TocEntry {
                    level: 3,
                    number: "1.1.1".to_string(),
                    title: "Detail".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 60,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See section, Subsection, and subsubsection."));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\nameCref"));
        assert!(!main.contains("\\lcnamecref"));
    }

    #[test]
    fn materialize_project_supports_plural_namecref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\paragraph{Claim}\\label{par:claim}\\paragraph{Case}\\label{par:case}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}See \\namecrefs{sub:scope,sub:detail}, \\nameCrefs{par:claim,par:case}, and \\lcnamecrefs{thm:first,lem:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{sub:scope}").expect("scope label") as u32,
                },
                super::SemanticLabel {
                    key: "sub:detail".to_string(),
                    number: "1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{sub:detail}").expect("detail label") as u32,
                },
                super::SemanticLabel {
                    key: "par:claim".to_string(),
                    number: "1.1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{par:claim}").expect("claim label") as u32,
                },
                super::SemanticLabel {
                    key: "par:case".to_string(),
                    number: "1.1.2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{par:case}").expect("case label") as u32,
                },
                super::SemanticLabel {
                    key: "thm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{thm:first}").expect("theorem label") as u32,
                },
                super::SemanticLabel {
                    key: "lem:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{lem:first}").expect("lemma label") as u32,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Detail".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\subsection{Detail}").expect("detail heading")
                        as u32,
                },
                super::TocEntry {
                    level: 4,
                    number: "1.1.1".to_string(),
                    title: "Claim".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\paragraph{Claim}").expect("claim heading") as u32,
                },
                super::TocEntry {
                    level: 4,
                    number: "1.1.2".to_string(),
                    title: "Case".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\paragraph{Case}").expect("case heading") as u32,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See section; subsection, Paragraphs, and theorem; lemma."));
        assert!(!main.contains("\\namecrefs"));
        assert!(!main.contains("\\nameCrefs"));
        assert!(!main.contains("\\lcnamecrefs"));
    }

    #[test]
    fn materialize_project_supports_labelcref_and_labelcpageref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\label{sec:intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\subsection{Detail}\\label{sub:detail}See \\labelcref{sec:intro,eq:first} and \\labelcpageref{sec:intro,sub:detail}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{sec:intro}").expect("sec label") as u32,
                },
                super::SemanticLabel {
                    key: "eq:first".to_string(),
                    number: "1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{eq:first}").expect("eq label") as u32,
                },
                super::SemanticLabel {
                    key: "sub:detail".to_string(),
                    number: "1.1".to_string(),
                    page: 3,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\label{sub:detail}").expect("sub label") as u32,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Detail".to_string(),
                    page: 3,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: source.find("\\subsection{Detail}").expect("detail heading")
                        as u32,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See 1, (1) and 2, 3."));
        assert!(!main.contains("\\labelcref"));
        assert!(!main.contains("\\labelcpageref"));
    }

    #[test]
    fn materialize_project_supports_crefrange_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\crefrange{sub:scope}{sub:detail} and \\Crefrange{par:claim}{par:case}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 24,
                },
                super::SemanticLabel {
                    key: "sub:detail".to_string(),
                    number: "1.2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 46,
                },
                super::SemanticLabel {
                    key: "par:claim".to_string(),
                    number: "1.2.1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 72,
                },
                super::SemanticLabel {
                    key: "par:case".to_string(),
                    number: "1.2.1.2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 98,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.2".to_string(),
                    title: "Detail".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 28,
                },
                super::TocEntry {
                    level: 4,
                    number: "1.2.1.1".to_string(),
                    title: "Claim".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 52,
                },
                super::TocEntry {
                    level: 4,
                    number: "1.2.1.2".to_string(),
                    title: "Case".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 78,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Subsections 1.1 to 1.2 and Paragraphs 1.2.1.1 to 1.2.1.2."));
        assert!(!main.contains("\\crefrange"));
        assert!(!main.contains("\\Crefrange"));
    }

    #[test]
    fn materialize_project_supports_page_oriented_ref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cpageref{sec:intro}, \\Cpageref{sub:scope}, \\vpageref{sub:scope}, \\autopageref{sec:intro}, \\vref{sec:intro}, and \\Vref{sub:scope}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 28,
                },
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 3,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 56,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 3,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 32,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "See page 2, Page 3, page 3, page 2, section 1 on page 2, and Subsection 1.1 on page 3."
        ));
        assert!(!main.contains("\\cpageref"));
        assert!(!main.contains("\\Cpageref"));
        assert!(!main.contains("\\vpageref"));
        assert!(!main.contains("\\autopageref"));
        assert!(!main.contains("\\vref"));
        assert!(!main.contains("\\Vref"));
    }

    #[test]
    fn materialize_project_supports_pagerefrange_variant() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\pagerefrange{sec:intro}{sub:scope}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 20,
                },
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 4,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 44,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See pages 2 to 4."));
        assert!(!main.contains("\\pagerefrange"));
    }

    #[test]
    fn materialize_project_supports_varioref_range_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\vpagerefrange{sec:intro}{sub:scope} and \\vrefrange{sec:intro}{sub:scope}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 21,
                },
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 4,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 45,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 4,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 32,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(
            main.contains("See pages 2 to 4 and section 1 on page 2 to subsection 1.1 on page 4.")
        );
        assert!(!main.contains("\\vpagerefrange"));
        assert!(!main.contains("\\vrefrange"));
    }

    #[test]
    fn materialize_project_supports_cpagerefrange_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cpagerefrange{sec:intro}{sub:scope} and \\Cpagerefrange{sec:intro}{sub:scope}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 21,
                },
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 4,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 45,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See pages 2 to 4 and Pages 2 to 4."));
        assert!(!main.contains("\\cpagerefrange"));
        assert!(!main.contains("\\Cpagerefrange"));
    }

    #[test]
    fn materialize_project_supports_autoref_for_chapter_and_appendix_labels() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\autoref{chap:intro}, \\autoref*{chap:proof}, and \\autoref{sec:lemma}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "chap:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 24,
                },
                super::SemanticLabel {
                    key: "chap:proof".to_string(),
                    number: "A".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 46,
                },
                super::SemanticLabel {
                    key: "sec:lemma".to_string(),
                    number: "A.1".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 72,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 0,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 0,
                    number: "A".to_string(),
                    title: "Proofs".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 30,
                },
                super::TocEntry {
                    level: 1,
                    number: "A.1".to_string(),
                    title: "Lemma".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 56,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Chapter 1, Appendix A, and Appendix A.1."));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_supports_autoref_for_subsection_depth() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\autoref{sec:intro}, \\autoref{sub:scope}, and \\autoref{subsub:detail}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 20,
                },
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 46,
                },
                super::SemanticLabel {
                    key: "subsub:detail".to_string(),
                    number: "1.1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 74,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 28,
                },
                super::TocEntry {
                    level: 3,
                    number: "1.1.1".to_string(),
                    title: "Detail".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 56,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Section 1, Subsection 1.1, and Subsubsection 1.1.1."));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_supports_autoref_for_paragraph_depth() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\autoref{par:claim} and \\autoref{subpar:case}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "par:claim".to_string(),
                    number: "1.1.1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 26,
                },
                super::SemanticLabel {
                    key: "subpar:case".to_string(),
                    number: "1.1.1.1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 54,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 4,
                    number: "1.1.1.1".to_string(),
                    title: "Claim".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 5,
                    number: "1.1.1.1.1".to_string(),
                    title: "Case".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 32,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Paragraph 1.1.1.1 and Subparagraph 1.1.1.1.1."));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_supports_equation_kinds_for_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\begin{equation}\\label{eq:second}b\\end{equation}See \\autoref{eq:first}, \\cref{eq:first,eq:second}, \\namecref{eq:first}, \\vref{eq:first}, and \\crefrange{eq:first}{eq:second}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let first_offset = source
            .find("\\label{eq:first}")
            .expect("first label offset") as u32;
        let second_offset = source
            .find("\\label{eq:second}")
            .expect("second label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "eq:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: first_offset,
                },
                super::SemanticLabel {
                    key: "eq:second".to_string(),
                    number: "2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: second_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "See Equation 1, Equations 1, 2, equation, equation 1 on page 1, and Equations 1 to 2."
        ));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
        assert!(!main.contains("\\crefrange"));
    }

    #[test]
    fn materialize_project_supports_float_kinds_for_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{figure}\\label{fig:first}a\\end{figure}\\begin{table}\\label{tab:first}b\\end{table}See \\autoref{fig:first}, \\cref{tab:first}, \\namecref{fig:first}, and \\vref{tab:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let figure_offset = source
            .find("\\label{fig:first}")
            .expect("figure label offset") as u32;
        let table_offset = source
            .find("\\label{tab:first}")
            .expect("table label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "fig:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: figure_offset,
                },
                super::SemanticLabel {
                    key: "tab:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: table_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Figure 1, Table 1, figure, and table 1 on page 1."));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
    }

    #[test]
    fn materialize_project_supports_algorithm_kinds_for_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{algorithm}\\label{alg:first}a\\end{algorithm}See \\autoref{alg:first}, \\namecref{alg:first}, and \\vref{alg:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let algorithm_offset = source
            .find("\\label{alg:first}")
            .expect("algorithm label offset") as u32;
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "alg:first".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: algorithm_offset,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Algorithm 1, algorithm, and algorithm 1 on page 1."));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
    }

    #[test]
    fn materialize_project_supports_theorem_kinds_for_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}See \\autoref{thm:first}, \\cref{lem:first}, \\namecref{thm:first}, and \\vref{lem:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let theorem_offset = source
            .find("\\label{thm:first}")
            .expect("theorem label offset") as u32;
        let lemma_offset = source
            .find("\\label{lem:first}")
            .expect("lemma label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "thm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: theorem_offset,
                },
                super::SemanticLabel {
                    key: "lem:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: lemma_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Theorem 1, Lemma 1, theorem, and lemma 1 on page 1."));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
    }

    #[test]
    fn materialize_project_supports_newtheorem_defined_environments() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\newtheorem{oblemma}{Observation Lemma}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\begin{oblemma}[Second]\\label{obs:second}b\\end{oblemma}See \\autoref{obs:first}, \\cref{obs:first,obs:second}, \\namecrefs{obs:first,obs:second}, and \\nameref{obs:second}.";
        fs::write(root.join("main.tex"), source).expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: source.len() as u32,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert_eq!(aux.label_number("obs:first"), Some("1"));
        assert_eq!(aux.label_number("obs:second"), Some("2"));
        assert!(main.contains("Observation Lemma 1. a"));
        assert!(main.contains("Observation Lemma 2 (Second). b"));
        assert!(main.contains(
            "See Observation Lemma 1, Observation Lemmas 1, 2, observation lemmas, and Second."
        ));
        assert!(!main.contains("\\newtheorem"));
        assert!(!main.contains("\\begin{oblemma}"));
        assert!(!main.contains("\\end{oblemma}"));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecrefs"));
        assert!(!main.contains("\\nameref"));
    }

    #[test]
    fn materialize_project_supports_newtheorem_shared_counters() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\newtheorem{oblemma}{Observation Lemma}\\newtheorem{obcor}[oblemma]{Observation Corollary}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\begin{obcor}\\label{obs:second}b\\end{obcor}See \\autoref{obs:first} and \\autoref{obs:second}.";
        fs::write(root.join("main.tex"), source).expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: source.len() as u32,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert_eq!(aux.label_number("obs:first"), Some("1"));
        assert_eq!(aux.label_number("obs:second"), Some("2"));
        assert!(main.contains("Observation Lemma 1. a"));
        assert!(main.contains("Observation Corollary 2. b"));
        assert!(main.contains("See Observation Lemma 1 and Observation Corollary 2."));
        assert!(!main.contains("\\newtheorem"));
        assert!(!main.contains("\\begin{oblemma}"));
        assert!(!main.contains("\\begin{obcor}"));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_supports_newtheorem_section_scoped_counters() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\newtheorem{oblemma}{Observation Lemma}[section]\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\section{Next}\\begin{oblemma}\\label{obs:second}b\\end{oblemma}See \\autoref{obs:first} and \\autoref{obs:second}.";
        fs::write(root.join("main.tex"), source).expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: source.len() as u32,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert_eq!(aux.label_number("obs:first"), Some("1.1"));
        assert_eq!(aux.label_number("obs:second"), Some("2.1"));
        assert!(main.contains("Observation Lemma 1.1. a"));
        assert!(main.contains("Observation Lemma 2.1. b"));
        assert!(main.contains("See Observation Lemma 1.1 and Observation Lemma 2.1."));
        assert!(!main.contains("\\newtheorem"));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_prefers_newtheorem_override_for_builtin_env_and_shared_scope() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\newtheorem{theorem}{Theorem}[section]\\newtheorem{cor}[theorem]{Corollary}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{cor}\\label{cor:first}b\\end{cor}\\section{Next}\\begin{theorem}\\label{thm:second}c\\end{theorem}\\begin{cor}\\label{cor:second}d\\end{cor}See \\autoref{thm:first}, \\autoref{cor:first}, \\autoref{thm:second}, and \\autoref{cor:second}.";
        fs::write(root.join("main.tex"), source).expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: source.len() as u32,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert_eq!(aux.label_number("thm:first"), Some("1.1"));
        assert_eq!(aux.label_number("cor:first"), Some("1.2"));
        assert_eq!(aux.label_number("thm:second"), Some("2.1"));
        assert_eq!(aux.label_number("cor:second"), Some("2.2"));
        assert!(main.contains("Theorem 1.1. a"));
        assert!(main.contains("Corollary 1.2. b"));
        assert!(main.contains("Theorem 2.1. c"));
        assert!(main.contains("Corollary 2.2. d"));
        assert!(main.contains("See Theorem 1.1, Corollary 1.2, Theorem 2.1, and Corollary 2.2."));
        assert!(!main.contains("\\newtheorem"));
        assert!(!main.contains("\\autoref"));
    }

    #[test]
    fn materialize_project_strips_theoremstyle_declarations() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\theoremstyle{definition}\\newtheoremstyle{tight}{}{}{}{}{}{}{ }{}\\swapnumbers\\newtheorem{oblemma}{Observation Lemma}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}See \\autoref{obs:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: source.len() as u32,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Observation Lemma 1. a"));
        assert!(main.contains("See Observation Lemma 1."));
        assert!(!main.contains("\\theoremstyle"));
        assert!(!main.contains("\\newtheoremstyle"));
        assert!(!main.contains("\\swapnumbers"));
    }

    #[test]
    fn materialize_project_renders_theorem_and_proof_environment_headers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}\\begin{proof}[Sketch]b\\end{proof}",
        )
        .expect("write main");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("IntroTheorem 1 (Pythagoras). aProof (Sketch). b"));
        assert!(!main.contains("\\begin{theorem}"));
        assert!(!main.contains("\\end{theorem}"));
        assert!(!main.contains("\\begin{proof}"));
        assert!(!main.contains("\\end{proof}"));
    }

    #[test]
    fn materialize_project_prefers_theorem_title_for_nameref() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\nameref{thm:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let label_offset = source.find("\\label{thm:first}").expect("label offset") as u32;
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "thm:first".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: label_offset,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Pythagoras."));
        assert!(!main.contains("See Intro."));
        assert!(!main.contains("\\nameref"));
    }

    #[test]
    fn materialize_project_supports_claim_and_example_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{claim}\\label{clm:first}a\\end{claim}\\begin{example}\\label{ex:first}b\\end{example}See \\autoref{clm:first}, \\cref{ex:first}, \\namecref{clm:first}, and \\vref{ex:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let claim_offset = source
            .find("\\label{clm:first}")
            .expect("claim label offset") as u32;
        let example_offset = source
            .find("\\label{ex:first}")
            .expect("example label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "clm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: claim_offset,
                },
                super::SemanticLabel {
                    key: "ex:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: example_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Claim 1, Example 1, claim, and example 1 on page 1."));
        assert!(!main.contains("\\autoref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
    }

    #[test]
    fn materialize_project_supports_axiom_fact_and_observation_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{axiom}\\label{ax:first}a\\end{axiom}\\begin{fact}\\label{fact:first}b\\end{fact}\\begin{observation}\\label{obs:first}c\\end{observation}See \\thmref{ax:first}, \\cref{fact:first}, \\namecref{obs:first}, and \\vref{fact:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let axiom_offset = source
            .find("\\label{ax:first}")
            .expect("axiom label offset") as u32;
        let fact_offset = source
            .find("\\label{fact:first}")
            .expect("fact label offset") as u32;
        let observation_offset = source
            .find("\\label{obs:first}")
            .expect("observation label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "ax:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: axiom_offset,
                },
                super::SemanticLabel {
                    key: "fact:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: fact_offset,
                },
                super::SemanticLabel {
                    key: "obs:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: observation_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Axiom 1, Fact 1, observation, and fact 1 on page 1."));
        assert!(!main.contains("\\thmref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
    }

    #[test]
    fn materialize_project_supports_problem_exercise_question_and_notation_reference_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{problem}\\label{prob:first}a\\end{problem}\\begin{exercise}\\label{ex:first}b\\end{exercise}\\begin{question}\\label{q:first}c\\end{question}\\begin{notation}\\label{not:first}d\\end{notation}See \\thmref{prob:first}, \\cref{ex:first}, \\namecref{not:first}, and \\vref{q:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let problem_offset = source
            .find("\\label{prob:first}")
            .expect("problem label offset") as u32;
        let exercise_offset = source
            .find("\\label{ex:first}")
            .expect("exercise label offset") as u32;
        let question_offset = source
            .find("\\label{q:first}")
            .expect("question label offset") as u32;
        let notation_offset = source
            .find("\\label{not:first}")
            .expect("notation label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "prob:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: problem_offset,
                },
                super::SemanticLabel {
                    key: "ex:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: exercise_offset,
                },
                super::SemanticLabel {
                    key: "q:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: question_offset,
                },
                super::SemanticLabel {
                    key: "not:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: notation_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Problem 1, Exercise 1, notation, and question 1 on page 1."));
        assert!(!main.contains("\\thmref"));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\namecref"));
        assert!(!main.contains("\\vref"));
    }

    #[test]
    fn materialize_project_supports_thmref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{claim}\\label{clm:first}b\\end{claim}See \\thmref{thm:first} and \\Thmref{clm:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let theorem_offset = source
            .find("\\label{thm:first}")
            .expect("theorem label offset") as u32;
        let claim_offset = source
            .find("\\label{clm:first}")
            .expect("claim label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "thm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: theorem_offset,
                },
                super::SemanticLabel {
                    key: "clm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: claim_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Theorem 1 and Claim 1."));
        assert!(!main.contains("\\thmref"));
        assert!(!main.contains("\\Thmref"));
    }

    #[test]
    fn materialize_project_supports_fullref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\section{Intro}\\label{sec:intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\fullref{sec:intro} and \\Fullref{thm:first}.";
        fs::write(root.join("main.tex"), source).expect("write main");
        let theorem_offset = source
            .find("\\label{thm:first}")
            .expect("theorem label offset") as u32;
        let section_offset = source
            .find("\\label{sec:intro}")
            .expect("section label offset") as u32;
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: section_offset,
                },
                super::SemanticLabel {
                    key: "thm:first".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: theorem_offset,
                },
            ],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Section 1 (Intro) and Theorem 1 (Pythagoras)."));
        assert!(!main.contains("\\fullref"));
        assert!(!main.contains("\\Fullref"));
    }

    #[test]
    fn materialize_project_strips_float_and_equation_environment_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\begin{figure}[tbp]\\caption{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{equation}\\label{eq:first}b\\end{equation}\\begin{table*}[!htbp]c\\end{table*}\\begin{algorithm}[H]d\\end{algorithm}";
        fs::write(root.join("main.tex"), source).expect("write main");
        let figure_label_offset = source.find("\\label{fig:first}").expect("figure label") as u32;
        let equation_label_offset =
            source.find("\\label{eq:first}").expect("equation label") as u32;
        let figure_caption_offset = source
            .find("\\caption{Long Figure Title}")
            .expect("figure caption") as u32;

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux {
                labels: vec![
                    super::SemanticLabel {
                        key: "fig:first".to_string(),
                        number: "1".to_string(),
                        page: 1,
                        file: Utf8PathBuf::from("main.tex"),
                        offset_utf8: figure_label_offset,
                    },
                    super::SemanticLabel {
                        key: "eq:first".to_string(),
                        number: "1".to_string(),
                        page: 1,
                        file: Utf8PathBuf::from("main.tex"),
                        offset_utf8: equation_label_offset,
                    },
                ],
                float_captions: vec![super::FloatCaption {
                    kind: "figure".to_string(),
                    number: "1".to_string(),
                    title: "Long Figure Title".to_string(),
                    body_title: "Long Figure Title".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: figure_caption_offset,
                }],
                ..SemanticAux::default()
            },
        )
        .expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Figure 1: Long Figure Titlea$b$cd"));
        assert!(!main.contains("tbp"));
        assert!(!main.contains("htbp"));
        assert!(!main.contains("\\begin{figure}"));
        assert!(!main.contains("\\end{figure}"));
        assert!(!main.contains("\\begin{equation}"));
        assert!(!main.contains("\\end{equation}"));
        assert!(!main.contains("\\begin{table*}"));
        assert!(!main.contains("\\begin{algorithm}"));
    }

    #[test]
    fn materialize_project_preserves_math_boundaries_for_display_environment_wrappers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\begin{equation}C\\addone W_m+C^k\\notgate\\end{equation}\\begin{displaymath}C^r\\notgate\\end{displaymath}\\begin{flalign*}C^k\\cnot\\end{flalign*}";
        fs::write(root.join("main.tex"), source).expect("write main");

        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("$C\\addone W_m+C^k\\notgate$"));
        assert!(main.contains("$C^r\\notgate$"));
        assert!(main.contains("$C^k\\cnot$"));
        assert!(!main.contains("\\begin{equation}"));
        assert!(!main.contains("\\begin{displaymath}"));
        assert!(!main.contains("\\begin{flalign*}"));
    }

    #[test]
    fn materialize_project_supports_float_captions_and_lists() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\listoffigures\\listoftables\\begin{figure}\\caption[Short Figure]{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{table}\\caption{Long Table Title}\\label{tab:first}b\\end{table}";
        fs::write(root.join("main.tex"), source).expect("write main");
        let figure_caption_offset = source
            .find("\\caption[Short Figure]{Long Figure Title}")
            .expect("figure caption offset") as u32;
        let table_caption_offset = source
            .find("\\caption{Long Table Title}")
            .expect("table caption offset") as u32;
        let aux = SemanticAux {
            float_captions: vec![
                super::FloatCaption {
                    kind: "figure".to_string(),
                    number: "1".to_string(),
                    title: "Short Figure".to_string(),
                    body_title: "Long Figure Title".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: figure_caption_offset,
                },
                super::FloatCaption {
                    kind: "table".to_string(),
                    number: "1".to_string(),
                    title: "Long Table Title".to_string(),
                    body_title: "Long Table Title".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: table_caption_offset,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("List of Figures\n1 Short Figure .... 1\n"));
        assert!(main.contains("List of Tables\n1 Long Table Title .... 1\n"));
        assert!(main.contains("Figure 1: Long Figure Title"));
        assert!(main.contains("Table 1: Long Table Title"));
        assert!(!main.contains("\\listoffigures"));
        assert!(!main.contains("\\listoftables"));
        assert!(!main.contains("\\caption"));
    }

    #[test]
    fn materialize_project_supports_captionof_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source =
            "\\listoffigures\\captionof{figure}[Short Figure]{Long Figure Title}\\label{fig:first}";
        fs::write(root.join("main.tex"), source).expect("write main");
        let caption_offset = source
            .find("\\captionof{figure}[Short Figure]{Long Figure Title}")
            .expect("captionof offset") as u32;
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "fig:first".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: caption_offset + 50,
            }],
            float_captions: vec![super::FloatCaption {
                kind: "figure".to_string(),
                number: "1".to_string(),
                title: "Short Figure".to_string(),
                body_title: "Long Figure Title".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: caption_offset,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("List of Figures\n1 Short Figure .... 1\n"));
        assert!(main.contains("Figure 1: Long Figure Title"));
        assert!(!main.contains("\\captionof"));
    }

    #[test]
    fn materialize_project_keeps_starred_captions_out_of_float_lists() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let source = "\\listoffigures\\begin{figure}\\caption*{Hidden Figure Title}\\end{figure}\\captionof*{figure}{Detached Hidden Figure}";
        fs::write(root.join("main.tex"), source).expect("write main");
        let hidden_offset = source
            .find("\\caption*{Hidden Figure Title}")
            .expect("caption offset") as u32;
        let detached_offset = source
            .find("\\captionof*{figure}{Detached Hidden Figure}")
            .expect("captionof offset") as u32;
        let aux = SemanticAux {
            float_captions: vec![
                super::FloatCaption {
                    kind: "figure".to_string(),
                    number: String::new(),
                    title: "Hidden Figure Title".to_string(),
                    body_title: "Hidden Figure Title".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: hidden_offset,
                },
                super::FloatCaption {
                    kind: "figure".to_string(),
                    number: String::new(),
                    title: "Detached Hidden Figure".to_string(),
                    body_title: "Detached Hidden Figure".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: detached_offset,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(!main.contains("List of Figures"));
        assert!(main.contains("Hidden Figure Title"));
        assert!(main.contains("Detached Hidden Figure"));
        assert!(!main.contains("\\caption*"));
        assert!(!main.contains("\\captionof*"));
    }

    #[test]
    fn materialize_project_supports_cleveref_variants() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\label{sec:intro}See \\cref{sec:intro} and \\Cref*{sec:intro}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 16,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Section 1 and Section 1."));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\Cref"));
    }

    #[test]
    fn materialize_project_supports_multi_label_cleveref_for_subsection_pluralization() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cref{sub:scope,sub:detail} and \\cref{sub:scope,subsub:detail}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sub:scope".to_string(),
                    number: "1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 20,
                },
                super::SemanticLabel {
                    key: "sub:detail".to_string(),
                    number: "1.2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 42,
                },
                super::SemanticLabel {
                    key: "subsub:detail".to_string(),
                    number: "1.2.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 70,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 2,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 8,
                },
                super::TocEntry {
                    level: 2,
                    number: "1.2".to_string(),
                    title: "Detail".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 30,
                },
                super::TocEntry {
                    level: 3,
                    number: "1.2.1".to_string(),
                    title: "Detail inner".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 56,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See Subsections 1.1, 1.2 and Subsection 1.1; Subsubsection 1.2.1."));
        assert!(!main.contains("\\cref"));
    }

    #[test]
    fn materialize_project_supports_multi_label_cleveref_lists() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cref{sec:intro,sec:scope}, \\Cref{chap:intro,chap:proof}, and \\cref{sec:scope,chap:proof}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![
                super::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1.1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 28,
                },
                super::SemanticLabel {
                    key: "sec:scope".to_string(),
                    number: "1.2".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 48,
                },
                super::SemanticLabel {
                    key: "chap:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 8,
                },
                super::SemanticLabel {
                    key: "chap:proof".to_string(),
                    number: "A".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 96,
                },
            ],
            toc: vec![
                super::TocEntry {
                    level: 0,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 1,
                    number: "1.1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 16,
                },
                super::TocEntry {
                    level: 1,
                    number: "1.2".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 36,
                },
                super::TocEntry {
                    level: 0,
                    number: "A".to_string(),
                    title: "Proofs".to_string(),
                    page: 2,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 84,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains(
            "See Sections 1.1, 1.2, Chapter 1; Appendix A, and Section 1.2; Appendix A."
        ));
        assert!(!main.contains("\\cref"));
        assert!(!main.contains("\\Cref"));
    }

    #[test]
    fn materialize_project_strips_appendix_command_and_keeps_appendix_titles() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\section{Intro}\\appendix\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:lemma".to_string(),
                number: "A.1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 72,
            }],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 1,
                    number: "A".to_string(),
                    title: "Proofs".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 31,
                },
                super::TocEntry {
                    level: 2,
                    number: "A.1".to_string(),
                    title: "Lemma".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 47,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("1 Intro .... 1"));
        assert!(main.contains("A Proofs .... 1"));
        assert!(main.contains("A.1 Lemma .... 1"));
        assert!(main.contains("See A.1."));
        assert!(!main.contains("\\appendix"));
    }

    #[test]
    fn materialize_project_strips_appendices_command_and_keeps_appendix_titles() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\section{Intro}\\appendices\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:lemma".to_string(),
                number: "A.1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 74,
            }],
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 1,
                    number: "A".to_string(),
                    title: "Proofs".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 33,
                },
                super::TocEntry {
                    level: 2,
                    number: "A.1".to_string(),
                    title: "Lemma".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 49,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("A Proofs .... 1"));
        assert!(main.contains("A.1 Lemma .... 1"));
        assert!(main.contains("See A.1."));
        assert!(!main.contains("\\appendices"));
    }

    #[test]
    fn materialize_project_prefers_long_section_title_in_body_and_short_title_in_toc() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\section[Short Intro]{Long Introduction}\\label{sec:intro}",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Short Intro".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("1 Short Intro .... 2"));
        assert!(main.contains("Long Introduction"));
        assert!(!main.contains("\\section["));
    }

    #[test]
    fn materialize_project_supports_chapter_numbering_and_titles() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\chapter{Intro}\\section{Scope}\\label{sec:scope}See \\ref{sec:scope}.",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:scope".to_string(),
                number: "1.1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 47,
            }],
            toc: vec![
                super::TocEntry {
                    level: 0,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 1,
                    number: "1.1".to_string(),
                    title: "Scope".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 16,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("1 Intro .... 1"));
        assert!(main.contains("1.1 Scope .... 1"));
        assert!(main.contains("See 1.1."));
        assert!(!main.contains("\\chapter"));
    }

    #[test]
    fn materialize_project_preserves_bibliography_stem_order_for_citation_numbers() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cite{beta} then \\cite{alpha}.\\bibliography{refsb,refsa}",
        )
        .expect("write main");
        fs::write(
            root.join("refsb.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{beta} Beta entry.\\end{thebibliography}",
        )
        .expect("write refsb");
        fs::write(
            root.join("refsa.bbl"),
            "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
        )
        .expect("write refsa");

        let scan = scan_project(&root, &Utf8PathBuf::from("main.tex")).expect("scan");
        let aux = derive_semantic_aux(
            &scan,
            &[PageSourceSlice {
                page_index: 0,
                source_spans: vec![SourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 64,
                }],
            }],
        );
        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("See [1] then [2]."));
    }

    #[test]
    fn materialize_project_keeps_starred_section_title_in_body_without_raw_command() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\section*{Prelude}\\section{Intro}\\label{sec:intro}",
        )
        .expect("write main");
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            toc: vec![super::TocEntry {
                level: 1,
                number: "1".to_string(),
                title: "Intro".to_string(),
                page: 1,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 0,
            }],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Prelude"));
        assert!(main.contains("1 Intro .... 1"));
        assert!(!main.contains("\\section*"));
    }

    #[test]
    fn materialize_project_strips_manual_toc_commands_but_keeps_manual_toc_output() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\tableofcontents\\section*{Prelude}\\phantomsection\\addcontentsline{toc}{section}{Prelude}\\section{Intro}",
        )
        .expect("write main");
        let aux = SemanticAux {
            toc: vec![
                super::TocEntry {
                    level: 1,
                    number: "".to_string(),
                    title: "Prelude".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
                super::TocEntry {
                    level: 1,
                    number: "1".to_string(),
                    title: "Intro".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                },
            ],
            ..SemanticAux::default()
        };

        let materialized =
            materialize_project(&root, &Utf8PathBuf::from("main.tex"), &aux).expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("Prelude .... 1"));
        assert!(main.contains("1 Intro .... 1"));
        assert!(main.contains("Prelude"));
        assert!(!main.contains("\\phantomsection"));
        assert!(!main.contains("\\addcontentsline"));
    }

    #[test]
    fn materialize_project_preserves_includeonly_for_runtime_semantics() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("chapters")).expect("chapters dir");
        fs::write(
            root.join("main.tex"),
            "\\includeonly{chapters/intro}\\include{chapters/intro}",
        )
        .expect("write main");
        fs::write(root.join("chapters/intro.tex"), "Intro").expect("write intro");
        let materialized = materialize_project(
            &root,
            &Utf8PathBuf::from("main.tex"),
            &SemanticAux::default(),
        )
        .expect("materialize");
        let main = materialized
            .files
            .get(&Utf8PathBuf::from("main.tex"))
            .expect("materialized main");

        assert!(main.contains("\\includeonly{chapters/intro}"));
        assert!(main.contains("\\include{chapters/intro}"));
    }

    #[test]
    fn semantic_aux_provides_render_model_aux_view() {
        let aux = SemanticAux {
            labels: vec![super::SemanticLabel {
                key: "sec:intro".to_string(),
                number: "1".to_string(),
                page: 2,
                file: Utf8PathBuf::from("main.tex"),
                offset_utf8: 10,
            }],
            bibliography: vec![super::BibliographyEntry {
                key: "alpha".to_string(),
                text: "Alpha entry.".to_string(),
                label: Some(r"Alpha 2024\natexlab{a}".to_string()),
                file: Utf8PathBuf::from("refs.bbl"),
            }],
            ..SemanticAux::default()
        };

        assert_eq!(
            aux.citation_label("alpha", CitationStyleHint::Numeric)
                .expect("citation label")
                .text,
            "Alpha 2024a"
        );
        assert_eq!(
            aux.bibliography_record("alpha")
                .expect("bibliography record")
                .text,
            "Alpha entry."
        );
        assert_eq!(
            aux.label_target("sec:intro").expect("label target").number,
            "1"
        );
    }
}
