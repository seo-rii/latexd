use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    process::Command,
};

use camino::{Utf8Path, Utf8PathBuf};
use latexd::compiler::{CompileRequest, CompilerDriver};
use tempfile::tempdir;
use tex_render_model::{
    AbstractBlock, BibliographyBlock, BibliographyItemIr, DocumentIr, EnvironmentBlock,
    FallbackReason, GraphicBlock, HeadingBlock, InlineNode, IrBlock, ListBlock, ParagraphBlock,
    ProvenanceSpan, RawFallbackIr, SourceProvenance, SourceSpan, TableBlock, TableCell, TableRow,
    TitleBlock,
};
use tex_world::ProjectWorld;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, serde::Deserialize)]
struct OracleManifest {
    cases: Vec<OracleCase>,
}

#[derive(Debug, serde::Deserialize)]
struct OracleCase {
    arxiv_id: String,
    version: String,
    title: String,
    toplevel: Utf8PathBuf,
    license: String,
    source_url: String,
    pdf_url: String,
    min_oracle_tokens: usize,
    min_internal_tokens: usize,
    min_common_token_ratio: f64,
    #[serde(default = "default_max_page_count_delta")]
    max_page_count_delta: usize,
    #[serde(default = "default_min_first_page_ink_ratio")]
    min_first_page_ink_ratio: f64,
}

#[derive(Debug, serde::Serialize)]
struct OracleReport {
    corpus_root: Utf8PathBuf,
    strict: bool,
    cases: Vec<OracleCaseReport>,
}

