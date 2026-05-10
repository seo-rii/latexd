#[derive(Clone, Copy)]
enum SingleSourceNoopTarget {
    Input,
    Toplevel,
}

enum SingleSourceNoopEditShape {
    Leading,
    LeadingAndTrailing,
}

enum SingleSourceNoopLeadingCase {
    InputLeading,
    InputLeadingAndTrailing,
    ToplevelLeading,
    ToplevelLeadingAndTrailing,
}

type SingleLeadCase = SingleSourceNoopLeadingCase;

async fn run_single_lead_case(case: SingleLeadCase) {
    run_unchanged_tail_single_source_leading(case).await;
}

struct UnchangedTailSingleSourceLeadingFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    original_body: String,
    target: SingleSourceNoopTarget,
    second_dirty_files: Vec<Utf8PathBuf>,
}

async fn prepare_unchanged_tail_single_source_leading_fixture(
    target: SingleSourceNoopTarget,
) -> UnchangedTailSingleSourceLeadingFixture {
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
    let original_body = (0..1536)
        .map(|index| format!("w{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let (first_changed_files, second_dirty_files) = match target {
        SingleSourceNoopTarget::Input => {
            fs::create_dir_all(root.join("sections")).expect("create sections dir");
            fs::write(root.join("sections/body.tex"), &original_body).expect("write body tex");
            fs::write(
                root.join("main.tex"),
                "\\documentclass{article}\\begin{document}\\input{sections/body}\\end{document}\n",
            )
            .expect("write main tex");
            (
                vec![
                    Utf8PathBuf::from("main.tex"),
                    Utf8PathBuf::from("sections/body.tex"),
                ],
                vec![Utf8PathBuf::from("sections/body.tex")],
            )
        }
        SingleSourceNoopTarget::Toplevel => {
            fs::write(
                root.join("main.tex"),
                format!(
                    "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}\n",
                    original_body
                ),
            )
            .expect("write main tex");
            (
                vec![Utf8PathBuf::from("main.tex")],
                vec![Utf8PathBuf::from("main.tex")],
            )
        }
    };

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
            changed_files: first_changed_files,
        })
        .await
        .expect("first build should succeed");
    assert!(
        !first.page_metadata.is_empty(),
        "fixture should render at least one page"
    );

    UnchangedTailSingleSourceLeadingFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        original_body,
        target,
        second_dirty_files,
    }
}

fn rewrite_unchanged_tail_single_source_leading(
    fixture: &UnchangedTailSingleSourceLeadingFixture,
    edit_shape: SingleSourceNoopEditShape,
) {
    match fixture.target {
        SingleSourceNoopTarget::Input => {
            let trailing = match edit_shape {
                SingleSourceNoopEditShape::Leading => "",
                SingleSourceNoopEditShape::LeadingAndTrailing => {
                    "% trailing comment after body content\n"
                }
            };
            fs::write(
                fixture.root.join("sections/body.tex"),
                format!(
                    "% leading comment before body content\n{}\n{}",
                    fixture.original_body, trailing
                ),
            )
            .expect("rewrite body tex");
        }
        SingleSourceNoopTarget::Toplevel => {
            let trailing = match edit_shape {
                SingleSourceNoopEditShape::Leading => "",
                SingleSourceNoopEditShape::LeadingAndTrailing => {
                    "% trailing comment after document\n"
                }
            };
            fs::write(
                fixture.root.join("main.tex"),
                format!(
                    "% leading comment before documentclass\n\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}\n{}",
                    fixture.original_body, trailing
                ),
            )
            .expect("rewrite main tex");
        }
    }
}

async fn compile_unchanged_tail_single_source_leading_second_pass(
    fixture: &UnchangedTailSingleSourceLeadingFixture,
) -> CompileOutcome {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: fixture.second_dirty_files.clone(),
        })
        .await
        .expect("second build should succeed")
}

fn assert_unchanged_tail_single_source_leading_reuse(
    fixture: &UnchangedTailSingleSourceLeadingFixture,
    second: &CompileOutcome,
) {
    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.previous_page_start, 0);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, fixture.first.page_metadata.len());
    assert_eq!(tail.page_count, second.page_metadata.len());
    assert_eq!(
        second
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        fixture
            .first
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>()
    );
    match fixture.target {
        SingleSourceNoopTarget::Input => assert!(second.reused_checkpoint_id.is_some()),
        SingleSourceNoopTarget::Toplevel => assert_eq!(second.reused_checkpoint_id, None),
    }
    assert!(second.page_patches.is_empty());
    assert!(
        second
            .page_artifacts
            .iter()
            .all(|page| page.pdf_url.starts_with("/artifacts/rev/1/pages/"))
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, fixture.second_dirty_files);
    match fixture.target {
        SingleSourceNoopTarget::Input => {
            assert_eq!(build_meta.start_checkpoint_id, second.reused_checkpoint_id);
            assert!(build_meta.start_page_index <= build_meta.page_count);
        }
        SingleSourceNoopTarget::Toplevel => {
            assert_eq!(build_meta.start_checkpoint_id, None);
            assert_eq!(build_meta.start_page_index, 0);
        }
    }
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, build_meta.page_count);
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_unchanged_tail_single_source_leading(case: SingleSourceNoopLeadingCase) {
    let (target, edit_shape) = match case {
        SingleSourceNoopLeadingCase::InputLeading => (
            SingleSourceNoopTarget::Input,
            SingleSourceNoopEditShape::Leading,
        ),
        SingleSourceNoopLeadingCase::InputLeadingAndTrailing => (
            SingleSourceNoopTarget::Input,
            SingleSourceNoopEditShape::LeadingAndTrailing,
        ),
        SingleSourceNoopLeadingCase::ToplevelLeading => (
            SingleSourceNoopTarget::Toplevel,
            SingleSourceNoopEditShape::Leading,
        ),
        SingleSourceNoopLeadingCase::ToplevelLeadingAndTrailing => (
            SingleSourceNoopTarget::Toplevel,
            SingleSourceNoopEditShape::LeadingAndTrailing,
        ),
    };
    let fixture = prepare_unchanged_tail_single_source_leading_fixture(target).await;
    rewrite_unchanged_tail_single_source_leading(&fixture, edit_shape);
    let second = compile_unchanged_tail_single_source_leading_second_pass(&fixture).await;
    assert_unchanged_tail_single_source_leading_reuse(&fixture, &second);
}
