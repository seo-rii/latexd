struct SemanticallyEqualMultiBibliographyShipoutReplayRun {
    third_output: String,
    fourth_output: String,
}

enum SemanticallyEqualMultiBibliographyShipoutReplayCase {
    Metadata,
    Output,
}

async fn run_semantically_equal_multi_bibliography_shipout_replay()
-> SemanticallyEqualMultiBibliographyShipoutReplayRun {
    let run = run_optioned_bibliography_order_stack_revisions(4).await;
    let third_bundle = load_checkpoint_bundle(&run.build_root.join("rev-3/checkpoints.json"))
        .expect("load third bundle");
    let third_preamble_checkpoint = third_bundle
        .checkpoints
        .first()
        .expect("third preamble checkpoint")
        .meta
        .checkpoint_id
        .clone();
    let third_output =
        fs::read_to_string(run.build_root.join("rev-3/output.txt")).expect("read third output");
    let fourth_output =
        fs::read_to_string(run.build_root.join("rev-4/output.txt")).expect("read fourth output");
    let third_build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(run.build_root.join("rev-3/build-meta.json")).expect("read third build meta"),
    )
    .expect("parse third build meta");
    let fourth_build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(run.build_root.join("rev-4/build-meta.json")).expect("read fourth build meta"),
    )
    .expect("parse fourth build meta");

    let third = run.outcomes.get(&3).expect("third outcome");
    let fourth = run.outcomes.get(&4).expect("fourth outcome");
    assert!(third_build_meta.aux_sensitive);
    assert_eq!(
        third_build_meta.dirty_files,
        vec![Utf8PathBuf::from("main.tex")]
    );
    assert_eq!(third_build_meta.start_page_index, 0);
    assert_eq!(third_build_meta.page_count, third.page_metadata.len());
    assert_eq!(third_build_meta.semantic_pass_count, 2);
    assert_eq!(third_build_meta.semantic_rerun_count, 1);
    assert!(third_build_meta.semantic_fixpoint_reached);
    assert!(!third_build_meta.semantic_aux_backdated);
    assert!(fourth_build_meta.aux_sensitive);
    assert_eq!(
        fourth_build_meta.dirty_files,
        vec![Utf8PathBuf::from("refsa.bbl")]
    );
    assert_eq!(
        fourth_build_meta.start_checkpoint_id,
        Some(third_preamble_checkpoint.clone())
    );
    assert_eq!(fourth_build_meta.start_page_index, 0);
    assert_eq!(fourth_build_meta.page_count, fourth.page_metadata.len());
    assert_eq!(fourth_build_meta.rebuilt_page_count, 0);
    assert_eq!(fourth_build_meta.reused_page_count, 1);
    assert_eq!(fourth_build_meta.semantic_pass_count, 1);
    assert_eq!(fourth_build_meta.semantic_rerun_count, 0);
    assert!(fourth_build_meta.semantic_fixpoint_reached);
    assert!(fourth_build_meta.semantic_aux_backdated);
    assert_eq!(
        fourth.reused_checkpoint_id,
        Some(third_preamble_checkpoint.clone())
    );
    let tail = fourth.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 3);
    assert_eq!(tail.previous_page_start, 0);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, third.page_metadata.len());
    assert_eq!(tail.page_count, fourth.page_metadata.len());
    assert_eq!(
        fourth
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        third
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(fourth.page_patches.is_empty());
    assert!(
        fourth
            .page_artifacts
            .iter()
            .all(|page| page.pdf_url.starts_with("/artifacts/rev/3/pages/"))
    );

    SemanticallyEqualMultiBibliographyShipoutReplayRun {
        third_output,
        fourth_output,
    }
}

async fn run_semantically_equal_multi_bibliography_shipout_replay_case(
    case: SemanticallyEqualMultiBibliographyShipoutReplayCase,
) {
    let run = run_semantically_equal_multi_bibliography_shipout_replay().await;
    if matches!(
        case,
        SemanticallyEqualMultiBibliographyShipoutReplayCase::Output
    ) {
        assert!(run.third_output.contains("Order check. [2] and [1]"));
        assert!(run.third_output.contains("[1] Beta entry."));
        assert!(run.third_output.contains("[2] Alpha entry."));
        assert_eq!(
            run.fourth_output
                .matches("wrapperarticletwocolumnunicode")
                .count(),
            1
        );
        assert!(!run.fourth_output.contains("column,unicode]wrapper"));
        assert!(!run.fourth_output.contains("icleOrder check"));
        assert!(run.fourth_output.contains("Order check. [2] and [1]"));
        assert!(run.fourth_output.contains("[1] Beta entry."));
        assert!(run.fourth_output.contains("[2] Alpha entry."));
    }
}

type ShipReplayCase = SemanticallyEqualMultiBibliographyShipoutReplayCase;

async fn run_ship_replay_case(case: ShipReplayCase) {
    run_semantically_equal_multi_bibliography_shipout_replay_case(case).await;
}