#[derive(Debug, serde::Serialize)]
struct OracleCaseReport {
    arxiv_id: String,
    version: String,
    title: String,
    license: String,
    source_url: String,
    pdf_url: String,
    toplevel: Utf8PathBuf,
    oracle_pdf: Utf8PathBuf,
    oracle_text: Utf8PathBuf,
    oracle_page_count: usize,
    oracle_first_page_raster: Utf8PathBuf,
    oracle_first_page_raster_smoke: RasterSmokeReport,
    max_page_count_delta: usize,
    min_first_page_ink_ratio: f64,
    source_root: Utf8PathBuf,
    oracle_token_count: usize,
    oracle_unique_token_count: usize,
    oracle_normalized_token_count: usize,
    oracle_normalized_unique_token_count: usize,
    internal_token_count: Option<usize>,
    internal_unique_token_count: Option<usize>,
    common_unique_token_count: Option<usize>,
    common_unique_token_ratio: Option<f64>,
    internal_normalized_token_count: Option<usize>,
    internal_normalized_unique_token_count: Option<usize>,
    normalized_common_unique_token_count: Option<usize>,
    normalized_common_unique_token_ratio: Option<f64>,
    ir_structure_slices: Vec<OracleStructureSliceReport>,
    missing_token_sample: Vec<String>,
    extra_token_sample: Vec<String>,
    normalized_missing_token_sample: Vec<String>,
    normalized_extra_token_sample: Vec<String>,
    metric_findings: Vec<OracleMetricFinding>,
    internal_text: Option<Utf8PathBuf>,
    internal_pdf: Option<Utf8PathBuf>,
    internal_document_ir: Option<Utf8PathBuf>,
    display_list_render: Option<OracleRenderPathReport>,
    internal_page_count: Option<usize>,
    page_count_delta: Option<i64>,
    page_count_within_tolerance: Option<bool>,
    internal_first_page_raster: Option<Utf8PathBuf>,
    internal_first_page_raster_smoke: Option<RasterSmokeReport>,
    first_page_raster_gross: Option<RasterGrossReport>,
    first_page_raster_diff: Option<Utf8PathBuf>,
    first_page_raster_diff_metrics: Option<RasterDiffReport>,
    internal_build_failure: Option<String>,
    internal_diagnostics: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct OracleRenderPathReport {
    pdf: Utf8PathBuf,
    text: Utf8PathBuf,
    token_count: usize,
    unique_token_count: usize,
    common_unique_token_count: usize,
    common_unique_token_ratio: f64,
    normalized_token_count: usize,
    normalized_unique_token_count: usize,
    normalized_common_unique_token_count: usize,
    normalized_common_unique_token_ratio: f64,
    missing_token_sample: Vec<String>,
    extra_token_sample: Vec<String>,
    normalized_missing_token_sample: Vec<String>,
    normalized_extra_token_sample: Vec<String>,
    page_count: usize,
    page_count_delta: i64,
    page_count_within_tolerance: bool,
    first_page_raster: Utf8PathBuf,
    first_page_raster_smoke: RasterSmokeReport,
    first_page_raster_gross: RasterGrossReport,
    first_page_raster_diff: Utf8PathBuf,
    first_page_raster_diff_metrics: RasterDiffReport,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct OracleStructureSliceReport {
    kind: OracleStructureSliceKind,
    token_count: usize,
    unique_token_count: usize,
    common_unique_token_count: usize,
    common_unique_token_ratio: f64,
    oracle_unique_token_coverage_ratio: f64,
    normalized_token_count: usize,
    normalized_unique_token_count: usize,
    normalized_common_unique_token_count: usize,
    normalized_common_unique_token_ratio: f64,
    normalized_oracle_unique_token_coverage_ratio: f64,
    source_backed_extra_token_count: usize,
    source_backed_extra_token_ratio: Option<f64>,
    normalized_source_backed_extra_token_count: usize,
    normalized_source_backed_extra_token_ratio: Option<f64>,
    extra_token_sample: Vec<String>,
    normalized_extra_token_sample: Vec<String>,
    source_backed_extra_token_sample: Vec<String>,
    normalized_source_backed_extra_token_sample: Vec<String>,
    source_unbacked_extra_token_sample: Vec<String>,
    normalized_source_unbacked_extra_token_sample: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum OracleStructureSliceKind {
    FrontMatter,
    Abstract,
    Body,
    Caption,
    Table,
    References,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct RasterSmokeReport {
    width_px: u32,
    height_px: u32,
    non_white_pixel_count: u64,
    non_white_bbox: Option<RasterBoundingBox>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct RasterBoundingBox {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct RasterGrossReport {
    status: RasterGrossStatus,
    page_size_matches: bool,
    oracle_ink_bbox_area_px: Option<u64>,
    internal_ink_bbox_area_px: Option<u64>,
    internal_to_oracle_ink_bbox_ratio: Option<f64>,
    oracle_ink_pixel_count: Option<u64>,
    internal_ink_pixel_count: Option<u64>,
    internal_to_oracle_ink_pixel_ratio: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct RasterDiffReport {
    width_px: u32,
    height_px: u32,
    differing_pixel_count: u64,
    differing_pixel_bbox: Option<RasterBoundingBox>,
    overlapping_differing_pixel_count: u64,
    oracle_only_pixel_count: u64,
    internal_only_pixel_count: u64,
    differing_pixel_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum RasterGrossStatus {
    Pass,
    PageSizeMismatch,
    BlankOraclePage,
    BlankInternalPage,
    MissingMajorInkBoundingBox,
    MissingMajorInkPixels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum OracleMetricFinding {
    BuildFailed,
    InternalTokenCountBelowBudget,
    RawOverlapBelowBudget,
    NormalizedOverlapBelowBudget,
    NormalizationSensitiveOverlap,
    PageCountOutOfTolerance,
    FirstPageRasterGrossFailure,
}

fn default_max_page_count_delta() -> usize {
    2
}

fn default_min_first_page_ink_ratio() -> f64 {
    0.35
}

#[tokio::test]
#[ignore = "requires a downloaded arXiv source/PDF corpus"]
async fn arxiv_cc0_local_corpus_compares_internal_pdf_text_to_official_pdf() {
    let Some(corpus_root) = env::var_os("LATEXD_ARXIV_ORACLE_CORPUS")
        .or_else(|| env::var_os("LATEXD_ARXIV_CC0_CORPUS"))
    else {
        eprintln!("skipping: LATEXD_ARXIV_ORACLE_CORPUS or LATEXD_ARXIV_CC0_CORPUS is not set");
        return;
    };
    let corpus_root =
        Utf8PathBuf::from_path_buf(corpus_root.into()).expect("corpus root should be utf8");
    let pdftotext = match which::which("pdftotext") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("skipping: pdftotext is not installed");
            return;
        }
    };
    let pdfinfo = match which::which("pdfinfo") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("skipping: pdfinfo is not installed");
            return;
        }
    };
    let pdftoppm = match which::which("pdftoppm") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("skipping: pdftoppm is not installed");
            return;
        }
    };
    let manifest_path = env::var_os("LATEXD_ARXIV_ORACLE_MANIFEST")
        .map(|path| Utf8PathBuf::from_path_buf(path.into()).expect("manifest path should be utf8"))
        .unwrap_or_else(|| {
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/arxiv-oracle/cc0-smoke.json")
        });
    let manifest = serde_json::from_slice::<OracleManifest>(
        &fs::read(manifest_path.as_std_path()).expect("read arXiv oracle manifest"),
    )
    .expect("parse arXiv oracle manifest");
    let case_filter = env::var("LATEXD_ARXIV_ORACLE_CASE").ok();
    let strict = env::var_os("LATEXD_ARXIV_ORACLE_STRICT").is_some_and(|value| value == "1");
    let report_dir = env::var_os("LATEXD_ARXIV_ORACLE_REPORT_DIR")
        .map(|path| Utf8PathBuf::from_path_buf(path.into()).expect("report dir should be utf8"))
        .unwrap_or_else(|| corpus_root.join("reports"));
    fs::create_dir_all(report_dir.as_std_path()).expect("create arXiv oracle report dir");

    let mut reports = Vec::new();
    for case in &manifest.cases {
        if case_filter
            .as_deref()
            .is_some_and(|filter| filter != case.arxiv_id)
        {
            continue;
        }
        eprintln!("running arXiv oracle case {}", case.arxiv_id);
        let Some((source_root, oracle_pdf)) = locate_case_files(&corpus_root, case) else {
            eprintln!("skipping {}: local source or PDF is missing", case.arxiv_id);
            continue;
        };
        let oracle_text = extract_pdf_text(&pdftotext, &oracle_pdf)
            .unwrap_or_else(|error| panic!("{} oracle pdftotext failed: {error}", case.arxiv_id));
        let oracle_text_path =
            oracle_case_artifact_path(&report_dir, &case.arxiv_id, &case.version, "oracle.txt");
        fs::write(oracle_text_path.as_std_path(), &oracle_text)
            .unwrap_or_else(|error| panic!("{} write oracle text failed: {error}", case.arxiv_id));
        let oracle_page_count = extract_pdf_page_count(&pdfinfo, &oracle_pdf)
            .unwrap_or_else(|error| panic!("{} oracle pdfinfo failed: {error}", case.arxiv_id));
        let oracle_raster_prefix = oracle_case_first_page_raster_prefix(
            &report_dir,
            &case.arxiv_id,
            &case.version,
            "oracle",
        );
        let oracle_first_page_raster =
            rasterize_pdf_first_page(&pdftoppm, &oracle_pdf, &oracle_raster_prefix).unwrap_or_else(
                |error| panic!("{} oracle first-page raster failed: {error}", case.arxiv_id),
            );
        let oracle_first_page_raster_smoke = extract_raster_smoke(&oracle_first_page_raster)
            .unwrap_or_else(|error| {
                panic!("{} oracle raster smoke failed: {error}", case.arxiv_id)
            });
        let oracle_tokens = tokenize(&oracle_text);
        let oracle_normalized_tokens = tokenize_normalized(&oracle_text);
        assert!(
            oracle_tokens.len() >= case.min_oracle_tokens,
            "{} official PDF text extraction produced only {} tokens",
            case.arxiv_id,
            oracle_tokens.len()
        );
        let oracle_unique = unique_tokens(&oracle_tokens);
        let oracle_normalized_unique = unique_tokens(&oracle_normalized_tokens);
        let tempdir = tempdir().expect("tempdir");
        let project_root =
            Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 project root");
        copy_dir(&source_root, &project_root);
        fs::write(
            project_root.join("00README.yaml").as_std_path(),
            format!("compiler: pdf_latex\ntoplevel:\n  - {}\n", case.toplevel),
        )
        .expect("write latexd manifest override");

        let world = ProjectWorld::load(project_root.clone()).expect("load corpus project");
        let build_root = project_root.join(".latexd/build");
        let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
        let rev = 1;
        let compile_result = driver
            .compile(CompileRequest {
                root: project_root.clone(),
                manifest: world.manifest.clone(),
                toplevel: case.toplevel.clone(),
                rev,
                build_root: build_root.clone(),
                changed_files: vec![case.toplevel.clone()],
            })
            .await;

        let mut report = OracleCaseReport {
            arxiv_id: case.arxiv_id.clone(),
            version: case.version.clone(),
            title: case.title.clone(),
            license: case.license.clone(),
            source_url: case.source_url.clone(),
            pdf_url: case.pdf_url.clone(),
            toplevel: case.toplevel.clone(),
            oracle_pdf: oracle_pdf.clone(),
            oracle_text: oracle_text_path,
            oracle_page_count,
            oracle_first_page_raster,
            oracle_first_page_raster_smoke,
            max_page_count_delta: case.max_page_count_delta,
            min_first_page_ink_ratio: case.min_first_page_ink_ratio,
            source_root: source_root.clone(),
            oracle_token_count: oracle_tokens.len(),
            oracle_unique_token_count: oracle_unique.len(),
            oracle_normalized_token_count: oracle_normalized_tokens.len(),
            oracle_normalized_unique_token_count: oracle_normalized_unique.len(),
            internal_token_count: None,
            internal_unique_token_count: None,
            common_unique_token_count: None,
            common_unique_token_ratio: None,
            internal_normalized_token_count: None,
            internal_normalized_unique_token_count: None,
            normalized_common_unique_token_count: None,
            normalized_common_unique_token_ratio: None,
            ir_structure_slices: Vec::new(),
            missing_token_sample: Vec::new(),
            extra_token_sample: Vec::new(),
            normalized_missing_token_sample: Vec::new(),
            normalized_extra_token_sample: Vec::new(),
            metric_findings: Vec::new(),
            internal_text: None,
            internal_pdf: None,
            internal_document_ir: None,
            display_list_render: None,
            internal_page_count: None,
            page_count_delta: None,
            page_count_within_tolerance: None,
            internal_first_page_raster: None,
            internal_first_page_raster_smoke: None,
            first_page_raster_gross: None,
            first_page_raster_diff: None,
            first_page_raster_diff_metrics: None,
            internal_build_failure: None,
            internal_diagnostics: Vec::new(),
        };

        match compile_result {
            Ok(outcome) => {
                report.internal_diagnostics = outcome
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.clone())
                    .collect();
                let internal_text =
                    extract_pdf_text(&pdftotext, &outcome.pdf_path).unwrap_or_else(|error| {
                        panic!("{} internal pdftotext failed: {error}", case.arxiv_id)
                    });
                let internal_text_path = oracle_case_artifact_path(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "internal.txt",
                );
                fs::write(internal_text_path.as_std_path(), &internal_text).unwrap_or_else(
                    |error| panic!("{} write internal text failed: {error}", case.arxiv_id),
                );
                let internal_pdf_path = oracle_case_artifact_path(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "internal.pdf",
                );
                fs::copy(
                    outcome.pdf_path.as_std_path(),
                    internal_pdf_path.as_std_path(),
                )
                .unwrap_or_else(|error| {
                    panic!("{} copy internal PDF failed: {error}", case.arxiv_id)
                });
                let internal_document_ir_artifact = render_ir_document_path(&build_root, rev);
                if internal_document_ir_artifact.exists() {
                    let internal_document_ir_path = oracle_case_artifact_path(
                        &report_dir,
                        &case.arxiv_id,
                        &case.version,
                        "internal-document-ir.json",
                    );
                    fs::copy(
                        internal_document_ir_artifact.as_std_path(),
                        internal_document_ir_path.as_std_path(),
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{} copy internal document IR failed: {error}",
                            case.arxiv_id
                        )
                    });
                    let document_ir =
                        read_document_ir(&internal_document_ir_path).unwrap_or_else(|error| {
                            panic!(
                                "{} read internal document IR failed: {error}",
                                case.arxiv_id
                            )
                        });
                    report.ir_structure_slices = build_structure_slice_reports(
                        &document_ir,
                        &source_root,
                        &oracle_unique,
                        &oracle_normalized_unique,
                    );
                    report.internal_document_ir = Some(internal_document_ir_path);
                }
                let display_list_pdf_artifact = render_ir_display_list_pdf_path(&build_root, rev);
                assert!(
                    display_list_pdf_artifact.exists(),
                    "{} display-list PDF artifact was not written",
                    case.arxiv_id
                );
                let display_list_pdf_path = oracle_case_artifact_path(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "display-list.pdf",
                );
                fs::copy(
                    display_list_pdf_artifact.as_std_path(),
                    display_list_pdf_path.as_std_path(),
                )
                .unwrap_or_else(|error| {
                    panic!("{} copy display-list PDF failed: {error}", case.arxiv_id)
                });
                let display_list_text = extract_pdf_text(&pdftotext, &display_list_pdf_artifact)
                    .unwrap_or_else(|error| {
                        panic!("{} display-list pdftotext failed: {error}", case.arxiv_id)
                    });
                let display_list_text_path = oracle_case_artifact_path(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "display-list.txt",
                );
                fs::write(display_list_text_path.as_std_path(), &display_list_text).unwrap_or_else(
                    |error| panic!("{} write display-list text failed: {error}", case.arxiv_id),
                );
                let display_list_page_count =
                    extract_pdf_page_count(&pdfinfo, &display_list_pdf_artifact).unwrap_or_else(
                        |error| panic!("{} display-list pdfinfo failed: {error}", case.arxiv_id),
                    );
                let display_list_raster_prefix = oracle_case_first_page_raster_prefix(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "display-list",
                );
                let display_list_first_page_raster = rasterize_pdf_first_page(
                    &pdftoppm,
                    &display_list_pdf_artifact,
                    &display_list_raster_prefix,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} display-list first-page raster failed: {error}",
                        case.arxiv_id
                    )
                });
                let display_list_first_page_raster_smoke =
                    extract_raster_smoke(&display_list_first_page_raster).unwrap_or_else(|error| {
                        panic!(
                            "{} display-list raster smoke failed: {error}",
                            case.arxiv_id
                        )
                    });
                let display_list_first_page_raster_diff = oracle_case_artifact_path(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "display-list-first-page-raster-diff.png",
                );
                let display_list_first_page_raster_diff_metrics = write_raster_diff_image(
                    &report.oracle_first_page_raster,
                    &display_list_first_page_raster,
                    &display_list_first_page_raster_diff,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} write display-list first-page raster diff failed: {error}",
                        case.arxiv_id
                    )
                });
                let display_list_tokens = tokenize(&display_list_text);
                let display_list_normalized_tokens = tokenize_normalized(&display_list_text);
                let display_list_unique = unique_tokens(&display_list_tokens);
                let display_list_normalized_unique = unique_tokens(&display_list_normalized_tokens);
                let display_list_common = oracle_unique
                    .intersection(&display_list_unique)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let display_list_common_ratio =
                    display_list_common.len() as f64 / oracle_unique.len().max(1) as f64;
                let display_list_normalized_common = oracle_normalized_unique
                    .intersection(&display_list_normalized_unique)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let display_list_normalized_common_ratio = display_list_normalized_common.len()
                    as f64
                    / oracle_normalized_unique.len().max(1) as f64;
                let display_list_page_count_delta =
                    display_list_page_count as i64 - oracle_page_count as i64;
                report.display_list_render = Some(OracleRenderPathReport {
                    pdf: display_list_pdf_path,
                    text: display_list_text_path,
                    token_count: display_list_tokens.len(),
                    unique_token_count: display_list_unique.len(),
                    common_unique_token_count: display_list_common.len(),
                    common_unique_token_ratio: display_list_common_ratio,
                    normalized_token_count: display_list_normalized_tokens.len(),
                    normalized_unique_token_count: display_list_normalized_unique.len(),
                    normalized_common_unique_token_count: display_list_normalized_common.len(),
                    normalized_common_unique_token_ratio: display_list_normalized_common_ratio,
                    missing_token_sample: ordered_difference_sample(
                        &oracle_tokens,
                        &display_list_common,
                        80,
                    ),
                    extra_token_sample: ordered_difference_sample(
                        &display_list_tokens,
                        &oracle_unique,
                        80,
                    ),
                    normalized_missing_token_sample: ordered_difference_sample(
                        &oracle_normalized_tokens,
                        &display_list_normalized_common,
                        80,
                    ),
                    normalized_extra_token_sample: ordered_difference_sample(
                        &display_list_normalized_tokens,
                        &oracle_normalized_unique,
                        80,
                    ),
                    page_count: display_list_page_count,
                    page_count_delta: display_list_page_count_delta,
                    page_count_within_tolerance: page_count_within_tolerance(
                        oracle_page_count,
                        display_list_page_count,
                        case.max_page_count_delta,
                    ),
                    first_page_raster: display_list_first_page_raster,
                    first_page_raster_gross: compare_raster_smoke(
                        &report.oracle_first_page_raster_smoke,
                        &display_list_first_page_raster_smoke,
                        case.min_first_page_ink_ratio,
                    ),
                    first_page_raster_smoke: display_list_first_page_raster_smoke,
                    first_page_raster_diff: display_list_first_page_raster_diff,
                    first_page_raster_diff_metrics: display_list_first_page_raster_diff_metrics,
                });
                let internal_page_count = extract_pdf_page_count(&pdfinfo, &outcome.pdf_path)
                    .unwrap_or_else(|error| {
                        panic!("{} internal pdfinfo failed: {error}", case.arxiv_id)
                    });
                let internal_raster_prefix = oracle_case_first_page_raster_prefix(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "internal",
                );
                let internal_first_page_raster =
                    rasterize_pdf_first_page(&pdftoppm, &outcome.pdf_path, &internal_raster_prefix)
                        .unwrap_or_else(|error| {
                            panic!(
                                "{} internal first-page raster failed: {error}",
                                case.arxiv_id
                            )
                        });
                let internal_first_page_raster_smoke =
                    extract_raster_smoke(&internal_first_page_raster).unwrap_or_else(|error| {
                        panic!("{} internal raster smoke failed: {error}", case.arxiv_id)
                    });
                let first_page_raster_diff = oracle_case_artifact_path(
                    &report_dir,
                    &case.arxiv_id,
                    &case.version,
                    "first-page-raster-diff.png",
                );
                let first_page_raster_diff_metrics = write_raster_diff_image(
                    &report.oracle_first_page_raster,
                    &internal_first_page_raster,
                    &first_page_raster_diff,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} write first-page raster diff failed: {error}",
                        case.arxiv_id
                    )
                });
                let internal_tokens = tokenize(&internal_text);
                let internal_normalized_tokens = tokenize_normalized(&internal_text);
                let internal_unique = unique_tokens(&internal_tokens);
                let internal_normalized_unique = unique_tokens(&internal_normalized_tokens);
                let common = oracle_unique
                    .intersection(&internal_unique)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let ratio = common.len() as f64 / oracle_unique.len().max(1) as f64;
                let normalized_common = oracle_normalized_unique
                    .intersection(&internal_normalized_unique)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let normalized_ratio =
                    normalized_common.len() as f64 / oracle_normalized_unique.len().max(1) as f64;
                report.internal_token_count = Some(internal_tokens.len());
                report.internal_unique_token_count = Some(internal_unique.len());
                report.common_unique_token_count = Some(common.len());
                report.common_unique_token_ratio = Some(ratio);
                report.internal_normalized_token_count = Some(internal_normalized_tokens.len());
                report.internal_normalized_unique_token_count =
                    Some(internal_normalized_unique.len());
                report.normalized_common_unique_token_count = Some(normalized_common.len());
                report.normalized_common_unique_token_ratio = Some(normalized_ratio);
                report.missing_token_sample =
                    ordered_difference_sample(&oracle_tokens, &common, 80);
                report.extra_token_sample =
                    ordered_difference_sample(&internal_tokens, &oracle_unique, 80);
                report.normalized_missing_token_sample =
                    ordered_difference_sample(&oracle_normalized_tokens, &normalized_common, 80);
                report.normalized_extra_token_sample = ordered_difference_sample(
                    &internal_normalized_tokens,
                    &oracle_normalized_unique,
                    80,
                );
                report.internal_text = Some(internal_text_path);
                report.internal_pdf = Some(internal_pdf_path);
                report.internal_page_count = Some(internal_page_count);
                report.page_count_delta =
                    Some(internal_page_count as i64 - oracle_page_count as i64);
                report.page_count_within_tolerance = Some(page_count_within_tolerance(
                    oracle_page_count,
                    internal_page_count,
                    case.max_page_count_delta,
                ));
                report.internal_first_page_raster = Some(internal_first_page_raster);
                report.first_page_raster_gross = Some(compare_raster_smoke(
                    &report.oracle_first_page_raster_smoke,
                    &internal_first_page_raster_smoke,
                    case.min_first_page_ink_ratio,
                ));
                report.internal_first_page_raster_smoke = Some(internal_first_page_raster_smoke);
                report.first_page_raster_diff = Some(first_page_raster_diff);
                report.first_page_raster_diff_metrics = Some(first_page_raster_diff_metrics);
                let raster_status = report
                    .first_page_raster_gross
                    .as_ref()
                    .map(|raster_report| raster_report.status);
                report.metric_findings = classify_oracle_metric_findings(
                    case.min_internal_tokens,
                    case.min_common_token_ratio,
                    report.internal_token_count,
                    report.common_unique_token_ratio,
                    report.normalized_common_unique_token_ratio,
                    report.page_count_within_tolerance,
                    raster_status,
                    false,
                );
                if strict {
                    assert!(
                        internal_tokens.len() >= case.min_internal_tokens,
                        "{} internal PDF text extraction produced only {} tokens",
                        case.arxiv_id,
                        internal_tokens.len()
                    );
                    assert!(
                        ratio >= case.min_common_token_ratio,
                        "{} common unique token ratio {ratio:.4} was below {:.4}",
                        case.arxiv_id,
                        case.min_common_token_ratio
                    );
                    assert!(
                        report.page_count_within_tolerance == Some(true),
                        "{} page count delta {:?} exceeded tolerance {}",
                        case.arxiv_id,
                        report.page_count_delta,
                        case.max_page_count_delta
                    );
                    assert_eq!(
                        report
                            .first_page_raster_gross
                            .as_ref()
                            .expect("raster gross report")
                            .status,
                        RasterGrossStatus::Pass,
                        "{} first-page raster gross smoke failed",
                        case.arxiv_id
                    );
                }
            }
            Err(error) => {
                report.internal_build_failure = Some(error.to_string());
                report.internal_diagnostics = error
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.clone())
                    .collect();
                if strict {
                    panic!("{} internal build failed: {error:?}", case.arxiv_id);
                }
                report.metric_findings = classify_oracle_metric_findings(
                    case.min_internal_tokens,
                    case.min_common_token_ratio,
                    report.internal_token_count,
                    report.common_unique_token_ratio,
                    report.normalized_common_unique_token_ratio,
                    report.page_count_within_tolerance,
                    report
                        .first_page_raster_gross
                        .as_ref()
                        .map(|raster_report| raster_report.status),
                    true,
                );
            }
        }
        reports.push(report);
    }

    assert!(
        !reports.is_empty(),
        "no arXiv oracle cases were found in {}",
        corpus_root
    );
    let report = OracleReport {
        corpus_root,
        strict,
        cases: reports,
    };
    let report_file = env::var("LATEXD_ARXIV_ORACLE_REPORT_FILE")
        .unwrap_or_else(|_| "cc0-smoke-report.json".to_string());
    let report_path = report_dir.join(report_file);
    fs::write(
        report_path.as_std_path(),
        serde_json::to_vec_pretty(&report).expect("serialize arXiv oracle report"),
    )
    .expect("write arXiv oracle report");
    eprintln!("wrote arXiv oracle report to {report_path}");
}

