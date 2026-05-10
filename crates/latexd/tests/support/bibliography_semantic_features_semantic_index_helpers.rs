struct BibliographySemanticFeaturesSemanticIndexFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
}

enum BibliographySemanticFeaturesBackdateAndRebuildCase {
    Backdate,
    Rebuild,
}

enum BibliographySemanticFeaturesAuxCase {
    IncludeOnly,
    IncludeSectionsAndLabels,
    ManualTocStarredSections,
}

enum BibliographySemanticFeaturesSemanticIndexCase {
    Artifact,
    PrintbibheadingBibintoc,
}

fn prepare_bibliography_semantic_features_semantic_index_fixture(
    main_source: &str,
    extra_files: &[(&str, &str)],
) -> BibliographySemanticFeaturesSemanticIndexFixture {
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
    fs::write(root.join("main.tex"), main_source).expect("write main");
    for (relative_path, contents) in extra_files {
        let target = root.join(relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent.as_std_path()).expect("create parent dir");
        }
        fs::write(target, contents).expect("write extra file");
    }

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    BibliographySemanticFeaturesSemanticIndexFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
    }
}

async fn compile_bibliography_semantic_features_semantic_index_fixture(
    fixture: &BibliographySemanticFeaturesSemanticIndexFixture,
    changed_files: &[&str],
) -> CompileOutcome {
    compile_bibliography_semantic_features_fixture(fixture, 1, changed_files).await
}

fn bibliography_semantic_features_changed_files(paths: &[&str]) -> Vec<Utf8PathBuf> {
    paths.iter().map(|path| Utf8PathBuf::from(*path)).collect()
}

async fn compile_bibliography_semantic_features_fixture(
    fixture: &BibliographySemanticFeaturesSemanticIndexFixture,
    rev: u64,
    changed_files: &[&str],
) -> CompileOutcome {
    let changed_files = bibliography_semantic_features_changed_files(changed_files);
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev,
            build_root: fixture.build_root.clone(),
            changed_files,
        })
        .await
        .expect("semantic aux build should succeed")
}

async fn compile_bibliography_semantic_features_rev2_with_build_meta(
    fixture: &BibliographySemanticFeaturesSemanticIndexFixture,
    changed_files: &[&str],
) -> (CompileOutcome, BuildMeta) {
    let outcome = compile_bibliography_semantic_features_fixture(fixture, 2, changed_files).await;
    let changed_files = bibliography_semantic_features_changed_files(changed_files);
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, changed_files);
    assert_eq!(build_meta.start_checkpoint_id, outcome.reused_checkpoint_id);
    assert!(build_meta.semantic_fixpoint_reached);
    assert_eq!(build_meta.page_count, outcome.page_metadata.len());
    (outcome, build_meta)
}

fn load_bibliography_semantic_features_semantic_index(
    fixture: &BibliographySemanticFeaturesSemanticIndexFixture,
    rev: u64,
) -> SemanticAuxIndex {
    serde_json::from_slice::<SemanticAuxIndex>(
        &fs::read(
            fixture
                .build_root
                .join(format!("rev-{rev}/semantic-index.json")),
        )
        .expect("read semantic index"),
    )
    .expect("parse semantic index")
}

async fn run_bibliography_semantic_features_semantic_index_case(
    case: BibliographySemanticFeaturesSemanticIndexCase,
) {
    match case {
        BibliographySemanticFeaturesSemanticIndexCase::Artifact => {
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\input{sections/intro}\\cite{alpha}.\\bibliography{refs}\\end{document}",
                &[
                    ("sections/intro.tex", "\\section{Intro}\\label{sec:intro}"),
                    (
                        "refs.bbl",
                        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
                    ),
                ],
            );
            compile_bibliography_semantic_features_semantic_index_fixture(
                &fixture,
                &["main.tex", "sections/intro.tex", "refs.bbl"],
            )
            .await;

            let index = load_bibliography_semantic_features_semantic_index(&fixture, 1);
            assert!(index.has_table_of_contents);
            assert!(!index.has_bibliography_heading);
            assert_eq!(index.label_count, 1);
            assert_eq!(index.toc_count, 1);
            assert_eq!(index.citation_key_count, 1);
            assert_eq!(index.bibliography_entry_count, 1);
            let main = index
                .files
                .iter()
                .find(|file| file.path == Utf8PathBuf::from("main.tex"))
                .expect("main summary");
            assert_eq!(main.citation_keys, vec![String::from("alpha")]);
            let intro = index
                .files
                .iter()
                .find(|file| file.path == Utf8PathBuf::from("sections/intro.tex"))
                .expect("intro summary");
            assert_eq!(intro.label_keys, vec![String::from("sec:intro")]);
            assert_eq!(intro.toc.len(), 1);
            assert_eq!(intro.toc[0].number, "1");
            assert_eq!(intro.toc[0].title, "Intro");
            let bibliography = index
                .files
                .iter()
                .find(|file| file.path == Utf8PathBuf::from("refs.bbl"))
                .expect("bibliography summary");
            assert_eq!(bibliography.bibliography_keys, vec![String::from("alpha")]);
        }
        BibliographySemanticFeaturesSemanticIndexCase::PrintbibheadingBibintoc => {
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\printbibheading[heading=bibintoc,title={References}]\\end{document}",
                &[],
            );
            compile_bibliography_semantic_features_semantic_index_fixture(&fixture, &["main.tex"])
                .await;

            let output = fs::read_to_string(fixture.build_root.join("rev-1/output.txt"))
                .expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("References .... 1"));
            assert!(output.contains("References"));

            let index = load_bibliography_semantic_features_semantic_index(&fixture, 1);
            assert!(index.has_table_of_contents);
            assert!(index.has_bibliography_heading);
            assert_eq!(index.toc_count, 1);
        }
    }
}

