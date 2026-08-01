use tex_render_model::{
    EventProducer, MetadataField, RenderEvent, SemanticConfidence, SourceSpanRole,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::Vm;

#[test]
fn false_conditionals_do_not_emit_front_matter() {
    let source = r"\iffalse
\title{Wrong}
\author{Hidden Author}
\date{Never}
\fi
\title{Right}
\author{Ada Lovelace \and Grace Hopper}
\date{1843}
\begin{document}
\iffalse\maketitle\fi
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let metadata = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::SetDocumentMetadata(metadata) => Some((
                metadata.field,
                metadata.value.as_str(),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let flushes = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::FlushTitleBlock(_)))
        .collect::<Vec<_>>();

    assert_eq!(
        metadata,
        vec![
            (
                MetadataField::Title,
                "Right",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Author,
                "Ada Lovelace",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Author,
                "Grace Hopper",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Date,
                "1843",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ]
    );
    assert_eq!(flushes.len(), 1);
    assert_eq!(flushes[0].meta.producer, EventProducer::Primitive);
    assert_eq!(flushes[0].meta.confidence, SemanticConfidence::High);
    let title = outcome
        .render_events
        .iter()
        .find(|event| {
            matches!(
                &event.event,
                RenderEvent::SetDocumentMetadata(metadata)
                    if metadata.field == MetadataField::Title
            )
        })
        .expect("title metadata");
    assert!(
        title
            .meta
            .source
            .related
            .iter()
            .any(|span| span.role == SourceSpanRole::ArgumentContent)
    );
    assert!(
        title
            .meta
            .source
            .related
            .iter()
            .any(|span| span.role == SourceSpanRole::Invocation)
    );
}

#[test]
fn macro_generated_front_matter_tracks_expansion_provenance() {
    let source = r"\def\setpaper#1#2{\title{#1}\author{#2}}
\def\showpaper{\maketitle}
\setpaper{Macro Paper}{Ada Lovelace}
\begin{document}
\showpaper
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let semantic_events = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::SetDocumentMetadata(_) | RenderEvent::FlushTitleBlock(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(semantic_events.len(), 3);
    for event in semantic_events {
        assert_eq!(event.meta.producer, EventProducer::Macro);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert!(!event.meta.source.expansion_stack.is_empty());
    }
    assert!(outcome.render_events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::SetDocumentMetadata(metadata)
            if metadata.field == MetadataField::Title
                && metadata.value == "Macro Paper"
    )));
    assert!(outcome.render_events.iter().any(|event| matches!(
        &event.event,
        RenderEvent::SetDocumentMetadata(metadata)
            if metadata.field == MetadataField::Author
                && metadata.value == "Ada Lovelace"
    )));
}

#[test]
fn generic_profile_metadata_is_emitted_only_when_executed() {
    let source = r"\iffalse
\affiliation{Hidden Institute}
\email{hidden@example.test}
\keywords{hidden}
\pacs{00.00}
\fi
\affil[1]{Analytical Engine Institute}
\institute{Difference Engine Laboratory}
\email{ada@example.test}
\keywords{preview, rendering}
\pacs{12.34.-x}
\begin{document}\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let metadata = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::SetDocumentMetadata(metadata)
                if matches!(
                    metadata.field,
                    MetadataField::Affiliation
                        | MetadataField::Correspondence
                        | MetadataField::Keywords
                        | MetadataField::Pacs
                ) =>
            {
                Some((
                    metadata.field,
                    metadata.value.as_str(),
                    event.meta.producer,
                    event.meta.confidence,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        metadata,
        vec![
            (
                MetadataField::Affiliation,
                "Analytical Engine Institute",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Affiliation,
                "Difference Engine Laboratory",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Correspondence,
                "ada@example.test",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Keywords,
                "preview, rendering",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Pacs,
                "12.34.-x",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn macro_generated_profile_metadata_tracks_expansion_provenance() {
    let source = r"\def\setprofile#1#2#3#4{%
\affiliation{#1}\email{#2}\keywords{#3}\pacs{#4}}
\setprofile{Analytical Engine Institute}{ada@example.test}{preview}{12.34.-x}
\begin{document}\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let metadata = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                &event.event,
                RenderEvent::SetDocumentMetadata(metadata)
                    if matches!(
                        metadata.field,
                        MetadataField::Affiliation
                            | MetadataField::Correspondence
                            | MetadataField::Keywords
                            | MetadataField::Pacs
                    )
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(metadata.len(), 4, "{:#?}", outcome.render_events);
    for event in metadata {
        assert_eq!(event.meta.producer, EventProducer::Macro);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert!(!event.meta.source.expansion_stack.is_empty());
    }
}

#[test]
fn icml_profile_metadata_is_emitted_only_when_executed() {
    let source = r"\iffalse
\icmltitle{Hidden Paper}
\icmlauthor{Hidden Author}{hidden}
\icmlaffiliation{hidden}{Hidden Institute}
\icmlcorrespondingauthor{Hidden Author}{hidden@example.test}
\icmlkeywords{hidden}
\printAffiliationsAndNotice{}
\fi
\begin{document}
\icmltitle{A Paper}
\icmlauthor{Ada Lovelace\thanks{Equal contribution}}{equal,engine}
\icmlaffiliation{engine}{Analytical Engine Institute}
\icmlcorrespondingauthor{Ada Lovelace}{ada@example.test}
\icmlkeywords{preview, rendering}
\printAffiliationsAndNotice{}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let metadata = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::SetDocumentMetadata(metadata) => Some((
                metadata.field,
                metadata.value.as_str(),
                event.meta.producer,
                event.meta.confidence,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let flushes = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::FlushTitleBlock(_)))
        .collect::<Vec<_>>();

    assert_eq!(
        metadata,
        vec![
            (
                MetadataField::Title,
                "A Paper",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Author,
                "Ada Lovelace",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::AuthorNote,
                "Equal contribution",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Affiliation,
                "Analytical Engine Institute",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Correspondence,
                "Ada Lovelace <ada@example.test>",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
            (
                MetadataField::Keywords,
                "preview, rendering",
                EventProducer::Primitive,
                SemanticConfidence::High,
            ),
        ],
        "{:#?}",
        outcome.render_events
    );
    assert_eq!(flushes.len(), 1, "{:#?}", outcome.render_events);
    assert_eq!(flushes[0].meta.producer, EventProducer::Primitive);
    assert_eq!(flushes[0].meta.confidence, SemanticConfidence::High);
}

#[test]
fn macro_generated_icml_metadata_tracks_expansion_provenance() {
    let source = r"\def\emitprofile#1#2#3#4{%
\icmltitle{#1}%
\icmlauthor{#2}{engine}%
\icmlaffiliation{engine}{#3}%
\icmlcorrespondingauthor{#2}{#4}%
\icmlkeywords{preview}%
\printAffiliationsAndNotice{}}
\begin{document}
\emitprofile{A Paper}{Ada Lovelace}{Analytical Engine Institute}{ada@example.test}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let events = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::SetDocumentMetadata(_) | RenderEvent::FlushTitleBlock(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(events.len(), 6, "{:#?}", outcome.render_events);
    for event in events {
        assert_eq!(event.meta.producer, EventProducer::Macro);
        assert_eq!(event.meta.confidence, SemanticConfidence::High);
        assert!(!event.meta.source.expansion_stack.is_empty());
    }
}

#[test]
fn redefined_icml_commands_suppress_scanner_recovery() {
    let source = r"\def\icmltitle#1{}
\def\icmlauthor#1#2{}
\def\icmlaffiliation#1#2{}
\def\icmlcorrespondingauthor#1#2{}
\def\icmlkeywords#1{}
\def\printAffiliationsAndNotice#1{}
\begin{document}
\icmltitle{Hidden Paper}
\icmlauthor{Hidden Author}{hidden}
\icmlaffiliation{hidden}{Hidden Institute}
\icmlcorrespondingauthor{Hidden Author}{hidden@example.test}
\icmlkeywords{hidden}
\printAffiliationsAndNotice{}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);

    assert!(
        !outcome.render_events.iter().any(|event| matches!(
            event.event,
            RenderEvent::SetDocumentMetadata(_) | RenderEvent::FlushTitleBlock(_)
        )),
        "{:#?}",
        outcome.render_events
    );
}

#[test]
fn icml_profile_prefers_preview_semantics_over_mounted_style() {
    let source = r"\usepackage{icml2020}
\begin{document}
\icmltitle{A Paper}
\icmlauthor{Ada Lovelace}{engine}
\icmlaffiliation{engine}{Analytical Engine Institute}
\icmlcorrespondingauthor{Ada Lovelace}{ada@example.test}
\icmlkeywords{preview}
\printAffiliationsAndNotice{}
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.mount_file(
        "icml2020.sty",
        r"\def\icmltitle#1{}
\def\icmlauthor#1#2{}
\def\icmlaffiliation#1#2{}
\def\icmlcorrespondingauthor#1#2{}
\def\icmlkeywords#1{}
\def\printAffiliationsAndNotice#1{}",
    );
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let metadata = outcome
        .render_events
        .iter()
        .filter(|event| matches!(event.event, RenderEvent::SetDocumentMetadata(_)))
        .collect::<Vec<_>>();
    let flush = outcome
        .render_events
        .iter()
        .find(|event| matches!(event.event, RenderEvent::FlushTitleBlock(_)))
        .expect("title-block flush");

    assert_eq!(metadata.len(), 5, "{:#?}", outcome.render_events);
    assert!(metadata.iter().all(|event| {
        event.meta.producer != EventProducer::ScannerRecovery
            && event.meta.confidence == SemanticConfidence::High
    }));
    assert_ne!(flush.meta.producer, EventProducer::ScannerRecovery);
    assert_eq!(flush.meta.confidence, SemanticConfidence::High);
}

#[test]
fn author_metadata_expands_user_macros_without_losing_separators() {
    let source = r"\def\firstauthor{Ada Lovelace}
\def\secondauthor{Grace Hopper}
\def\allauthors{\firstauthor \and \secondauthor}
\author{\allauthors}
\begin{document}
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let authors = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::SetDocumentMetadata(metadata)
                if metadata.field == MetadataField::Author =>
            {
                Some(metadata.value.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(authors, vec!["Ada Lovelace", "Grace Hopper"]);
}

#[test]
fn unused_macro_metadata_does_not_reorder_executed_assignments() {
    let source = r"\def\unused{\title{Ghost}}
\def\setpaper#1{\title{#1}}
\date{1843}
\setpaper{Right}
\begin{document}
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let metadata = outcome
        .render_events
        .iter()
        .filter_map(|event| match &event.event {
            RenderEvent::SetDocumentMetadata(metadata) => {
                Some((metadata.field, metadata.value.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        metadata,
        vec![
            (MetadataField::Date, "1843"),
            (MetadataField::Title, "Right"),
        ]
    );
}

#[test]
fn front_matter_aliases_keep_primitive_semantics() {
    let source = r"\let\papertitle\title
\let\paperauthor\author
\let\paperdate\date
\let\showpaper\maketitle
\papertitle{Alias Paper}
\paperauthor{Ada Lovelace}
\paperdate{1843}
\begin{document}
\showpaper
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let semantic_events = outcome
        .render_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                RenderEvent::SetDocumentMetadata(_) | RenderEvent::FlushTitleBlock(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(semantic_events.len(), 4);
    assert!(
        semantic_events
            .iter()
            .all(|event| event.meta.producer == EventProducer::Primitive)
    );
    assert!(
        semantic_events
            .iter()
            .all(|event| event.meta.confidence == SemanticConfidence::High)
    );
}

#[test]
fn article_bridge_executes_generic_affiliation_metadata() {
    let source = r"\documentclass{article}
\title{Bridge Paper}
\author{Ada Lovelace}
\date{1843}
\affiliation{Analytical Engine Institute}
\begin{document}
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);

    for event in &outcome.render_events {
        match &event.event {
            RenderEvent::SetDocumentMetadata(metadata)
                if matches!(
                    metadata.field,
                    MetadataField::Title
                        | MetadataField::Author
                        | MetadataField::AuthorNote
                        | MetadataField::Date
                ) =>
            {
                assert_eq!(event.meta.producer, EventProducer::Macro);
                assert_eq!(event.meta.confidence, SemanticConfidence::High);
            }
            RenderEvent::FlushTitleBlock(_) => {
                assert_eq!(event.meta.producer, EventProducer::Macro);
                assert_eq!(event.meta.confidence, SemanticConfidence::High);
            }
            _ => {}
        }
    }
    let affiliation = outcome
        .render_events
        .iter()
        .find(|event| {
            matches!(
                &event.event,
                RenderEvent::SetDocumentMetadata(metadata)
                    if metadata.field == MetadataField::Affiliation
            )
        })
        .expect("affiliation metadata");
    assert_eq!(affiliation.meta.producer, EventProducer::Primitive);
    assert_eq!(affiliation.meta.confidence, SemanticConfidence::High);
}

#[test]
fn profile_shims_delegate_metadata_to_vm_execution() {
    let cases = [
        (
            "authblk",
            r"\documentclass{article}
\usepackage{authblk}
\affil[1]{Analytical Engine Institute}
\begin{document}\maketitle\end{document}",
            vec![(MetadataField::Affiliation, "Analytical Engine Institute")],
        ),
        (
            "llncs",
            r"\documentclass{llncs}
\institute{Analytical Engine Institute}
\email{ada@example.test}
\keywords{preview}
\begin{document}\maketitle\end{document}",
            vec![
                (MetadataField::Affiliation, "Analytical Engine Institute"),
                (MetadataField::Correspondence, "ada@example.test"),
                (MetadataField::Keywords, "preview"),
            ],
        ),
        (
            "revtex",
            r"\documentclass{revtex4-2}
\affiliation{Analytical Engine Institute}
\email{ada@example.test}
\keywords{preview}
\pacs{12.34.-x}
\begin{document}\maketitle\end{document}",
            vec![
                (MetadataField::Affiliation, "Analytical Engine Institute"),
                (MetadataField::Correspondence, "ada@example.test"),
                (MetadataField::Keywords, "preview"),
                (MetadataField::Pacs, "12.34.-x"),
            ],
        ),
        (
            "wacv",
            r"\documentclass{article}
\usepackage{wacv}
\affiliation{Analytical Engine Institute}
\begin{document}\end{document}",
            vec![(MetadataField::Affiliation, "Analytical Engine Institute")],
        ),
    ];

    for (profile, source, expected) in cases {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        vm.set_entry_source_path("main.tex");
        vm.enable_render_event_capture();
        let outcome = vm.run_plain(source);
        let metadata = outcome
            .render_events
            .iter()
            .filter_map(|event| match &event.event {
                RenderEvent::SetDocumentMetadata(metadata)
                    if matches!(
                        metadata.field,
                        MetadataField::Affiliation
                            | MetadataField::Correspondence
                            | MetadataField::Keywords
                            | MetadataField::Pacs
                    ) =>
                {
                    Some((
                        metadata.field,
                        metadata.value.as_str(),
                        event.meta.producer,
                        event.meta.confidence,
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let actual = metadata
            .iter()
            .map(|(field, value, _, _)| (*field, *value))
            .collect::<Vec<_>>();

        assert_eq!(actual, expected, "{profile}: {:#?}", outcome.render_events);
        assert!(
            metadata.iter().all(|(_, _, producer, confidence)| {
                *producer != EventProducer::ScannerRecovery
                    && *confidence == SemanticConfidence::High
            }),
            "{profile}: {metadata:#?}"
        );
    }
}

#[test]
fn article_author_accepts_paragraphs_between_affiliations() {
    let source = r"\documentclass{article}
\begin{document}
\author{Ada Lovelace

Analytical Engine Institute}
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);

    assert!(
        outcome.diagnostics.iter().all(|diagnostic| !diagnostic
            .detail
            .contains("paragraph ended before \\author")),
        "{:#?}",
        outcome.diagnostics
    );
    assert!(outcome.render_events.iter().any(|event| {
        matches!(
            &event.event,
            RenderEvent::SetDocumentMetadata(metadata)
                if metadata.field == MetadataField::Author
                    && metadata.value.contains("Ada Lovelace")
                    && metadata.value.contains("Analytical Engine Institute")
        )
    }));
}

#[test]
fn ieee_author_accepts_paragraphs_between_authors_and_notes() {
    let source = r"\documentclass{IEEEtran}
\begin{document}
\author{Ada Lovelace,
Grace Hopper

\thanks{Shared correspondence note.}
}
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);

    assert!(
        outcome.diagnostics.iter().all(|diagnostic| !diagnostic
            .detail
            .contains("paragraph ended before \\author")),
        "{:#?}",
        outcome.diagnostics
    );
    assert!(outcome.render_events.iter().any(|event| {
        matches!(
            &event.event,
            RenderEvent::SetDocumentMetadata(metadata)
                if metadata.field == MetadataField::Author
                    && metadata.value.contains("Ada Lovelace")
                    && metadata.value.contains("Grace Hopper")
        )
    }));
}

#[test]
fn redefined_front_matter_commands_suppress_scanner_recovery() {
    let source = r"\def\title#1{}
\def\affiliation#1{}
\def\email#1{}
\def\keywords#1{}
\def\pacs#1{}
\title{Hidden}
\affiliation{Hidden Institute}
\email{hidden@example.test}
\keywords{hidden}
\pacs{00.00}
\begin{document}
\def\maketitle{}
\maketitle
\end{document}";
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.set_entry_source_path("main.tex");
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);

    assert!(!outcome.render_events.iter().any(|event| {
        matches!(
            event.event,
            RenderEvent::SetDocumentMetadata(_) | RenderEvent::FlushTitleBlock(_)
        )
    }));
}