fn locate_case_files(
    corpus_root: &Utf8Path,
    case: &OracleCase,
) -> Option<(Utf8PathBuf, Utf8PathBuf)> {
    let source_root = corpus_root.join("sources").join(&case.arxiv_id);
    let oracle_pdf = corpus_root
        .join("pdfs")
        .join(format!("{}.pdf", case.arxiv_id));
    let raw_pdf = corpus_root
        .join("raw")
        .join(format!("{}.pdf", case.arxiv_id));
    let oracle_pdf = if oracle_pdf.exists() {
        oracle_pdf
    } else if raw_pdf.exists() {
        raw_pdf
    } else {
        return None;
    };
    if source_root.join(&case.toplevel).exists() {
        Some((source_root, oracle_pdf))
    } else {
        None
    }
}

fn render_ir_document_path(build_root: &Utf8Path, rev: u64) -> Utf8PathBuf {
    build_root.join(format!("rev-{rev}/render-ir/document-ir.json"))
}

fn render_ir_display_list_pdf_path(build_root: &Utf8Path, rev: u64) -> Utf8PathBuf {
    build_root.join(format!("rev-{rev}/render-ir/display-list.pdf"))
}

fn read_document_ir(path: &Utf8Path) -> anyhow::Result<DocumentIr> {
    let bytes = fs::read(path.as_std_path())?;
    serde_json::from_slice::<DocumentIr>(&bytes)
        .map_err(|error| anyhow::anyhow!("failed to parse document IR {path}: {error}"))
}

#[derive(Debug, Clone, Default)]
struct OracleStructureSliceText {
    text: String,
    sources: Vec<SourceProvenance>,
}