type BibSemAux = BibliographySemanticFeaturesAuxCase;
type BibSemIndex = BibliographySemanticFeaturesSemanticIndexCase;
type BibSemBuild = BibliographySemanticFeaturesBackdateAndRebuildCase;

async fn run_bib_sem_aux(case: BibSemAux) {
    run_bibliography_semantic_features_aux_case(case).await;
}

async fn run_bib_sem_index(case: BibSemIndex) {
    run_bibliography_semantic_features_semantic_index_case(case).await;
}

async fn run_bib_sem_build(case: BibSemBuild) {
    run_bibliography_semantic_features_backdate_and_rebuild(case).await;
}

async fn run_bibliography_semantic_features_aux_case(case: BibliographySemanticFeaturesAuxCase) {
    match case {
        BibliographySemanticFeaturesAuxCase::IncludeOnly => {
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\includeonly{chapters/intro}\\include{chapters/intro}\\include{chapters/extra}\\end{document}",
                &[
                    ("chapters/intro.tex", "\\section{Intro}\\label{sec:intro}"),
                    ("chapters/extra.tex", "\\section{Extra}\\label{sec:extra}"),
                ],
            );
            let outcome = compile_bibliography_semantic_features_semantic_index_fixture(
                &fixture,
                &["main.tex", "chapters/intro.tex", "chapters/extra.tex"],
            )
            .await;
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("1 Intro"));
            assert!(!output.contains("Extra"));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 1);
            assert_eq!(aux.toc[0].title, "Intro");
            assert_eq!(aux.labels.len(), 1);
            assert_eq!(aux.labels[0].key, "sec:intro");
            assert!(
                outcome
                    .dep_trace
                    .inputs
                    .contains(&Utf8PathBuf::from("chapters/intro.tex"))
            );
            assert!(
                !outcome
                    .dep_trace
                    .inputs
                    .contains(&Utf8PathBuf::from("chapters/extra.tex"))
            );
        }
        BibliographySemanticFeaturesAuxCase::IncludeSectionsAndLabels => {
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\include{chapters/intro}See \\ref{sec:intro}.\\end{document}",
                &[("chapters/intro.tex", "\\section{Intro}\\label{sec:intro}")],
            );

            let outcome = compile_bibliography_semantic_features_semantic_index_fixture(
                &fixture,
                &["main.tex", "chapters/intro.tex"],
            )
            .await;
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("1 Intro"));
            assert!(output.contains("See 1."));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 1);
            assert_eq!(aux.toc[0].title, "Intro");
            assert_eq!(aux.labels.len(), 1);
            assert_eq!(aux.labels[0].key, "sec:intro");
            assert_eq!(aux.labels[0].number, "1");
            assert!(
                outcome
                    .dep_trace
                    .inputs
                    .contains(&Utf8PathBuf::from("chapters/intro.tex"))
            );
        }
        BibliographySemanticFeaturesAuxCase::ManualTocStarredSections => {
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\section*{Prelude}\\phantomsection\\addcontentsline{toc}{section}{Prelude}\\section{Intro}\\label{sec:intro}See \\ref{sec:intro}.\\end{document}",
                &[],
            );
            compile_bibliography_semantic_features_semantic_index_fixture(&fixture, &["main.tex"])
                .await;

            let build_root = &fixture.build_root;
            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("Prelude .... 1"));
            assert!(output.contains("1 Intro .... 1"));
            assert!(output.contains("Prelude"));
            assert!(output.contains("See 1."));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 2);
            assert_eq!(aux.toc[0].title, "Prelude");
            assert_eq!(aux.toc[0].number, "");
            assert_eq!(aux.toc[1].title, "Intro");
            assert_eq!(aux.toc[1].number, "1");

            let stored_sources = serde_json::from_slice::<StoredSources>(
                &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
            )
            .expect("parse sources");
            let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
            assert!(!executed_main.contains("\\phantomsection"));
            assert!(!executed_main.contains("\\addcontentsline"));
        }
    }
}

