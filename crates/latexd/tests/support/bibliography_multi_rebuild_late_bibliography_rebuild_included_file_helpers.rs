enum LateBibliographyRebuildIncludedFileCase {
    Baseline,
    BaselineUntrackedFollows,
    BaselineUntrackedPrecedes,
    BaselineUnreadableFollows,
    BaselineUnreadablePrecedes,
    Tracked,
    TrackedUntrackedFollows,
    TrackedUntrackedPrecedes,
    TrackedUnreadableFollows,
    TrackedUnreadablePrecedes,
}

async fn run_bibliography_multi_rebuild_late_bibliography_rebuild_included_file(
    case: LateBibliographyRebuildIncludedFileCase,
) {
    enum DirtyMarker {
        Untracked,
        Unreadable,
    }

    let include_appendix = matches!(
        case,
        LateBibliographyRebuildIncludedFileCase::Tracked
            | LateBibliographyRebuildIncludedFileCase::TrackedUntrackedFollows
            | LateBibliographyRebuildIncludedFileCase::TrackedUntrackedPrecedes
            | LateBibliographyRebuildIncludedFileCase::TrackedUnreadableFollows
            | LateBibliographyRebuildIncludedFileCase::TrackedUnreadablePrecedes
    );
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = "late bibliography replay filler text ".repeat(220);
    let appendix_filler = "appendix trailing filler text ".repeat(180);
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    let main_source = if include_appendix {
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\input{sections/appendix}\\bibliography{refs}\\end{document}"
    } else {
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\bibliography{refs}\\end{document}"
    };
    fs::write(root.join("main.tex"), main_source).expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Early cite \\cite{{alpha}}. {body_filler} Late year \\citeyear{{beta}}."),
    )
    .expect("write body");
    if include_appendix {
        fs::write(
            root.join("sections/appendix.tex"),
            format!("Appendix A. {appendix_filler}"),
        )
        .expect("write appendix");
    }
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2024]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let mut initial_changed_files = vec![
        Utf8PathBuf::from("main.tex"),
        Utf8PathBuf::from("sections/body.tex"),
    ];
    if include_appendix {
        initial_changed_files.push(Utf8PathBuf::from("sections/appendix.tex"));
    }
    initial_changed_files.push(Utf8PathBuf::from("refs.bbl"));
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: initial_changed_files,
        })
        .await
        .expect("first semantic aux build should succeed");
    if !include_appendix {
        assert!(
            first.page_metadata.len() >= 2,
            "fixture should push late cite onto a later page"
        );
    }

    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");
    if include_appendix {
        fs::write(
            root.join("sections/appendix.tex"),
            format!("Appendix B. {appendix_filler}"),
        )
        .expect("rewrite appendix");
    }

    let (dirty_marker, dirty_files) = match case {
        LateBibliographyRebuildIncludedFileCase::Baseline => {
            (None, vec![Utf8PathBuf::from("refs.bbl")])
        }
        LateBibliographyRebuildIncludedFileCase::BaselineUntrackedFollows => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::BaselineUntrackedPrecedes => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::BaselineUnreadableFollows => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::BaselineUnreadablePrecedes => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::Tracked => (
            None,
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::TrackedUntrackedFollows => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::TrackedUntrackedPrecedes => (
            Some(DirtyMarker::Untracked),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::TrackedUnreadableFollows => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        LateBibliographyRebuildIncludedFileCase::TrackedUnreadablePrecedes => (
            Some(DirtyMarker::Unreadable),
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
        ),
    };
    match dirty_marker {
        Some(DirtyMarker::Untracked) => {
            fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        Some(DirtyMarker::Unreadable) => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
        None => {}
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
        second_sources.executed_files[&Utf8PathBuf::from("sections/body.tex")]
            .contains("Late year 2025."),
        "executed body.tex should reflect the semantic bibliography change"
    );
    if include_appendix {
        assert!(
            second_sources.executed_files[&Utf8PathBuf::from("sections/appendix.tex")]
                .contains("Appendix B."),
            "executed appendix.tex should reflect the later tracked change"
        );
    }
    assert_eq!(second.reused_checkpoint_id, None);
    if !include_appendix {
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
    }
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

type LateRebuildInc = LateBibliographyRebuildIncludedFileCase;

async fn run_late_rebuild_inc(case: LateRebuildInc) {
    run_bibliography_multi_rebuild_late_bibliography_rebuild_included_file(case).await;
}