fn build_structure_slice_reports(
    document_ir: &DocumentIr,
    source_root: &Utf8Path,
    oracle_unique: &BTreeSet<String>,
    oracle_normalized_unique: &BTreeSet<String>,
) -> Vec<OracleStructureSliceReport> {
    let mut source_cache = BTreeMap::<Utf8PathBuf, Option<String>>::new();
    collect_structure_slice_texts(document_ir)
        .into_iter()
        .filter_map(|(kind, slice)| {
            let text = slice.text;
            let tokens = tokenize(&text);
            let normalized_tokens = tokenize_normalized(&text);
            if tokens.is_empty() && normalized_tokens.is_empty() {
                return None;
            }

            let unique = unique_tokens(&tokens);
            let normalized_unique = unique_tokens(&normalized_tokens);
            let common = unique
                .intersection(oracle_unique)
                .cloned()
                .collect::<BTreeSet<_>>();
            let normalized_common = normalized_unique
                .intersection(oracle_normalized_unique)
                .cloned()
                .collect::<BTreeSet<_>>();
            let extra = unique
                .difference(oracle_unique)
                .cloned()
                .collect::<BTreeSet<_>>();
            let normalized_extra = normalized_unique
                .difference(oracle_normalized_unique)
                .cloned()
                .collect::<BTreeSet<_>>();
            let mut source_text = String::new();
            let mut append_source_span_text = |span: &SourceSpan| {
                let path = if span.path.is_absolute() {
                    span.path.clone()
                } else {
                    source_root.join(&span.path)
                };
                let contents = source_cache
                    .entry(path.clone())
                    .or_insert_with(|| fs::read_to_string(path.as_std_path()).ok());
                let Some(contents) = contents.as_deref() else {
                    return;
                };
                let Some(fragment) = contents.get(span.start_utf8 as usize..span.end_utf8 as usize)
                else {
                    return;
                };
                if !source_text.is_empty() {
                    source_text.push('\n');
                }
                source_text.push_str(fragment);
            };
            for source in &slice.sources {
                if let ProvenanceSpan::File(span) = &source.primary {
                    append_source_span_text(span);
                }
                for related in &source.related {
                    if let ProvenanceSpan::File(span) = &related.span {
                        append_source_span_text(span);
                    }
                }
                for frame in &source.expansion_stack {
                    if let ProvenanceSpan::File(span) = &frame.call_span {
                        append_source_span_text(span);
                    }
                    if let Some(ProvenanceSpan::File(span)) = &frame.definition_span {
                        append_source_span_text(span);
                    }
                }
            }
            let source_unique = unique_tokens(&tokenize(&source_text));
            let source_normalized_unique = unique_tokens(&tokenize_normalized(&source_text));
            let mut source_compact_alnum = String::new();
            let mut source_compact_digits = String::new();
            for character in source_text.chars() {
                if character.is_alphanumeric() {
                    source_compact_alnum.extend(character.to_lowercase());
                }
                if character.is_ascii_digit() {
                    source_compact_digits.push(character);
                }
            }
            let token_has_source_evidence = |token: &str, source_unique: &BTreeSet<String>| {
                source_unique.contains(token)
                    || source_compact_alnum.contains(token)
                    || (token.chars().all(|character| character.is_ascii_digit())
                        && source_compact_digits.contains(token))
            };
            let source_backed_extra = extra
                .iter()
                .filter(|token| token_has_source_evidence(token, &source_unique))
                .cloned()
                .collect::<BTreeSet<_>>();
            let normalized_source_backed_extra = normalized_extra
                .iter()
                .filter(|token| token_has_source_evidence(token, &source_normalized_unique))
                .cloned()
                .collect::<BTreeSet<_>>();
            let ordered_sample_from = |tokens: &[String], members: &BTreeSet<String>| {
                let mut seen = BTreeSet::new();
                let mut sample = Vec::new();
                for token in tokens {
                    if !members.contains(token) || !seen.insert(token.clone()) {
                        continue;
                    }
                    sample.push(token.clone());
                    if sample.len() >= 40 {
                        break;
                    }
                }
                sample
            };
            let source_unbacked_extra = extra
                .difference(&source_backed_extra)
                .cloned()
                .collect::<BTreeSet<_>>();
            let normalized_source_unbacked_extra = normalized_extra
                .difference(&normalized_source_backed_extra)
                .cloned()
                .collect::<BTreeSet<_>>();

            Some(OracleStructureSliceReport {
                kind,
                token_count: tokens.len(),
                unique_token_count: unique.len(),
                common_unique_token_count: common.len(),
                common_unique_token_ratio: common.len() as f64 / unique.len().max(1) as f64,
                oracle_unique_token_coverage_ratio: common.len() as f64
                    / oracle_unique.len().max(1) as f64,
                normalized_token_count: normalized_tokens.len(),
                normalized_unique_token_count: normalized_unique.len(),
                normalized_common_unique_token_count: normalized_common.len(),
                normalized_common_unique_token_ratio: normalized_common.len() as f64
                    / normalized_unique.len().max(1) as f64,
                normalized_oracle_unique_token_coverage_ratio: normalized_common.len() as f64
                    / oracle_normalized_unique.len().max(1) as f64,
                source_backed_extra_token_count: source_backed_extra.len(),
                source_backed_extra_token_ratio: (!extra.is_empty())
                    .then_some(source_backed_extra.len() as f64 / extra.len() as f64),
                normalized_source_backed_extra_token_count: normalized_source_backed_extra.len(),
                normalized_source_backed_extra_token_ratio: (!normalized_extra.is_empty())
                    .then_some(
                        normalized_source_backed_extra.len() as f64 / normalized_extra.len() as f64,
                    ),
                extra_token_sample: ordered_difference_sample(&tokens, oracle_unique, 40),
                normalized_extra_token_sample: ordered_difference_sample(
                    &normalized_tokens,
                    oracle_normalized_unique,
                    40,
                ),
                source_backed_extra_token_sample: ordered_sample_from(
                    &tokens,
                    &source_backed_extra,
                ),
                normalized_source_backed_extra_token_sample: ordered_sample_from(
                    &normalized_tokens,
                    &normalized_source_backed_extra,
                ),
                source_unbacked_extra_token_sample: ordered_sample_from(
                    &tokens,
                    &source_unbacked_extra,
                ),
                normalized_source_unbacked_extra_token_sample: ordered_sample_from(
                    &normalized_tokens,
                    &normalized_source_unbacked_extra,
                ),
            })
        })
        .collect()
}