async fn run_bibliography_semantic_features_backdate_and_rebuild(
    case: BibliographySemanticFeaturesBackdateAndRebuildCase,
) {
    match case {
        BibliographySemanticFeaturesBackdateAndRebuildCase::Backdate => {
            let main_source = "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\label{sec:intro}See \\ref{sec:intro} on page \\pageref{sec:intro}. Cite \\cite{alpha}.\\bibliography{refs}\\end{document}";
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                main_source,
                &[(
                    "refs.bbl",
                    "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
                )],
            );

            compile_bibliography_semantic_features_semantic_index_fixture(
                &fixture,
                &["main.tex", "refs.bbl"],
            )
            .await;
            let first_bundle =
                load_checkpoint_bundle(&fixture.build_root.join("rev-1/checkpoints.json"))
                    .expect("load first checkpoint bundle");

            let rev1_aux_path = fixture.build_root.join("rev-1/aux.json");
            let previous_payload = fs::read(&rev1_aux_path).expect("read prior aux");
            fs::write(&rev1_aux_path, &previous_payload).expect("rewrite aux payload");
            fs::write(fixture.root.join("main.tex"), format!("{main_source}\n"))
                .expect("rewrite main");

            let (second, build_meta) = compile_bibliography_semantic_features_rev2_with_build_meta(
                &fixture,
                &["main.tex"],
            )
            .await;
            let replay_checkpoint = first_bundle
                .checkpoints
                .iter()
                .find(|checkpoint| {
                    Some(&checkpoint.meta.checkpoint_id) == second.reused_checkpoint_id.as_ref()
                })
                .expect("reused rev-1 checkpoint");
            let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
            assert_eq!(tail.previous_rev, 1);
            assert_eq!(tail.current_page_start, 0);
            assert_eq!(tail.page_count, second.page_metadata.len());
            assert!(second.page_patches.is_empty());
            assert_eq!(
                fs::read(fixture.build_root.join("rev-2/aux.json")).expect("read backdated aux"),
                previous_payload
            );
            let replay_start_page = replay_checkpoint
                .meta
                .page_index_after
                .min(second.page_metadata.len());
            assert_eq!(build_meta.start_page_index, replay_start_page);
            assert_eq!(build_meta.rebuilt_page_count, 0);
            assert_eq!(build_meta.reused_page_count, second.page_metadata.len());
            assert_eq!(build_meta.semantic_pass_count, 1);
            assert_eq!(build_meta.semantic_rerun_count, 0);
            assert!(build_meta.semantic_aux_backdated);
        }
        BibliographySemanticFeaturesBackdateAndRebuildCase::Rebuild => {
            let filler = "late semantic invalidation filler ".repeat(520);
            let fixture = prepare_bibliography_semantic_features_semantic_index_fixture(
                &format!(
                    "\\documentclass{{article}}\\begin{{document}}\\tableofcontents\\section{{Intro}}Intro body. {filler}\\input{{sections/tail}}\\end{{document}}"
                ),
                &[(
                    "sections/tail.tex",
                    "\\section{Old Tail Scope}\\label{sec:tail}See \\ref{sec:tail}.",
                )],
            );

            let first = compile_bibliography_semantic_features_semantic_index_fixture(
                &fixture,
                &["main.tex", "sections/tail.tex"],
            )
            .await;
            assert!(
                first.page_metadata.len() >= 2,
                "fixture should push the second section onto a later page"
            );

            fs::write(
                fixture.root.join("sections/tail.tex"),
                "\\section{New Tail Scope}\\label{sec:tail}See \\ref{sec:tail}.",
            )
            .expect("rewrite tail");

            let (_, build_meta) = compile_bibliography_semantic_features_rev2_with_build_meta(
                &fixture,
                &["sections/tail.tex"],
            )
            .await;

            let second_output = fs::read_to_string(fixture.build_root.join("rev-2/output.txt"))
                .expect("read second output");
            assert!(second_output.contains("New Tail Scope"));
            assert!(
                !second_output.contains("Old Tail Scope"),
                "late section title edits should not leave the stale TOC entry behind"
            );
            assert_eq!(build_meta.start_page_index, 0);
            assert_eq!(
                build_meta.rebuilt_page_count + build_meta.reused_page_count,
                build_meta.page_count
            );
            assert_eq!(build_meta.semantic_pass_count, 2);
            assert_eq!(build_meta.semantic_rerun_count, 1);
            assert!(!build_meta.semantic_aux_backdated);
        }
    }
}
