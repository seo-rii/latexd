use std::{collections::BTreeSet, env, fs, process::Command};

use camino::{Utf8Path, Utf8PathBuf};
use latexd::compiler::{CompileRequest, CompilerDriver};
use tempfile::tempdir;
use tex_world::ProjectWorld;

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
    internal_token_count: Option<usize>,
    internal_unique_token_count: Option<usize>,
    common_unique_token_count: Option<usize>,
    common_unique_token_ratio: Option<f64>,
    missing_token_sample: Vec<String>,
    extra_token_sample: Vec<String>,
    internal_text: Option<Utf8PathBuf>,
    internal_pdf: Option<Utf8PathBuf>,
    internal_page_count: Option<usize>,
    page_count_delta: Option<i64>,
    page_count_within_tolerance: Option<bool>,
    internal_first_page_raster: Option<Utf8PathBuf>,
    internal_first_page_raster_smoke: Option<RasterSmokeReport>,
    first_page_raster_gross: Option<RasterGrossReport>,
    internal_build_failure: Option<String>,
    internal_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct RasterSmokeReport {
    width_px: u32,
    height_px: u32,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum RasterGrossStatus {
    Pass,
    PageSizeMismatch,
    BlankOraclePage,
    BlankInternalPage,
    MissingMajorTextBlocks,
}

fn default_max_page_count_delta() -> usize {
    2
}

fn default_min_first_page_ink_ratio() -> f64 {
    0.35
}

#[tokio::test]
#[ignore = "requires LATEXD_ARXIV_CC0_CORPUS with downloaded arXiv source/PDF files"]
async fn arxiv_cc0_local_corpus_compares_internal_pdf_text_to_official_pdf() {
    let Some(corpus_root) = env::var_os("LATEXD_ARXIV_CC0_CORPUS") else {
        eprintln!("skipping: LATEXD_ARXIV_CC0_CORPUS is not set");
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
    let manifest_path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-oracle/cc0-smoke.json");
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
        assert!(
            oracle_tokens.len() >= case.min_oracle_tokens,
            "{} official PDF text extraction produced only {} tokens",
            case.arxiv_id,
            oracle_tokens.len()
        );
        let oracle_unique = unique_tokens(&oracle_tokens);
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
        let compile_result = driver
            .compile(CompileRequest {
                root: project_root.clone(),
                manifest: world.manifest.clone(),
                toplevel: case.toplevel.clone(),
                rev: 1,
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
            internal_token_count: None,
            internal_unique_token_count: None,
            common_unique_token_count: None,
            common_unique_token_ratio: None,
            missing_token_sample: Vec::new(),
            extra_token_sample: Vec::new(),
            internal_text: None,
            internal_pdf: None,
            internal_page_count: None,
            page_count_delta: None,
            page_count_within_tolerance: None,
            internal_first_page_raster: None,
            internal_first_page_raster_smoke: None,
            first_page_raster_gross: None,
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
                let internal_tokens = tokenize(&internal_text);
                let internal_unique = unique_tokens(&internal_tokens);
                let common = oracle_unique
                    .intersection(&internal_unique)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let ratio = common.len() as f64 / oracle_unique.len().max(1) as f64;
                report.internal_token_count = Some(internal_tokens.len());
                report.internal_unique_token_count = Some(internal_unique.len());
                report.common_unique_token_count = Some(common.len());
                report.common_unique_token_ratio = Some(ratio);
                report.missing_token_sample =
                    ordered_difference_sample(&oracle_tokens, &common, 80);
                report.extra_token_sample =
                    ordered_difference_sample(&internal_tokens, &oracle_unique, 80);
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
    let report_path = report_dir.join("cc0-smoke-report.json");
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
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    Ok(RasterSmokeReport {
        width_px,
        height_px,
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
    let status = if !page_size_matches {
        RasterGrossStatus::PageSizeMismatch
    } else if oracle_ink_bbox_area_px.is_none() {
        RasterGrossStatus::BlankOraclePage
    } else if internal_ink_bbox_area_px.is_none() {
        RasterGrossStatus::BlankInternalPage
    } else if internal_to_oracle_ink_bbox_ratio
        .is_some_and(|ratio| ratio < min_internal_to_oracle_ink_ratio)
    {
        RasterGrossStatus::MissingMajorTextBlocks
    } else {
        RasterGrossStatus::Pass
    };

    RasterGrossReport {
        status,
        page_size_matches,
        oracle_ink_bbox_area_px,
        internal_ink_bbox_area_px,
        internal_to_oracle_ink_bbox_ratio,
    }
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
}

#[test]
fn arxiv_oracle_parses_pdfinfo_page_count() {
    let output = "Title:          A Paper\nPages:          17\nPage size:      612 x 792 pts\n";

    assert_eq!(parse_pdfinfo_page_count(output).expect("page count"), 17);
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
fn arxiv_oracle_raster_smoke_reports_non_white_bbox() {
    let rgba = vec![
        255, 255, 255, 255, 10, 10, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 240, 240, 240,
        255, 255, 255, 255, 255,
    ];

    let smoke = raster_smoke_from_rgba(3, 2, rgba).expect("raster smoke");

    assert_eq!(smoke.width_px, 3);
    assert_eq!(smoke.height_px, 2);
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
}

#[test]
fn arxiv_oracle_raster_gross_flags_missing_major_text_blocks() {
    let oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
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
        non_white_bbox: Some(RasterBoundingBox {
            x: 5,
            y: 5,
            width: 20,
            height: 20,
        }),
    };

    let report = compare_raster_smoke(&oracle, &internal, 0.35);

    assert_eq!(report.status, RasterGrossStatus::MissingMajorTextBlocks);
    assert!(
        report
            .internal_to_oracle_ink_bbox_ratio
            .is_some_and(|ratio| ratio < 0.35)
    );
}

#[test]
fn arxiv_oracle_raster_gross_flags_blank_and_page_size_failures() {
    let oracle = RasterSmokeReport {
        width_px: 100,
        height_px: 100,
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
        non_white_bbox: None,
    };
    let wrong_size_internal = RasterSmokeReport {
        width_px: 90,
        height_px: 100,
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
        compare_raster_smoke(&oracle, &wrong_size_internal, 0.35).status,
        RasterGrossStatus::PageSizeMismatch
    );
}