fn collect_structure_slice_texts(
    document_ir: &DocumentIr,
) -> BTreeMap<OracleStructureSliceKind, OracleStructureSliceText> {
    let fallback_text = |fallback: &RawFallbackIr| {
        fallback
            .normalized_visible_text
            .as_deref()
            .unwrap_or(&fallback.source_excerpt)
            .to_string()
    };
    let inline_nodes_text = |nodes: &[InlineNode]| {
        let mut text = String::new();
        for node in nodes {
            match node {
                InlineNode::Text { text: value, .. } => text.push_str(value),
                InlineNode::Space { .. } => text.push(' '),
                InlineNode::LineBreak { .. } => text.push('\n'),
                InlineNode::Citation(citation) => text.push_str(&citation.display_text),
                InlineNode::Reference(reference) => text.push_str(&reference.display_text),
                InlineNode::Link(link) => text.push_str(&link.display_text),
                InlineNode::InlineMath {
                    raw_source,
                    normalized_text,
                    ..
                } => text.push_str(normalized_text.as_deref().unwrap_or(raw_source)),
                InlineNode::RawFallback(fallback) => text.push_str(&fallback_text(fallback)),
            }
        }
        text
    };
    let title_block_text = |block: &TitleBlock| {
        let mut lines = Vec::new();
        if let Some(title) = &block.title {
            lines.push(title.clone());
        }
        lines.extend(block.authors.iter().cloned());
        if let Some(date) = &block.date {
            lines.push(date.clone());
        }
        lines.extend(block.keywords.iter().cloned());
        lines.extend(block.pacs.iter().cloned());
        lines.join("\n")
    };
    let bibliography_text = |block: &BibliographyBlock| {
        block
            .items
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let list_text = |block: &ListBlock| {
        block
            .items
            .iter()
            .map(|item| {
                let mut text = String::new();
                text.push_str(&item.marker);
                text.push(' ');
                text.push_str(&inline_nodes_text(&item.content));
                text
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let table_rows_text = |block: &TableBlock| {
        block
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut slices = BTreeMap::<OracleStructureSliceKind, OracleStructureSliceText>::new();
    let mut append_slice =
        |kind: OracleStructureSliceKind, text: String, sources: Vec<SourceProvenance>| {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            let entry = slices.entry(kind).or_default();
            if !entry.text.is_empty() {
                entry.text.push('\n');
            }
            entry.text.push_str(text);
            entry.sources.extend(sources);
        };

    let mut pending_blocks = document_ir.blocks.iter().rev().collect::<Vec<_>>();
    while let Some(block) = pending_blocks.pop() {
        match block {
            IrBlock::TitleBlock(block) => {
                let mut sources = vec![block.source.clone()];
                if let Some(source) = &block.title_source {
                    sources.push(source.clone());
                }
                sources.extend(block.author_sources.iter().cloned());
                if let Some(source) = &block.date_source {
                    sources.push(source.clone());
                }
                sources.extend(block.keyword_sources.iter().cloned());
                sources.extend(block.pacs_sources.iter().cloned());
                append_slice(
                    OracleStructureSliceKind::FrontMatter,
                    title_block_text(block),
                    sources,
                );
            }
            IrBlock::Abstract(AbstractBlock {
                content, source, ..
            }) => {
                append_slice(
                    OracleStructureSliceKind::Abstract,
                    inline_nodes_text(content),
                    vec![source.clone()],
                );
            }
            IrBlock::Heading(HeadingBlock {
                number,
                content,
                source,
                ..
            }) => {
                let mut text = String::new();
                if let Some(number) = number {
                    text.push_str(number);
                    text.push(' ');
                }
                text.push_str(&inline_nodes_text(content));
                append_slice(OracleStructureSliceKind::Body, text, vec![source.clone()]);
            }
            IrBlock::Paragraph(ParagraphBlock {
                content, source, ..
            })
            | IrBlock::Environment(EnvironmentBlock {
                content, source, ..
            }) => {
                append_slice(
                    OracleStructureSliceKind::Body,
                    inline_nodes_text(content),
                    vec![source.clone()],
                );
            }
            IrBlock::LayoutContainer(block) => {
                pending_blocks.extend(block.children.iter().rev());
            }
            IrBlock::List(block) => {
                append_slice(
                    OracleStructureSliceKind::Body,
                    list_text(block),
                    vec![block.source.clone()],
                );
            }
            IrBlock::DisplayMath(block) => {
                append_slice(
                    OracleStructureSliceKind::Body,
                    block
                        .normalized_text
                        .as_deref()
                        .unwrap_or(&block.raw_source)
                        .to_string(),
                    vec![block.source.clone()],
                );
            }
            IrBlock::Bibliography(block) => {
                let mut sources = vec![block.source.clone()];
                sources.extend(block.items.iter().map(|item| item.source.clone()));
                append_slice(
                    OracleStructureSliceKind::References,
                    bibliography_text(block),
                    sources,
                );
            }
            IrBlock::Graphic(GraphicBlock {
                caption,
                caption_source,
                source,
                ..
            }) => {
                if let Some(caption) = caption {
                    append_slice(
                        OracleStructureSliceKind::Caption,
                        caption.clone(),
                        vec![caption_source.clone().unwrap_or_else(|| source.clone())],
                    );
                }
            }
            IrBlock::Table(block) => {
                if let Some(caption) = &block.caption {
                    append_slice(
                        OracleStructureSliceKind::Caption,
                        caption.clone(),
                        vec![
                            block
                                .caption_source
                                .clone()
                                .unwrap_or_else(|| block.source.clone()),
                        ],
                    );
                }
                append_slice(
                    OracleStructureSliceKind::Table,
                    table_rows_text(block),
                    vec![block.source.clone()],
                );
            }
            IrBlock::RawFallback(block) => {
                append_slice(
                    OracleStructureSliceKind::Fallback,
                    fallback_text(block),
                    vec![block.source.clone()],
                );
            }
        }
    }

    slices
}

fn extract_pdf_text(pdftotext: &std::path::Path, pdf_path: &Utf8Path) -> anyhow::Result<String> {
    let output = Command::new(pdftotext)
        .args(["-layout", "-enc", "UTF-8", pdf_path.as_str(), "-"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "pdftotext exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn extract_pdf_page_count(pdfinfo: &std::path::Path, pdf_path: &Utf8Path) -> anyhow::Result<usize> {
    let output = Command::new(pdfinfo).arg(pdf_path.as_str()).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "pdfinfo exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_pdfinfo_page_count(stdout.as_ref())
}

fn parse_pdfinfo_page_count(output: &str) -> anyhow::Result<usize> {
    for line in output.lines() {
        let Some(value) = line.strip_prefix("Pages:") else {
            continue;
        };
        return value.trim().parse::<usize>().map_err(|error| {
            anyhow::anyhow!("invalid pdfinfo Pages value {:?}: {error}", value.trim())
        });
    }
    anyhow::bail!("pdfinfo output did not contain Pages")
}

fn rasterize_pdf_first_page(
    pdftoppm: &std::path::Path,
    pdf_path: &Utf8Path,
    output_prefix: &Utf8Path,
) -> anyhow::Result<Utf8PathBuf> {
    let output = Command::new(pdftoppm)
        .args([
            "-png",
            "-singlefile",
            "-f",
            "1",
            "-l",
            "1",
            "-r",
            "72",
            pdf_path.as_str(),
            output_prefix.as_str(),
        ])
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "pdftoppm exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output_path = rasterized_singlefile_png_path(output_prefix);
    if !output_path.exists() {
        anyhow::bail!("pdftoppm did not write expected output {output_path}");
    }
    Ok(output_path)
}

fn rasterized_singlefile_png_path(output_prefix: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from(format!("{output_prefix}.png"))
}

fn extract_raster_smoke(path: &Utf8Path) -> anyhow::Result<RasterSmokeReport> {
    let bytes = fs::read(path.as_std_path())?;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|error| anyhow::anyhow!("failed to decode PNG {}: {error}", path))?
        .into_rgba8();
    let (width, height) = image.dimensions();
    raster_smoke_from_rgba(width, height, image.into_raw())
}

fn write_raster_diff_image(
    oracle_path: &Utf8Path,
    internal_path: &Utf8Path,
    output_path: &Utf8Path,
) -> anyhow::Result<RasterDiffReport> {
    let oracle_bytes = fs::read(oracle_path.as_std_path())?;
    let oracle_image = image::load_from_memory_with_format(&oracle_bytes, image::ImageFormat::Png)
        .map_err(|error| anyhow::anyhow!("failed to decode oracle PNG {}: {error}", oracle_path))?
        .into_rgba8();
    let internal_bytes = fs::read(internal_path.as_std_path())?;
    let internal_image =
        image::load_from_memory_with_format(&internal_bytes, image::ImageFormat::Png)
            .map_err(|error| {
                anyhow::anyhow!("failed to decode internal PNG {}: {error}", internal_path)
            })?
            .into_rgba8();
    let (oracle_width, oracle_height) = oracle_image.dimensions();
    let (internal_width, internal_height) = internal_image.dimensions();
    let width = oracle_width.max(internal_width);
    let height = oracle_height.max(internal_height);
    let mut diff_rgba = Vec::with_capacity(width as usize * height as usize * 4);
    let mut differing_pixel_count = 0_u64;
    let mut overlapping_differing_pixel_count = 0_u64;
    let mut oracle_only_pixel_count = 0_u64;
    let mut internal_only_pixel_count = 0_u64;
    let mut differing_min_x = width;
    let mut differing_min_y = height;
    let mut differing_max_x = 0_u32;
    let mut differing_max_y = 0_u32;

    for y in 0..height {
        for x in 0..width {
            let oracle_pixel =
                (x < oracle_width && y < oracle_height).then(|| oracle_image.get_pixel(x, y).0);
            let internal_pixel = (x < internal_width && y < internal_height)
                .then(|| internal_image.get_pixel(x, y).0);
            let (diff_pixel, differs) = match (oracle_pixel, internal_pixel) {
                (Some(oracle), Some(internal)) => {
                    let channel_diff = oracle
                        .iter()
                        .zip(internal.iter())
                        .map(|(left, right)| left.abs_diff(*right))
                        .max()
                        .unwrap_or(0);
                    if channel_diff == 0 {
                        ([255, 255, 255, 255], false)
                    } else {
                        let intensity = channel_diff.max(32);
                        (
                            [
                                255,
                                255_u8.saturating_sub(intensity),
                                255_u8.saturating_sub(intensity),
                                255,
                            ],
                            true,
                        )
                    }
                }
                (Some(_), None) => {
                    oracle_only_pixel_count += 1;
                    ([255, 0, 255, 255], true)
                }
                (None, Some(_)) => {
                    internal_only_pixel_count += 1;
                    ([0, 0, 255, 255], true)
                }
                (None, None) => ([255, 255, 255, 255], false),
            };
            if differs {
                differing_pixel_count += 1;
                differing_min_x = differing_min_x.min(x);
                differing_min_y = differing_min_y.min(y);
                differing_max_x = differing_max_x.max(x);
                differing_max_y = differing_max_y.max(y);
                if oracle_pixel.is_some() && internal_pixel.is_some() {
                    overlapping_differing_pixel_count += 1;
                }
            }
            diff_rgba.extend_from_slice(&diff_pixel);
        }
    }

    let diff_image = image::RgbaImage::from_raw(width, height, diff_rgba)
        .expect("diff buffer should match image dimensions");
    diff_image.save_with_format(output_path.as_std_path(), image::ImageFormat::Png)?;
    let total_pixel_count = (width as u64 * height as u64).max(1);
    let differing_pixel_bbox = (differing_pixel_count > 0).then(|| RasterBoundingBox {
        x: differing_min_x,
        y: differing_min_y,
        width: differing_max_x - differing_min_x + 1,
        height: differing_max_y - differing_min_y + 1,
    });
    Ok(RasterDiffReport {
        width_px: width,
        height_px: height,
        differing_pixel_count,
        differing_pixel_bbox,
        overlapping_differing_pixel_count,
        oracle_only_pixel_count,
        internal_only_pixel_count,
        differing_pixel_ratio: differing_pixel_count as f64 / total_pixel_count as f64,
    })
}

fn raster_smoke_from_rgba(
    width_px: u32,
    height_px: u32,
    rgba: Vec<u8>,
) -> anyhow::Result<RasterSmokeReport> {
    let expected_len = width_px as usize * height_px as usize * 4;
    if rgba.len() != expected_len {
        anyhow::bail!(
            "RGBA buffer length {} did not match expected {} for {}x{} image",
            rgba.len(),
            expected_len,
            width_px,
            height_px
        );
    }

    let mut min_x = width_px;
    let mut min_y = height_px;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;
    let mut non_white_pixel_count = 0_u64;
    for y in 0..height_px {
        for x in 0..width_px {
            let offset = (y as usize * width_px as usize + x as usize) * 4;
            let red = rgba[offset];
            let green = rgba[offset + 1];
            let blue = rgba[offset + 2];
            let alpha = rgba[offset + 3];
            if alpha == 0 || (red >= 250 && green >= 250 && blue >= 250) {
                continue;
            }
            found = true;
            non_white_pixel_count += 1;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    Ok(RasterSmokeReport {
        width_px,
        height_px,
        non_white_pixel_count,
        non_white_bbox: found.then_some(RasterBoundingBox {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        }),
    })
}

fn page_count_within_tolerance(
    oracle_page_count: usize,
    internal_page_count: usize,
    max_page_count_delta: usize,
) -> bool {
    oracle_page_count.abs_diff(internal_page_count) <= max_page_count_delta
}

fn compare_raster_smoke(
    oracle: &RasterSmokeReport,
    internal: &RasterSmokeReport,
    min_internal_to_oracle_ink_ratio: f64,
) -> RasterGrossReport {
    let page_size_matches =
        oracle.width_px == internal.width_px && oracle.height_px == internal.height_px;
    let oracle_ink_bbox_area_px = oracle
        .non_white_bbox
        .as_ref()
        .map(|bbox| bbox.width as u64 * bbox.height as u64);
    let internal_ink_bbox_area_px = internal
        .non_white_bbox
        .as_ref()
        .map(|bbox| bbox.width as u64 * bbox.height as u64);
    let internal_to_oracle_ink_bbox_ratio =
        match (oracle_ink_bbox_area_px, internal_ink_bbox_area_px) {
            (Some(oracle_area), Some(internal_area)) if oracle_area > 0 => {
                Some(internal_area as f64 / oracle_area as f64)
            }
            _ => None,
        };
    let oracle_ink_pixel_count =
        (oracle.non_white_pixel_count > 0).then_some(oracle.non_white_pixel_count);
    let internal_ink_pixel_count =
        (internal.non_white_pixel_count > 0).then_some(internal.non_white_pixel_count);
    let internal_to_oracle_ink_pixel_ratio =
        match (oracle_ink_pixel_count, internal_ink_pixel_count) {
            (Some(oracle_pixels), Some(internal_pixels)) if oracle_pixels > 0 => {
                Some(internal_pixels as f64 / oracle_pixels as f64)
            }
            _ => None,
        };
    let status = if !page_size_matches {
        RasterGrossStatus::PageSizeMismatch
    } else if oracle_ink_bbox_area_px.is_none() {
        RasterGrossStatus::BlankOraclePage
    } else if internal_ink_bbox_area_px.is_none() {
        RasterGrossStatus::BlankInternalPage
    } else if internal_to_oracle_ink_bbox_ratio
        .is_some_and(|ratio| ratio < min_internal_to_oracle_ink_ratio)
    {
        RasterGrossStatus::MissingMajorInkBoundingBox
    } else if internal_to_oracle_ink_pixel_ratio
        .is_some_and(|ratio| ratio < min_internal_to_oracle_ink_ratio)
    {
        RasterGrossStatus::MissingMajorInkPixels
    } else {
        RasterGrossStatus::Pass
    };

    RasterGrossReport {
        status,
        page_size_matches,
        oracle_ink_bbox_area_px,
        internal_ink_bbox_area_px,
        internal_to_oracle_ink_bbox_ratio,
        oracle_ink_pixel_count,
        internal_ink_pixel_count,
        internal_to_oracle_ink_pixel_ratio,
    }
}

fn classify_oracle_metric_findings(
    min_internal_tokens: usize,
    min_common_token_ratio: f64,
    internal_token_count: Option<usize>,
    common_unique_token_ratio: Option<f64>,
    normalized_common_unique_token_ratio: Option<f64>,
    page_count_within_tolerance: Option<bool>,
    first_page_raster_gross_status: Option<RasterGrossStatus>,
    build_failed: bool,
) -> Vec<OracleMetricFinding> {
    if build_failed {
        return vec![OracleMetricFinding::BuildFailed];
    }

    let mut findings = Vec::new();
    if internal_token_count.is_some_and(|count| count < min_internal_tokens) {
        findings.push(OracleMetricFinding::InternalTokenCountBelowBudget);
    }
    let raw_overlap_below_budget =
        common_unique_token_ratio.is_some_and(|ratio| ratio < min_common_token_ratio);
    let normalized_overlap_below_budget =
        normalized_common_unique_token_ratio.is_some_and(|ratio| ratio < min_common_token_ratio);
    if raw_overlap_below_budget {
        findings.push(OracleMetricFinding::RawOverlapBelowBudget);
    }
    if normalized_overlap_below_budget {
        findings.push(OracleMetricFinding::NormalizedOverlapBelowBudget);
    } else if raw_overlap_below_budget && normalized_common_unique_token_ratio.is_some() {
        findings.push(OracleMetricFinding::NormalizationSensitiveOverlap);
    }
    if page_count_within_tolerance == Some(false) {
        findings.push(OracleMetricFinding::PageCountOutOfTolerance);
    }
    if first_page_raster_gross_status.is_some_and(|status| status != RasterGrossStatus::Pass) {
        findings.push(OracleMetricFinding::FirstPageRasterGrossFailure);
    }
    findings
}

fn copy_dir(source_root: &Utf8Path, target_root: &Utf8Path) {
    let mut stack = vec![(source_root.to_owned(), target_root.to_owned())];
    while let Some((source_dir, target_dir)) = stack.pop() {
        fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read source dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path = Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 source path");
            let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                stack.push((source_path, target_path));
                continue;
            }
            fs::copy(source_path.as_std_path(), target_path.as_std_path())
                .expect("copy source file");
        }
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.len() >= 2)
        .collect()
}

fn tokenize_normalized(text: &str) -> Vec<String> {
    let mut normalized = String::with_capacity(text.len());
    for character in text.nfkd() {
        match character {
            '\u{00ad}' => {}
            '\u{0300}'..='\u{036f}'
            | '\u{1ab0}'..='\u{1aff}'
            | '\u{1dc0}'..='\u{1dff}'
            | '\u{20d0}'..='\u{20ff}'
            | '\u{fe20}'..='\u{fe2f}' => {}
            '\u{0131}' => normalized.push('i'),
            '\u{0237}' => normalized.push('j'),
            'ﬀ' => normalized.push_str("ff"),
            'ﬁ' => normalized.push_str("fi"),
            'ﬂ' => normalized.push_str("fl"),
            'ﬃ' => normalized.push_str("ffi"),
            'ﬄ' => normalized.push_str("ffl"),
            'ﬅ' | 'ﬆ' => normalized.push_str("st"),
            'α' | 'Α' => normalized.push_str(" alpha "),
            'β' | 'Β' => normalized.push_str(" beta "),
            'γ' | 'Γ' => normalized.push_str(" gamma "),
            'δ' | 'Δ' => normalized.push_str(" delta "),
            'ε' | 'Ε' => normalized.push_str(" epsilon "),
            'ζ' | 'Ζ' => normalized.push_str(" zeta "),
            'η' | 'Η' => normalized.push_str(" eta "),
            'θ' | 'Θ' => normalized.push_str(" theta "),
            'ι' | 'Ι' => normalized.push_str(" iota "),
            'κ' | 'Κ' => normalized.push_str(" kappa "),
            'λ' | 'Λ' => normalized.push_str(" lambda "),
            'μ' | 'Μ' | 'µ' => normalized.push_str(" mu "),
            'ν' | 'Ν' => normalized.push_str(" nu "),
            'ξ' | 'Ξ' => normalized.push_str(" xi "),
            'ο' | 'Ο' => normalized.push_str(" omicron "),
            'π' | 'Π' => normalized.push_str(" pi "),
            'ρ' | 'Ρ' => normalized.push_str(" rho "),
            'σ' | 'ς' | 'Σ' => normalized.push_str(" sigma "),
            'τ' | 'Τ' => normalized.push_str(" tau "),
            'υ' | 'Υ' => normalized.push_str(" upsilon "),
            'φ' | 'Φ' => normalized.push_str(" phi "),
            'χ' | 'Χ' => normalized.push_str(" chi "),
            'ψ' | 'Ψ' => normalized.push_str(" psi "),
            'ω' | 'Ω' => normalized.push_str(" omega "),
            _ => normalized.push(character),
        }
    }
    tokenize(&normalized)
}

fn unique_tokens(tokens: &[String]) -> BTreeSet<String> {
    tokens.iter().cloned().collect()
}

fn ordered_difference_sample(
    tokens: &[String],
    allowed: &BTreeSet<String>,
    limit: usize,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut sample = Vec::new();
    for token in tokens {
        if allowed.contains(token) || !seen.insert(token.clone()) {
            continue;
        }
        sample.push(token.clone());
        if sample.len() >= limit {
            break;
        }
    }
    sample
}

fn oracle_case_artifact_path(
    report_dir: &Utf8Path,
    arxiv_id: &str,
    version: &str,
    suffix: &str,
) -> Utf8PathBuf {
    let safe_arxiv_id = arxiv_id
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character => character,
        })
        .collect::<String>();
    report_dir.join(format!("{safe_arxiv_id}-{version}-{suffix}"))
}

fn oracle_case_first_page_raster_prefix(
    report_dir: &Utf8Path,
    arxiv_id: &str,
    version: &str,
    variant: &str,
) -> Utf8PathBuf {
    oracle_case_artifact_path(report_dir, arxiv_id, version, &format!("{variant}-page-1"))
}

fn sample_structure_slice_document_ir() -> DocumentIr {
    let source = SourceProvenance::file("main.tex", 0, 1);
    DocumentIr::new(vec![
        IrBlock::TitleBlock(TitleBlock {
            title: Some("A Paper".to_string()),
            title_source: None,
            authors: vec!["Ada Lovelace".to_string()],
            author_sources: Vec::new(),
            date: None,
            date_source: None,
            keywords: Vec::new(),
            keyword_sources: Vec::new(),
            pacs: Vec::new(),
            pacs_sources: Vec::new(),
            source: source.clone(),
        }),
        IrBlock::Abstract(AbstractBlock {
            content: vec![InlineNode::Text {
                text: "Short abstract.".to_string(),
                source: source.clone(),
            }],
            source: source.clone(),
        }),
        IrBlock::Heading(HeadingBlock {
            level: 1,
            number: Some("1".to_string()),
            content: vec![InlineNode::Text {
                text: "Intro".to_string(),
                source: source.clone(),
            }],
            source: source.clone(),
        }),
        IrBlock::Paragraph(ParagraphBlock {
            content: vec![InlineNode::Text {
                text: "Body text.".to_string(),
                source: source.clone(),
            }],
            source: source.clone(),
        }),
        IrBlock::Graphic(GraphicBlock {
            path: "figure.png".to_string(),
            options: None,
            page_selection: None,
            asset_format: None,
            asset_hash: None,
            asset_dimensions: None,
            caption: Some("Plot caption.".to_string()),
            caption_source: None,
            source: source.clone(),
        }),
        IrBlock::Table(TableBlock {
            environment: "tabular".to_string(),
            width_spec: None,
            columns: Vec::new(),
            rows: vec![TableRow {
                rule_above: false,
                partial_rules_above: Vec::new(),
                cells: vec![
                    TableCell {
                        text: "Cell".to_string(),
                        column_span: None,
                        row_span: None,
                        alignment: None,
                        rule_before_count: 0,
                        rule_after_count: 0,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                    TableCell {
                        text: "Alpha".to_string(),
                        column_span: None,
                        row_span: None,
                        alignment: None,
                        rule_before_count: 0,
                        rule_after_count: 0,
                        cell_prefix: None,
                        cell_suffix: None,
                    },
                ],
                rule_below: false,
                partial_rules_below: Vec::new(),
            }],
            caption: Some("Table caption.".to_string()),
            caption_source: None,
            source: source.clone(),
        }),
        IrBlock::Bibliography(BibliographyBlock {
            items: vec![BibliographyItemIr {
                key: "key".to_string(),
                label: Some("1".to_string()),
                content: "Author. Title.".to_string(),
                source: source.clone(),
            }],
            source: source.clone(),
        }),
        IrBlock::RawFallback(RawFallbackIr {
            source_excerpt: "Fallback noisy".to_string(),
            expanded_text: None,
            normalized_visible_text: None,
            environment: Some("unknownenv".to_string()),
            reason: FallbackReason::UnsupportedEnvironment,
            source_hash: None,
            full_source_artifact: None,
            truncated: false,
            source,
        }),
    ])
}

#[test]
fn arxiv_oracle_artifact_paths_are_report_local_and_safe() {
    let report_dir = Utf8PathBuf::from("/tmp/latexd-report");

    assert_eq!(
        oracle_case_artifact_path(&report_dir, "2301.01234", "v2", "oracle.txt"),
        Utf8PathBuf::from("/tmp/latexd-report/2301.01234-v2-oracle.txt")
    );
    assert_eq!(
        oracle_case_artifact_path(&report_dir, "math/0301001", "v1", "internal.pdf"),
        Utf8PathBuf::from("/tmp/latexd-report/math_0301001-v1-internal.pdf")
    );
    assert_eq!(
        oracle_case_artifact_path(
            &report_dir,
            "math/0301001",
            "v1",
            "first-page-raster-diff.png"
        ),
        Utf8PathBuf::from("/tmp/latexd-report/math_0301001-v1-first-page-raster-diff.png")
    );
}

#[test]
fn arxiv_oracle_parses_pdfinfo_page_count() {
    let output = "Title:          A Paper\nPages:          17\nPage size:      612 x 792 pts\n";

    assert_eq!(parse_pdfinfo_page_count(output).expect("page count"), 17);
}

#[test]
fn arxiv_oracle_report_serializes_first_page_raster_diff_path() {
    let report = OracleCaseReport {
        arxiv_id: "2301.01234".to_string(),
        version: "v1".to_string(),
        title: "A Paper".to_string(),
        license: "cc0".to_string(),
        source_url: "https://example.invalid/source".to_string(),
        pdf_url: "https://example.invalid/pdf".to_string(),
        toplevel: Utf8PathBuf::from("main.tex"),
        oracle_pdf: Utf8PathBuf::from("/tmp/oracle.pdf"),
        oracle_text: Utf8PathBuf::from("/tmp/oracle.txt"),
        oracle_page_count: 1,
        oracle_first_page_raster: Utf8PathBuf::from("/tmp/oracle-page-1.png"),
        oracle_first_page_raster_smoke: RasterSmokeReport {
            width_px: 100,
            height_px: 100,
            non_white_pixel_count: 50,
            non_white_bbox: Some(RasterBoundingBox {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
        },
        max_page_count_delta: 2,
        min_first_page_ink_ratio: 0.35,
        source_root: Utf8PathBuf::from("/tmp/source"),
        oracle_token_count: 10,
        oracle_unique_token_count: 9,
        oracle_normalized_token_count: 10,
        oracle_normalized_unique_token_count: 9,
        internal_token_count: Some(8),
        internal_unique_token_count: Some(7),
        common_unique_token_count: Some(6),
        common_unique_token_ratio: Some(0.75),
        internal_normalized_token_count: Some(8),
        internal_normalized_unique_token_count: Some(7),
        normalized_common_unique_token_count: Some(6),
        normalized_common_unique_token_ratio: Some(0.75),
        ir_structure_slices: Vec::new(),
        missing_token_sample: Vec::new(),
        extra_token_sample: Vec::new(),
        normalized_missing_token_sample: Vec::new(),
        normalized_extra_token_sample: Vec::new(),
        metric_findings: Vec::new(),
        internal_text: Some(Utf8PathBuf::from("/tmp/internal.txt")),
        internal_pdf: Some(Utf8PathBuf::from("/tmp/internal.pdf")),
        internal_document_ir: Some(Utf8PathBuf::from("/tmp/internal-document-ir.json")),
        display_list_render: None,
        internal_page_count: Some(1),
        page_count_delta: Some(0),
        page_count_within_tolerance: Some(true),
        internal_first_page_raster: Some(Utf8PathBuf::from("/tmp/internal-page-1.png")),
        internal_first_page_raster_smoke: Some(RasterSmokeReport {
            width_px: 100,
            height_px: 100,
            non_white_pixel_count: 45,
            non_white_bbox: Some(RasterBoundingBox {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
        }),
        first_page_raster_gross: Some(RasterGrossReport {
            status: RasterGrossStatus::Pass,
            page_size_matches: true,
            oracle_ink_bbox_area_px: Some(12),
            internal_ink_bbox_area_px: Some(12),
            internal_to_oracle_ink_bbox_ratio: Some(1.0),
            oracle_ink_pixel_count: Some(50),
            internal_ink_pixel_count: Some(45),
            internal_to_oracle_ink_pixel_ratio: Some(0.9),
        }),
        first_page_raster_diff: Some(Utf8PathBuf::from(
            "/tmp/2301.01234-v1-first-page-raster-diff.png",
        )),
        first_page_raster_diff_metrics: Some(RasterDiffReport {
            width_px: 100,
            height_px: 100,
            differing_pixel_count: 10,
            differing_pixel_bbox: Some(RasterBoundingBox {
                x: 4,
                y: 5,
                width: 6,
                height: 7,
            }),
            overlapping_differing_pixel_count: 7,
            oracle_only_pixel_count: 1,
            internal_only_pixel_count: 2,
            differing_pixel_ratio: 0.001,
        }),
        internal_build_failure: None,
        internal_diagnostics: Vec::new(),
    };

    let value = serde_json::to_value(report).expect("serialize report");

    assert_eq!(
        value["first_page_raster_diff"],
        "/tmp/2301.01234-v1-first-page-raster-diff.png"
    );
    assert_eq!(value["first_page_raster_diff_metrics"]["width_px"], 100);
    assert_eq!(
        value["first_page_raster_diff_metrics"]["differing_pixel_count"],
        10
    );
    assert_eq!(
        value["first_page_raster_diff_metrics"]["differing_pixel_bbox"]["x"],
        4
    );
    assert_eq!(
        value["first_page_raster_diff_metrics"]["overlapping_differing_pixel_count"],
        7
    );
    assert_eq!(
        value["first_page_raster_diff_metrics"]["oracle_only_pixel_count"],
        1
    );
    assert_eq!(
        value["first_page_raster_diff_metrics"]["internal_only_pixel_count"],
        2
    );
}

#[test]
fn arxiv_oracle_first_page_raster_paths_are_report_local() {
    let report_dir = Utf8PathBuf::from("/tmp/latexd-report");

    assert_eq!(
        oracle_case_first_page_raster_prefix(&report_dir, "math/0301001", "v1", "oracle"),
        Utf8PathBuf::from("/tmp/latexd-report/math_0301001-v1-oracle-page-1")
    );
}

#[test]
fn arxiv_oracle_first_page_raster_png_path_preserves_dotted_ids() {
    let report_dir = Utf8PathBuf::from("/tmp/latexd-report");
    let prefix = oracle_case_first_page_raster_prefix(&report_dir, "2602.14379", "v1", "oracle");

    assert_eq!(
        rasterized_singlefile_png_path(&prefix),
        Utf8PathBuf::from("/tmp/latexd-report/2602.14379-v1-oracle-page-1.png")
    );
}

#[test]
fn arxiv_oracle_writes_first_page_raster_diff_artifact() {
    let tempdir = tempdir().expect("tempdir");
    let tempdir = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8");
    let oracle_path = tempdir.join("oracle.png");
    let internal_path = tempdir.join("internal.png");
    let diff_path = tempdir.join("diff.png");

    image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255])
        .expect("oracle image")
        .save_with_format(oracle_path.as_std_path(), image::ImageFormat::Png)
        .expect("write oracle image");
    image::RgbaImage::from_raw(
        2,
        2,
        vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255],
    )
    .expect("internal image")
    .save_with_format(internal_path.as_std_path(), image::ImageFormat::Png)
    .expect("write internal image");

    let metrics =
        write_raster_diff_image(&oracle_path, &internal_path, &diff_path).expect("write diff");

    let diff = image::load_from_memory_with_format(
        &fs::read(diff_path.as_std_path()).expect("read diff"),
        image::ImageFormat::Png,
    )
    .expect("decode diff")
    .into_rgba8();
    assert_eq!(diff.dimensions(), (2, 2));
    assert_eq!(diff.get_pixel(0, 0).0, [255, 255, 255, 255]);
    assert_eq!(diff.get_pixel(1, 0).0, [255, 0, 0, 255]);
    assert_eq!(diff.get_pixel(0, 1).0, [0, 0, 255, 255]);
    assert_eq!(
        metrics,
        RasterDiffReport {
            width_px: 2,
            height_px: 2,
            differing_pixel_count: 3,
            differing_pixel_bbox: Some(RasterBoundingBox {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            }),
            overlapping_differing_pixel_count: 1,
            oracle_only_pixel_count: 0,
            internal_only_pixel_count: 2,
            differing_pixel_ratio: 0.75,
        }
    );

    let oracle_only_oracle_path = tempdir.join("oracle-only-oracle.png");
    let oracle_only_internal_path = tempdir.join("oracle-only-internal.png");
    let oracle_only_diff_path = tempdir.join("oracle-only-diff.png");
    image::RgbaImage::from_raw(1, 2, vec![0, 0, 0, 255, 255, 255, 255, 255])
        .expect("oracle-only oracle image")
        .save_with_format(
            oracle_only_oracle_path.as_std_path(),
            image::ImageFormat::Png,
        )
        .expect("write oracle-only oracle image");
    image::RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255])
        .expect("oracle-only internal image")
        .save_with_format(
            oracle_only_internal_path.as_std_path(),
            image::ImageFormat::Png,
        )
        .expect("write oracle-only internal image");

    let oracle_only_metrics = write_raster_diff_image(
        &oracle_only_oracle_path,
        &oracle_only_internal_path,
        &oracle_only_diff_path,
    )
    .expect("write oracle-only diff");
    let oracle_only_diff = image::load_from_memory_with_format(
        &fs::read(oracle_only_diff_path.as_std_path()).expect("read oracle-only diff"),
        image::ImageFormat::Png,
    )
    .expect("decode oracle-only diff")
    .into_rgba8();

    assert_eq!(oracle_only_diff.dimensions(), (1, 2));
    assert_eq!(oracle_only_diff.get_pixel(0, 0).0, [255, 255, 255, 255]);
    assert_eq!(oracle_only_diff.get_pixel(0, 1).0, [255, 0, 255, 255]);
    assert_eq!(
        oracle_only_metrics,
        RasterDiffReport {
            width_px: 1,
            height_px: 2,
            differing_pixel_count: 1,
            differing_pixel_bbox: Some(RasterBoundingBox {
                x: 0,
                y: 1,
                width: 1,
                height: 1,
            }),
            overlapping_differing_pixel_count: 0,
            oracle_only_pixel_count: 1,
            internal_only_pixel_count: 0,
            differing_pixel_ratio: 0.5,
        }
    );

    let identical_left_path = tempdir.join("identical-left.png");
    let identical_right_path = tempdir.join("identical-right.png");
    let identical_diff_path = tempdir.join("identical-diff.png");
    image::RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255])
        .expect("identical left image")
        .save_with_format(identical_left_path.as_std_path(), image::ImageFormat::Png)
        .expect("write identical left image");
    fs::copy(
        identical_left_path.as_std_path(),
        identical_right_path.as_std_path(),
    )
    .expect("copy identical image");

    let identical_metrics = write_raster_diff_image(
        &identical_left_path,
        &identical_right_path,
        &identical_diff_path,
    )
    .expect("write identical diff");

    assert_eq!(
        identical_metrics,
        RasterDiffReport {
            width_px: 1,
            height_px: 1,
            differing_pixel_count: 0,
            differing_pixel_bbox: None,
            overlapping_differing_pixel_count: 0,
            oracle_only_pixel_count: 0,
            internal_only_pixel_count: 0,
            differing_pixel_ratio: 0.0,
        }
    );
}

#[test]
fn arxiv_oracle_render_ir_document_path_uses_revision_artifact_layout() {
    assert_eq!(
        render_ir_document_path(&Utf8PathBuf::from("/tmp/build"), 7),
        Utf8PathBuf::from("/tmp/build/rev-7/render-ir/document-ir.json")
    );
}

#[test]
fn arxiv_oracle_render_ir_display_list_pdf_path_uses_revision_artifact_layout() {
    assert_eq!(
        render_ir_display_list_pdf_path(&Utf8PathBuf::from("/tmp/build"), 7),
        Utf8PathBuf::from("/tmp/build/rev-7/render-ir/display-list.pdf")
    );
}

#[test]
fn arxiv_oracle_reads_copied_document_ir_artifact() {
    let document_ir = sample_structure_slice_document_ir();
    let tempdir = tempdir().expect("tempdir");
    let build_root = Utf8PathBuf::from_path_buf(tempdir.path().join("build")).expect("utf8");
    let document_ir_path = render_ir_document_path(&build_root, 3);
    fs::create_dir_all(
        document_ir_path
            .parent()
            .expect("document IR parent")
            .as_std_path(),
    )
    .expect("create document IR artifact parent");
    fs::write(
        document_ir_path.as_std_path(),
        serde_json::to_vec(&document_ir).expect("document IR json"),
    )
    .expect("write document IR artifact");

    let read = read_document_ir(&document_ir_path).expect("read document IR artifact");

    assert_eq!(read.extracted_text(), document_ir.extracted_text());
}

#[test]
fn arxiv_oracle_ir_structure_slice_reports_separate_document_regions() {
    let document_ir = sample_structure_slice_document_ir();
    let slice_texts = collect_structure_slice_texts(&document_ir);
    let tempdir = tempdir().expect("tempdir");
    let source_root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8");
    let oracle_unique = unique_tokens(&tokenize(
        "A Paper Ada Lovelace Short abstract Intro Body text Plot caption Table caption Cell Alpha Author Title",
    ));
    let oracle_normalized_unique = unique_tokens(&tokenize_normalized(
        "A Paper Ada Lovelace Short abstract Intro Body text Plot caption Table caption Cell Alpha Author Title",
    ));

    let reports = build_structure_slice_reports(
        &document_ir,
        &source_root,
        &oracle_unique,
        &oracle_normalized_unique,
    );
    let reports_by_kind = reports
        .iter()
        .map(|report| (report.kind, report))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        slice_texts[&OracleStructureSliceKind::Caption].text,
        "Plot caption.\nTable caption."
    );
    assert_eq!(reports_by_kind.len(), 7);
    assert_eq!(
        reports_by_kind[&OracleStructureSliceKind::FrontMatter].common_unique_token_ratio,
        1.0
    );
    assert_eq!(
        reports_by_kind[&OracleStructureSliceKind::Caption].unique_token_count,
        3
    );
    assert_eq!(
        reports_by_kind[&OracleStructureSliceKind::Table].common_unique_token_ratio,
        1.0
    );
    assert_eq!(
        reports_by_kind[&OracleStructureSliceKind::References].common_unique_token_ratio,
        1.0
    );
    assert_eq!(
        reports_by_kind[&OracleStructureSliceKind::Fallback].common_unique_token_count,
        0
    );
    assert_eq!(
        reports_by_kind[&OracleStructureSliceKind::Fallback].extra_token_sample,
        vec!["fallback".to_string(), "noisy".to_string()]
    );
}

#[test]
fn arxiv_oracle_ir_structure_slice_reports_source_backed_extra_tokens() {
    let tempdir = tempdir().expect("tempdir");
    let source_root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8");
    let source_text = "\\begin{tabular}{c}$\\ket{100}\\ket{000}$\\end{tabular}";
    fs::write(source_root.join("main.tex").as_std_path(), source_text).expect("write source");
    let source = SourceProvenance::file("main.tex", 0, source_text.len() as u32);
    let document_ir = DocumentIr::new(vec![IrBlock::Table(TableBlock {
        environment: "tabular".to_string(),
        width_spec: None,
        columns: Vec::new(),
        rows: vec![TableRow {
            rule_above: false,
            partial_rules_above: Vec::new(),
            cells: vec![TableCell {
                text: "$100000$".to_string(),
                column_span: None,
                row_span: None,
                alignment: None,
                rule_before_count: 0,
                rule_after_count: 0,
                cell_prefix: None,
                cell_suffix: None,
            }],
            rule_below: false,
            partial_rules_below: Vec::new(),
        }],
        caption: None,
        caption_source: None,
        source,
    })]);
    let oracle_unique = unique_tokens(&tokenize("clock state"));
    let oracle_normalized_unique = unique_tokens(&tokenize_normalized("clock state"));

    let reports = build_structure_slice_reports(
        &document_ir,
        &source_root,
        &oracle_unique,
        &oracle_normalized_unique,
    );
    let table = reports
        .iter()
        .find(|report| report.kind == OracleStructureSliceKind::Table)
        .expect("table report");

    assert_eq!(table.extra_token_sample, vec!["100000".to_string()]);
    assert_eq!(table.source_backed_extra_token_count, 1);
    assert_eq!(table.source_backed_extra_token_ratio, Some(1.0));
    assert_eq!(
        table.source_backed_extra_token_sample,
        vec!["100000".to_string()]
    );
    assert!(table.source_unbacked_extra_token_sample.is_empty());
}

#[test]
fn arxiv_oracle_raster_smoke_reports_non_white_bbox() {
    let rgba = vec![
        255, 255, 255, 255, 10, 10, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 240, 240, 240,
        255, 255, 255, 255, 255,
    ];

    let smoke = raster_smoke_from_rgba(3, 2, rgba).expect("raster smoke");

    assert_eq!(smoke.width_px, 3);
    assert_eq!(smoke.height_px, 2);
    assert_eq!(smoke.non_white_pixel_count, 2);
    assert_eq!(
        smoke.non_white_bbox,
        Some(RasterBoundingBox {
            x: 1,
            y: 0,
            width: 1,
            height: 2,
        })
    );
}

#[test]
fn arxiv_oracle_normalized_tokens_fold_greek_ligatures_soft_hyphens_and_accents() {
    let tokens = tokenize_normalized(
        "α β ﬁeld co\u{00ad}author François Franc\u{0327}ois CentraleSupélec CentraleSupe\u{0301}lec Saarbrücken Saarbru\u{0308}cken Fı\u{0301}sica Vı\u{0301}ctor",
    );

    assert_eq!(
        tokens,
        vec![
            "alpha",
            "beta",
            "field",
            "coauthor",
            "francois",
            "francois",
            "centralesupelec",
            "centralesupelec",
            "saarbrucken",
            "saarbrucken",
            "fisica",
            "victor",
        ]
    );
}

#[test]
fn arxiv_oracle_normalized_overlap_matches_symbol_and_name_forms() {
    let oracle = unique_tokens(&tokenize_normalized(
        "α ﬁeld co\u{00ad}author Franc\u{0327}ois CentraleSupe\u{0301}lec Saarbru\u{0308}cken Fı\u{0301}sica Vı\u{0301}ctor",
    ));
    let internal = unique_tokens(&tokenize_normalized(
        "alpha field coauthor François CentraleSupélec Saarbrücken Física Víctor",
    ));

    assert_eq!(oracle.intersection(&internal).count(), 8);
}

#[test]
fn arxiv_oracle_metric_findings_classify_normalization_sensitive_overlap() {
    let findings = classify_oracle_metric_findings(
        10,
        0.8,
        Some(20),
        Some(0.5),
        Some(0.9),
        Some(true),
        Some(RasterGrossStatus::Pass),
        false,
    );

    assert_eq!(
        findings,
        vec![
            OracleMetricFinding::RawOverlapBelowBudget,
            OracleMetricFinding::NormalizationSensitiveOverlap,
        ]
    );
}

#[test]
fn arxiv_oracle_metric_findings_classify_persistent_text_page_and_raster_failures() {
    let findings = classify_oracle_metric_findings(
        10,
        0.8,
        Some(4),
        Some(0.3),
        Some(0.4),
        Some(false),
        Some(RasterGrossStatus::MissingMajorInkBoundingBox),
        false,
    );

    assert_eq!(
        findings,
        vec![
            OracleMetricFinding::InternalTokenCountBelowBudget,
            OracleMetricFinding::RawOverlapBelowBudget,
            OracleMetricFinding::NormalizedOverlapBelowBudget,
            OracleMetricFinding::PageCountOutOfTolerance,
            OracleMetricFinding::FirstPageRasterGrossFailure,
        ]
    );
}

#[test]
fn arxiv_oracle_metric_findings_classify_build_failure() {
    let findings = classify_oracle_metric_findings(10, 0.8, None, None, None, None, None, true);

    assert_eq!(findings, vec![OracleMetricFinding::BuildFailed]);
}

#[test]
fn arxiv_oracle_manifest_defaults_phase2_budgets() {
    let manifest = serde_json::from_str::<OracleManifest>(
        r#"{
            "cases": [{
            "arxiv_id": "2301.01234",
            "version": "v1",
            "title": "A Paper",
            "toplevel": "main.tex",
            "license": "cc0",
            "source_url": "https://example.invalid/source",
            "pdf_url": "https://example.invalid/pdf",
            "min_oracle_tokens": 10,
            "min_internal_tokens": 5,
            "min_common_token_ratio": 0.5
          }]
        }"#,
    )
    .expect("parse manifest");

    let case = &manifest.cases[0];
    assert_eq!(case.max_page_count_delta, 2);
    assert_eq!(case.min_first_page_ink_ratio, 0.35);
}

