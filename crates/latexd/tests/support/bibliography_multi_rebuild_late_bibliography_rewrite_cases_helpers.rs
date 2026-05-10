struct LateBibliographyRewriteCaseRun {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    first: CompileOutcome,
    second: CompileOutcome,
    first_sources: StoredSources,
    second_sources: StoredSources,
}

enum LateBibliographyRewriteCaseKind {
    EarlierCitationOutput,
    LateOnlyChange,
}

type LateRewrite = LateBibliographyRewriteCaseKind;

async fn run_late_rewrite(case: LateRewrite) {
    run_late_bibliography_rewrite_case(case).await;
}

async fn compile_late_bibliography_rewrite_case(
    main_source: impl AsRef<str>,
    first_bibliography: impl AsRef<str>,
    second_bibliography: impl AsRef<str>,
) -> LateBibliographyRewriteCaseRun {
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
    fs::write(root.join("main.tex"), main_source.as_ref()).expect("write main");
    fs::write(root.join("refs.bbl"), first_bibliography.as_ref()).expect("write bbl");

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

    fs::write(root.join("refs.bbl"), second_bibliography.as_ref()).expect("rewrite bbl");

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
    let first_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read first sources"),
    )
    .expect("parse first sources");
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");

    LateBibliographyRewriteCaseRun {
        _tempdir: tempdir,
        build_root,
        first,
        second,
        first_sources,
        second_sources,
    }
}

fn assert_late_bibliography_rewrite_case_rebuild_from_cp0(
    build_root: &Utf8Path,
    second: &CompileOutcome,
) {
    assert_eq!(
        second.reused_checkpoint_id, None,
        "semantic-changing bibliography edits should rebuild from the base snapshot"
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("refs.bbl")]);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.start_checkpoint_id, None);
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

async fn run_late_bibliography_rewrite_case(case: LateBibliographyRewriteCaseKind) {
    match case {
        LateBibliographyRewriteCaseKind::EarlierCitationOutput => {
            let filler = "bibliography replay filler text ".repeat(220);
            let run = compile_late_bibliography_rewrite_case(
                format!(
                    "\\documentclass{{article}}\\begin{{document}}\\section{{Intro}} {filler} Cite \\cite{{alpha}}.\\bibliography{{refs}}\\end{{document}}"
                ),
                "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
                "\\begin{thebibliography}{2}\n\\bibitem{beta} Beta entry.\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
            )
            .await;

            let main_path = Utf8Path::new("main.tex");
            assert!(run.first_sources.executed_files[main_path].contains("Cite [1]."));
            let first_bundle =
                load_checkpoint_bundle(&run.build_root.join("rev-1/checkpoints.json"))
                    .expect("load bundle");
            let bibliography_output_start = first_bundle
                .checkpoints
                .iter()
                .find(|checkpoint| {
                    checkpoint.meta.kind == CheckpointKind::InputBoundary
                        && checkpoint.meta.module_path.as_deref() == Some(Utf8Path::new("refs.bbl"))
                })
                .expect("refs.bbl input boundary")
                .meta
                .output_start_utf8;
            assert!(run.second_sources.files[main_path].contains("\\cite{alpha}"));
            assert!(run.second_sources.executed_files[main_path].contains("Cite [2]."));
            let earliest_changed_rewrite_output_start = run.first_sources.rewrite_spans[main_path]
                .iter()
                .zip(&run.second_sources.rewrite_spans[main_path])
                .find_map(|(previous, current)| {
                    (previous.start_utf8 == current.start_utf8
                        && previous.end_utf8 == current.end_utf8
                        && previous.rendered != current.rendered)
                        .then_some(previous.output_start_utf8)
                })
                .expect("changed cite rewrite span");
            let cp0_output_start = first_bundle.checkpoints[0].meta.output_start_utf8;
            assert_eq!(cp0_output_start, 0);
            assert!(cp0_output_start <= earliest_changed_rewrite_output_start);
            assert!(cp0_output_start < bibliography_output_start);

            assert_late_bibliography_rewrite_case_rebuild_from_cp0(&run.build_root, &run.second);
        }
        LateBibliographyRewriteCaseKind::LateOnlyChange => {
            let filler = "late bibliography replay filler text ".repeat(220);
            let run = compile_late_bibliography_rewrite_case(
                format!(
                    "\\documentclass{{article}}\\begin{{document}}Early cite \\cite{{alpha}}. {filler} Late year \\citeyear{{beta}}.\\bibliography{{refs}}\\end{{document}}"
                ),
                "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2024]{beta} Beta entry.\n\\end{thebibliography}\n",
                "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
            )
            .await;
            assert!(
                run.first.page_metadata.len() >= 2,
                "fixture should push late cite onto a later page"
            );

            assert!(
                run.second_sources.executed_files[&Utf8PathBuf::from("main.tex")]
                    .contains("Late year 2025."),
                "executed main.tex should reflect only the late citeyear change"
            );

            assert_late_bibliography_rewrite_case_rebuild_from_cp0(&run.build_root, &run.second);
        }
    }
}
