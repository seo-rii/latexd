use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::compiler::{CompileOutcome, CompileRequest, CompilerDriver};

async fn compile_preview(
    driver: &CompilerDriver,
    root: &Utf8Path,
    build_root: &Utf8Path,
    rev: u64,
    applied_rev: u64,
    changed_files: &[&str],
) -> Result<CompileOutcome, crate::compiler::CompileFailure> {
    driver
        .compile_for_preview(
            CompileRequest {
                root: root.to_owned(),
                manifest: tex_world::ProjectManifest::discover(root).expect("manifest"),
                toplevel: "main.tex".into(),
                rev,
                build_root: build_root.to_owned(),
                changed_files: changed_files
                    .iter()
                    .map(|path| Utf8PathBuf::from(*path))
                    .collect(),
            },
            applied_rev,
        )
        .await
}

#[tokio::test]
async fn preview_revision_undo_after_unpublished_success_and_failed_followup_matches_clean() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = Utf8Path::from_path(directory.path()).expect("UTF-8 root");
    let build_root = root.join(".latexd/build");
    let driver = CompilerDriver::new(Some("internal".into()), Vec::new());
    let initial_source = r"\input{defs.sty}\begin{document}\word Initial body.\end{document}";
    let final_source = r"\input{defs.sty}\begin{document}\word Revised body.\end{document}";
    fs::write(root.join("defs.sty"), r"\def\word{Alpha}").expect("initial style");
    fs::write(root.join("main.tex"), initial_source).expect("initial source");
    compile_preview(&driver, root, &build_root, 1, 0, &["main.tex", "defs.sty"])
        .await
        .expect("published revision 1");

    fs::write(root.join("defs.sty"), r"\def\word{Bravo}").expect("changed style");
    compile_preview(&driver, root, &build_root, 2, 1, &["defs.sty"])
        .await
        .expect("successful revision 2 remains unpublished");
    assert!(
        fs::read_to_string(build_root.join("rev-2/output.txt"))
            .expect("unpublished output")
            .contains("Bravo")
    );

    fs::write(
        root.join("main.tex"),
        r"\input{defs.sty}\begin{document}\errmessage{intentional failure}\end{document}",
    )
    .expect("erroneous source");
    assert!(
        compile_preview(&driver, root, &build_root, 3, 1, &["defs.sty", "main.tex"])
            .await
            .is_err()
    );

    fs::write(root.join("defs.sty"), r"\def\word{Alpha}").expect("undo style");
    fs::write(root.join("main.tex"), final_source).expect("repair and edit body");
    // The daemon's cache still describes revision 1. Undo makes defs.sty clean
    // against that cache, even though unpublished revision 2 executed Bravo.
    let resumed = compile_preview(&driver, root, &build_root, 4, 1, &["main.tex"])
        .await
        .expect("resume from the published input baseline");
    let clean_root = root.join(".latexd/clean");
    let clean = compile_preview(&driver, root, &clean_root, 1, 0, &["main.tex", "defs.sty"])
        .await
        .expect("independent clean build");
    let resumed_output =
        fs::read_to_string(build_root.join("rev-4/output.txt")).expect("resumed output");
    assert!(resumed_output.contains("Alpha"));
    assert!(!resumed_output.contains("Bravo"));
    assert_eq!(
        resumed_output,
        fs::read_to_string(clean_root.join("rev-1/output.txt")).expect("clean output")
    );
    assert_eq!(
        resumed
            .renderer_page_metadata
            .iter()
            .map(|page| &page.content_hash)
            .collect::<Vec<_>>(),
        clean
            .renderer_page_metadata
            .iter()
            .map(|page| &page.content_hash)
            .collect::<Vec<_>>()
    );
    let resumed_syncmap: serde_json::Value = serde_json::from_slice(
        &fs::read(build_root.join("rev-4/page-syncmap.json")).expect("resumed source map"),
    )
    .expect("parse resumed source map");
    let clean_syncmap: serde_json::Value = serde_json::from_slice(
        &fs::read(clean_root.join("rev-1/page-syncmap.json")).expect("clean source map"),
    )
    .expect("parse clean source map");
    assert_eq!(resumed_syncmap, clean_syncmap);
}