#[test]
fn arxiv_oracle_page_count_tolerance_uses_absolute_delta() {
    assert!(page_count_within_tolerance(10, 12, 2));
    assert!(page_count_within_tolerance(12, 10, 2));
    assert!(!page_count_within_tolerance(10, 13, 2));
}

#[test]
fn arxiv_oracle_raster_gross_passes_with_enough_matching_first_page_ink() {
    let oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 1_000,
        non_white_bbox: Some(RasterBoundingBox {
            x: 10,
            y: 10,
            width: 50,
            height: 40,
        }),
    };
    let internal = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 600,
        non_white_bbox: Some(RasterBoundingBox {
            x: 12,
            y: 12,
            width: 40,
            height: 30,
        }),
    };

    let report = compare_raster_smoke(&oracle, &internal, 0.5);

    assert_eq!(report.status, RasterGrossStatus::Pass);
    assert_eq!(report.oracle_ink_bbox_area_px, Some(2000));
    assert_eq!(report.internal_ink_bbox_area_px, Some(1200));
    assert_eq!(report.internal_to_oracle_ink_bbox_ratio, Some(0.6));
    assert_eq!(report.oracle_ink_pixel_count, Some(1_000));
    assert_eq!(report.internal_ink_pixel_count, Some(600));
    assert_eq!(report.internal_to_oracle_ink_pixel_ratio, Some(0.6));
}

