struct UnchangedTailMutationFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    shrink_path: Utf8PathBuf,
    tail_path: Utf8PathBuf,
    shrink_source: String,
    first_tail_page: usize,
    shrink_only_pages: Vec<usize>,
    page_files: Vec<(usize, Vec<Utf8PathBuf>)>,
}

async fn prepare_unchanged_tail_mutation_fixture() -> UnchangedTailMutationFixture {
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
    fs::create_dir_all(root.join("sections")).expect("create sections dir");
    let intro = (0..320)
        .map(|index| format!("intro{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let shrink_source = (0..2600)
        .map(|index| format!("shrink{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let tail_source = (0..2200)
        .map(|index| format!("tail{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/shrink.tex"), &shrink_source).expect("write shrink input");
    fs::write(root.join("sections/tail.tex"), &tail_source).expect("write tail input");
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}} {intro} \\input{{sections/shrink}} \\input{{sections/tail}} \\end{{document}}"
        ),
    )
    .expect("write main tex");

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
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/shrink.tex"),
                Utf8PathBuf::from("sections/tail.tex"),
            ],
        })
        .await
        .expect("first build should succeed");

    let shrink_path = Utf8PathBuf::from("sections/shrink.tex");
    let tail_path = Utf8PathBuf::from("sections/tail.tex");
    let page_files = first
        .page_metadata
        .iter()
        .map(|page| {
            (
                page.index,
                page.source_spans
                    .iter()
                    .map(|span| span.file.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let first_tail_page = first
        .page_metadata
        .iter()
        .find(|page| page.source_spans.iter().any(|span| span.file == tail_path))
        .map(|page| page.index)
        .unwrap_or_else(|| panic!("expected at least one tail page, saw {:?}", page_files));
    let mut shrink_only_pages = Vec::new();
    let mut cursor = first_tail_page;
    while cursor > 0 {
        let previous_page = &first.page_metadata[cursor - 1];
        if previous_page.source_spans.is_empty()
            || !previous_page
                .source_spans
                .iter()
                .all(|span| span.file == shrink_path)
        {
            break;
        }
        shrink_only_pages.push(previous_page.index);
        cursor -= 1;
    }
    shrink_only_pages.reverse();
    assert!(
        !shrink_only_pages.is_empty(),
        "expected at least one shrink-only page before the tail boundary, saw {:?}",
        page_files
    );

    UnchangedTailMutationFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        shrink_path,
        tail_path,
        shrink_source,
        first_tail_page,
        shrink_only_pages,
        page_files,
    }
}

fn shrink_span_range(
    first: &CompileOutcome,
    shrink_path: &Utf8Path,
    shrink_only_pages: &[usize],
) -> (usize, usize) {
    let mut start = usize::MAX;
    let mut end = 0usize;
    for page_index in shrink_only_pages {
        for span in first.page_metadata[*page_index]
            .source_spans
            .iter()
            .filter(|span| span.file == shrink_path)
        {
            start = start.min(span.start_utf8 as usize);
            end = end.max(span.end_utf8 as usize);
        }
    }
    assert!(start < end);
    (start, end)
}

#[derive(Clone, Copy)]
enum InsertedAndReplacedUnchangedTailMutationKind {
    Inserted,
    Replaced,
}

async fn run_inserted_or_replaced_unchanged_tail_mutation(
    mutation_kind: InsertedAndReplacedUnchangedTailMutationKind,
) {
    let fixture = prepare_unchanged_tail_mutation_fixture().await;
    let root = fixture.root.clone();
    let build_root = fixture.build_root.clone();
    let shrink_path = fixture.shrink_path.clone();
    let mutated_pages = fixture.shrink_only_pages.clone();
    let first_tail_page = fixture.first_tail_page;

    let (mutation_start, mutation_end) =
        shrink_span_range(&fixture.first, &fixture.shrink_path, &mutated_pages);
    match mutation_kind {
        InsertedAndReplacedUnchangedTailMutationKind::Inserted => {
            let inserted_source = format!(
                "{}{}{}",
                &fixture.shrink_source[..mutation_end],
                &fixture.shrink_source[mutation_start..mutation_end],
                &fixture.shrink_source[mutation_end..]
            );
            fs::write(root.join("sections/shrink.tex"), &inserted_source)
                .expect("rewrite shrink input");
        }
        InsertedAndReplacedUnchangedTailMutationKind::Replaced => {
            let mut replaced_source = fixture.shrink_source.clone().into_bytes();
            for byte in &mut replaced_source[mutation_start..mutation_end] {
                if byte.is_ascii_digit() {
                    *byte = if *byte == b'9' { b'0' } else { *byte + 1 };
                }
            }
            let replaced_source = String::from_utf8(replaced_source).expect("utf8 shrink source");
            fs::write(root.join("sections/shrink.tex"), &replaced_source)
                .expect("rewrite shrink input");
        }
    }

    let second = fixture
        .driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![shrink_path.clone()],
        })
        .await
        .expect("second build should succeed");

    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_page_patches_transform(
        &fixture
            .first
            .renderer_page_metadata
            .iter()
            .map(|page| page.page_id.clone())
            .collect::<Vec<_>>(),
        &second.page_patches,
        &second
            .renderer_page_metadata
            .iter()
            .map(|page| page.page_id.clone())
            .collect::<Vec<_>>(),
    );
    assert_eq!(tail.previous_rev, 1);
    match mutation_kind {
        InsertedAndReplacedUnchangedTailMutationKind::Inserted => {
            assert!(tail.previous_page_start < tail.current_page_start);
            assert_eq!(
                tail.page_count,
                fixture.first.page_metadata.len() - tail.previous_page_start
            );
            assert!(!second.page_patches.is_empty());
        }
        InsertedAndReplacedUnchangedTailMutationKind::Replaced => {
            assert_eq!(tail.previous_page_start, first_tail_page);
            assert_eq!(tail.current_page_start, first_tail_page);
            assert_eq!(
                tail.page_count,
                fixture.first.page_metadata.len() - first_tail_page
            );
            assert!(!second.page_patches.is_empty());
            for offset in 0..tail.page_count {
                assert_eq!(
                    second.page_metadata[tail.current_page_start + offset].page_id,
                    fixture.first.page_metadata[first_tail_page + offset].page_id
                );
            }
        }
    }
    assert_eq!(
        tail.page_count,
        second.page_metadata.len() - tail.current_page_start
    );

    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![shrink_path]);
    assert_eq!(build_meta.start_checkpoint_id, second.reused_checkpoint_id);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert!(build_meta.reused_page_count >= tail.page_count);
    match mutation_kind {
        InsertedAndReplacedUnchangedTailMutationKind::Inserted => {
            assert!(build_meta.start_page_index <= first_tail_page);
        }
        InsertedAndReplacedUnchangedTailMutationKind::Replaced => {
            assert!(build_meta.start_page_index <= tail.current_page_start);
        }
    }
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

type TailMut = InsertedAndReplacedUnchangedTailMutationKind;

async fn run_tail_mut(case: TailMut) {
    run_inserted_or_replaced_unchanged_tail_mutation(case).await;
}
