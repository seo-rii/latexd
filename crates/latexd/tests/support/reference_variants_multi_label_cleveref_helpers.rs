enum ReferenceVariantsMultiLabelCleverefCase {
    SectionAppendixLists,
    SubsectionPluralization,
}

async fn run_reference_variants_multi_label_cleveref_case(
    case: ReferenceVariantsMultiLabelCleverefCase,
) {
    let (main_source, expected_render, removed_commands) = match case {
        ReferenceVariantsMultiLabelCleverefCase::SectionAppendixLists => (
            "\\documentclass{book}\\begin{document}\\chapter{Intro}\\section{Setup}\\label{sec:setup}\\section{Scope}\\label{sec:scope}\\appendix\\chapter{Proofs}\\label{chap:proof}See \\cref{sec:setup,sec:scope}, \\Cref{chap:proof}, and \\cref{sec:scope,chap:proof}.\\end{document}",
            "See Sections 1.1, 1.2, Appendix A, and Section 1.2; Appendix A.",
            vec!["\\cref", "\\Cref"],
        ),
        ReferenceVariantsMultiLabelCleverefCase::SubsectionPluralization => (
            "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\subsubsection{Inner}\\label{subsub:detail}See \\cref{sub:scope,sub:detail} and \\cref{sub:scope,subsub:detail}.\\end{document}",
            "See Subsections 1.1, 1.2 and Subsection 1.1; Subsubsection 1.2.1.",
            vec!["\\cref"],
        ),
    };
    let fixture = prepare_reference_variants_basic_refs_fixture(main_source);
    compile_reference_variants_basic_refs_fixture(&fixture)
        .await
        .expect("semantic aux build should succeed");
    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_render));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(expected_render));
    for removed_command in removed_commands {
        assert!(!executed_main.contains(removed_command));
    }
}

type RefClever = ReferenceVariantsMultiLabelCleverefCase;

async fn run_ref_clever(case: RefClever) {
    run_reference_variants_multi_label_cleveref_case(case).await;
}
