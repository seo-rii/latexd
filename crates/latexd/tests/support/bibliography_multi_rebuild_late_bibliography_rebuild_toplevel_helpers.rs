enum LateBibliographyRebuildToplevelCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

async fn run_bibliography_multi_rebuild_late_bibliography_rebuild_toplevel(
    case: LateBibliographyRebuildToplevelCase,
) {
    enum DirtyMarker {
        Untracked,
        Unreadable,
    }

    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let filler = "late bibliography replay filler text ".repeat(220);
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::write(root.join("article.cls"), "").expect("write class");
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}Early cite \\cite{{alpha}}. {filler} Late year \\citeyear{{beta}}.\\bibliography{{refs}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2024]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert!(
        first.page_metadata.len() >= 2,
        "fixture should push late cite onto a later page"
    );

    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");

    let (dirty_marker, dirty_files) = match case {
        LateBibliographyRebuildToplevelCase::UntrackedFollows => (
            DirtyMarker::Untracked,
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        LateBibliographyRebuildToplevelCase::UntrackedPrecedes => (
            DirtyMarker::Untracked,
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
            ],
        ),
        LateBibliographyRebuildToplevelCase::UnreadableFollows => (
            DirtyMarker::Unreadable,
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        LateBibliographyRebuildToplevelCase::UnreadablePrecedes => (
            DirtyMarker::Unreadable,
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
            ],
        ),
    };
    match dirty_marker {
        DirtyMarker::Untracked => {
            fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        DirtyMarker::Unreadable => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
    }

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("second semantic aux build should succeed");

    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("Late year 2025."),
        "executed main.tex should reflect the late bibliography change"
    );
    assert_eq!(
        second.reused_checkpoint_id, None,
        "semantic-changing bibliography edits should rebuild from the base snapshot"
    );
    let replace_indexes = second
        .page_patches
        .iter()
        .filter_map(|patch| match patch {
            PagePatchOp::ReplacePage { index, .. } => Some(*index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        replace_indexes,
        vec![second.page_metadata.len().saturating_sub(1)]
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(!build_meta.semantic_aux_backdated);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
}

type LateRebuildTop = LateBibliographyRebuildToplevelCase;

async fn run_late_rebuild_top(case: LateRebuildTop) {
    run_bibliography_multi_rebuild_late_bibliography_rebuild_toplevel(case).await;
}
