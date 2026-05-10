struct Rev5SemanticMultiBibliographySiblingDirtyRun {
    _tempdir: tempfile::TempDir,
    build_root: Utf8PathBuf,
    fifth: CompileOutcome,
}

enum Rev5SemanticMultiBibliographySiblingDirtyCase {
    PlainOrder,
    ReversedOrder,
}

async fn compile_rev5_semantic_multi_bibliography_base_rebuild_with_sibling_dirty(
    dirty_files: Vec<Utf8PathBuf>,
) -> Rev5SemanticMultiBibliographySiblingDirtyRun {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    build_optioned_bibliography_order_stack_to_rev4(&fixture_root, &root, &driver, &build_root)
        .await;

    apply_fixture_overlay(&fixture_root.join("rev5"), &root);
    let world = ProjectWorld::load(root.clone()).expect("world");
    let fifth = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 5,
            build_root: build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("fifth build should succeed");

    Rev5SemanticMultiBibliographySiblingDirtyRun {
        _tempdir: tempdir,
        build_root,
        fifth,
    }
}

async fn run_rev5_semantic_multi_bibliography_sibling_dirty_case(
    case: Rev5SemanticMultiBibliographySiblingDirtyCase,
) {
    let dirty_files = match case {
        Rev5SemanticMultiBibliographySiblingDirtyCase::PlainOrder => {
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ]
        }
        Rev5SemanticMultiBibliographySiblingDirtyCase::ReversedOrder => {
            vec![
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ]
        }
    };
    let run = compile_rev5_semantic_multi_bibliography_base_rebuild_with_sibling_dirty(
        dirty_files.clone(),
    )
    .await;
    assert_rev5_semantic_multi_bibliography_base_rebuild(&run.build_root, &run.fifth, dirty_files);
}

type Rev5SibDirty = Rev5SemanticMultiBibliographySiblingDirtyCase;

async fn run_rev5_sib_dirty(case: Rev5SibDirty) {
    run_rev5_semantic_multi_bibliography_sibling_dirty_case(case).await;
}
