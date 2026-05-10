enum BibliographyFixpointReuseLocation {
    Toplevel,
    IncludedBody,
}

enum BibliographyFixpointReuseCase {
    ReplayCheckpoint,
    TailNoiseUntracked,
    TailNoiseUnreadable,
}

enum BibliographyFixpointReuseCompactCase {
    ToplevelReplayCheckpoint,
    ToplevelTailNoiseUntracked,
    ToplevelTailNoiseUnreadable,
    IncludedBodyReplayCheckpoint,
    IncludedBodyTailNoiseUntracked,
    IncludedBodyTailNoiseUnreadable,
}

async fn run_bibliography_fixpoint_reuse_compact(case: BibliographyFixpointReuseCompactCase) {
    let (location, reuse_case) = match case {
        BibliographyFixpointReuseCompactCase::ToplevelReplayCheckpoint => (
            BibliographyFixpointReuseLocation::Toplevel,
            BibliographyFixpointReuseCase::ReplayCheckpoint,
        ),
        BibliographyFixpointReuseCompactCase::ToplevelTailNoiseUntracked => (
            BibliographyFixpointReuseLocation::Toplevel,
            BibliographyFixpointReuseCase::TailNoiseUntracked,
        ),
        BibliographyFixpointReuseCompactCase::ToplevelTailNoiseUnreadable => (
            BibliographyFixpointReuseLocation::Toplevel,
            BibliographyFixpointReuseCase::TailNoiseUnreadable,
        ),
        BibliographyFixpointReuseCompactCase::IncludedBodyReplayCheckpoint => (
            BibliographyFixpointReuseLocation::IncludedBody,
            BibliographyFixpointReuseCase::ReplayCheckpoint,
        ),
        BibliographyFixpointReuseCompactCase::IncludedBodyTailNoiseUntracked => (
            BibliographyFixpointReuseLocation::IncludedBody,
            BibliographyFixpointReuseCase::TailNoiseUntracked,
        ),
        BibliographyFixpointReuseCompactCase::IncludedBodyTailNoiseUnreadable => (
            BibliographyFixpointReuseLocation::IncludedBody,
            BibliographyFixpointReuseCase::TailNoiseUnreadable,
        ),
    };

    run_bibliography_fixpoint_reuse(location, reuse_case).await;
}

type FixReuse = BibliographyFixpointReuseCompactCase;

async fn run_fix_reuse(case: FixReuse) {
    run_bibliography_fixpoint_reuse_compact(case).await;
}