#[test]
fn arxiv_oracle_raster_gross_flags_missing_major_ink_bounding_box() {
    let oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 6_400,
        non_white_bbox: Some(RasterBoundingBox {
            x: 5,
            y: 5,
            width: 80,
            height: 80,
        }),
    };
    let internal = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 400,
        non_white_bbox: Some(RasterBoundingBox {
            x: 5,
            y: 5,
            width: 20,
            height: 20,
        }),
    };

    let report = compare_raster_smoke(&oracle, &internal, 0.35);

    assert_eq!(report.status, RasterGrossStatus::MissingMajorInkBoundingBox);
    assert!(
        report
            .internal_to_oracle_ink_bbox_ratio
            .is_some_and(|ratio| ratio < 0.35)
    );
}

#[test]
fn arxiv_oracle_raster_gross_flags_missing_major_ink_pixels() {
    let oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 2_000,
        non_white_bbox: Some(RasterBoundingBox {
            x: 10,
            y: 10,
            width: 60,
            height: 50,
        }),
    };
    let internal = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 300,
        non_white_bbox: Some(RasterBoundingBox {
            x: 10,
            y: 10,
            width: 60,
            height: 50,
        }),
    };

    let report = compare_raster_smoke(&oracle, &internal, 0.35);

    assert_eq!(report.status, RasterGrossStatus::MissingMajorInkPixels);
    assert_eq!(report.internal_to_oracle_ink_bbox_ratio, Some(1.0));
    assert_eq!(report.internal_to_oracle_ink_pixel_ratio, Some(0.15));
}

#[test]
fn arxiv_oracle_raster_gross_flags_blank_and_page_size_failures() {
    let oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 6_400,
        non_white_bbox: Some(RasterBoundingBox {
            x: 0,
            y: 0,
            width: 80,
            height: 80,
        }),
    };
    let blank_internal = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 0,
        non_white_bbox: None,
    };
    let blank_oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
        non_white_pixel_count: 0,
        non_white_bbox: None,
    };
    let wrong_size_internal = RasterSmokeReport {
        width_px: 90,
        height_px: 100,
        non_white_pixel_count: 6_400,
        non_white_bbox: Some(RasterBoundingBox {
            x: 0,
            y: 0,
            width: 80,
            height: 80,
        }),
    };

    assert_eq!(
        compare_raster_smoke(&oracle, &blank_internal, 0.35).status,
        RasterGrossStatus::BlankInternalPage
    );
    assert_eq!(
        compare_raster_smoke(&blank_oracle, &oracle, 0.35).status,
        RasterGrossStatus::BlankOraclePage
    );
    assert_eq!(
        compare_raster_smoke(&oracle, &wrong_size_internal, 0.35).status,
        RasterGrossStatus::PageSizeMismatch
    );
}