async fn run_bibliography_fixpoint_reuse(
    location: BibliographyFixpointReuseLocation,
    case: BibliographyFixpointReuseCase,
) {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
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
    let filler = "bibliography replay filler text ".repeat(220);
    let initial_changed_files = match location {
        BibliographyFixpointReuseLocation::Toplevel => {
            fs::write(
                root.join("main.tex"),
                format!(
                    "\\documentclass{{article}}\\begin{{document}}Cite \\cite{{alpha}}.\\section{{Intro}} {filler}\\bibliography{{refs}}\\end{{document}}"
                ),
            )
            .expect("write main");
            vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")]
        }
        BibliographyFixpointReuseLocation::IncludedBody => {
            fs::create_dir_all(root.join("sections")).expect("sections dir");
            fs::write(
                root.join("main.tex"),
                "\\documentclass{article}\\begin{document}\\input{sections/body}\\bibliography{refs}\\end{document}",
            )
            .expect("write main");
            fs::write(
                root.join("sections/body.tex"),
                format!("Cite \\cite{{alpha}}.\\section{{Intro}} {filler}"),
            )
            .expect("write body");
            vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("refs.bbl"),
            ]
        }
    };
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
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
            changed_files: initial_changed_files,
        })
        .await
        .expect("first semantic aux build should succeed");

    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    match case {
        BibliographyFixpointReuseCase::ReplayCheckpoint => {
            assert!(
                first.page_metadata.len() >= 2,
                "fixture should push bibliography onto a later page"
            );
            let expected_checkpoint = first_bundle
                .checkpoints
                .iter()
                .find(|checkpoint| {
                    checkpoint.meta.kind == CheckpointKind::InputBoundary
                        && checkpoint.meta.module_path.as_ref()
                            == Some(&Utf8PathBuf::from("refs.bbl"))
                })
                .expect("refs.bbl input boundary");
            fs::write(
                root.join("refs.bbl"),
                "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha   entry.\n\\end{thebibliography}\n",
            )
            .expect("rewrite bbl");

            let second = driver
                .compile(CompileRequest {
                    root: root.clone(),
                    manifest: world.manifest.clone(),
                    toplevel: Utf8PathBuf::from("main.tex"),
                    rev: 2,
                    build_root: build_root.clone(),
                    changed_files: vec![Utf8PathBuf::from("refs.bbl")],
                })
                .await
                .expect("second semantic aux build should succeed");

            assert_eq!(
                second.reused_checkpoint_id,
                Some(expected_checkpoint.meta.checkpoint_id.clone())
            );
            if matches!(location, BibliographyFixpointReuseLocation::Toplevel) {
                assert_full_rev1_tail_reused(&first, &second);
            }
            assert!(second.page_patches.is_empty());
            let build_meta = serde_json::from_slice::<BuildMeta>(
                &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
            )
            .expect("parse build meta");
            assert!(build_meta.aux_sensitive);
            assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("refs.bbl")]);
            assert_eq!(
                build_meta.start_checkpoint_id,
                Some(expected_checkpoint.meta.checkpoint_id.clone())
            );
            assert_eq!(
                build_meta.start_page_index,
                expected_checkpoint.meta.page_index_after
            );
            assert_eq!(build_meta.page_count, second.page_metadata.len());
            assert_eq!(build_meta.rebuilt_page_count, 0);
            assert_eq!(build_meta.reused_page_count, second.page_metadata.len());
            assert_eq!(build_meta.semantic_pass_count, 1);
            assert_eq!(build_meta.semantic_rerun_count, 0);
            assert!(build_meta.semantic_fixpoint_reached);
            assert!(build_meta.semantic_aux_backdated);
        }
        BibliographyFixpointReuseCase::TailNoiseUntracked
        | BibliographyFixpointReuseCase::TailNoiseUnreadable => {
            let preamble_checkpoint_id = first_bundle.checkpoints[0].meta.checkpoint_id.clone();
            match case {
                BibliographyFixpointReuseCase::TailNoiseUntracked => {
                    fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
                }
                BibliographyFixpointReuseCase::TailNoiseUnreadable => {
                    fs::create_dir_all(root.join("notes.txt"))
                        .expect("create unreadable dirty dir");
                }
                BibliographyFixpointReuseCase::ReplayCheckpoint => unreachable!(),
            }
            let dirty_files = vec![Utf8PathBuf::from("notes.txt")];
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

            assert_full_rev1_tail_reused(&first, &second);
            assert_eq!(
                second.reused_checkpoint_id,
                Some(preamble_checkpoint_id.clone())
            );
            assert!(second.page_patches.is_empty());
            assert!(
                second
                    .page_artifacts
                    .iter()
                    .all(|page| page.pdf_url.starts_with("/artifacts/rev/1/pages/"))
            );
            let build_meta = serde_json::from_slice::<BuildMeta>(
                &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
            )
            .expect("parse build meta");
            assert!(build_meta.aux_sensitive);
            assert_eq!(build_meta.dirty_files, dirty_files);
            assert_eq!(build_meta.start_checkpoint_id, Some(preamble_checkpoint_id));
            assert_eq!(build_meta.start_page_index, 0);
            assert_eq!(build_meta.page_count, second.page_metadata.len());
            assert_eq!(build_meta.rebuilt_page_count, 0);
            assert_eq!(build_meta.reused_page_count, build_meta.page_count);
            assert_eq!(build_meta.semantic_pass_count, 1);
            assert_eq!(build_meta.semantic_rerun_count, 0);
            assert!(build_meta.semantic_fixpoint_reached);
            assert!(build_meta.semantic_aux_backdated);
        }
    }
}

fn assert_full_rev1_tail_reused(first: &CompileOutcome, second: &CompileOutcome) {
    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.previous_page_start, 0);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, first.page_metadata.len());
    assert_eq!(tail.page_count, second.page_metadata.len());
    assert_eq!(
        second
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        first
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>()
    );
}
