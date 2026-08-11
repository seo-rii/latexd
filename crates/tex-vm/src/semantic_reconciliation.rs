use camino::Utf8PathBuf;
use tex_render_model::{ProvenanceSpan, SourceProvenance, SourceSpan};

pub(super) fn source_locations_overlap(left: &SourceProvenance, right: &SourceProvenance) -> bool {
    file_locations(left).any(|left_span| {
        file_locations(right).any(|right_span| {
            left_span.path == right_span.path
                && left_span.start_utf8 < right_span.end_utf8
                && right_span.start_utf8 < left_span.end_utf8
        })
    })
}

pub(super) fn call_invocation_primary_anchor(
    source: &SourceProvenance,
) -> Option<(Utf8PathBuf, u32, u32)> {
    if let Some(ProvenanceSpan::File(span)) =
        source.expansion_stack.last().map(|frame| &frame.call_span)
    {
        return Some((span.path.clone(), span.start_utf8, span.end_utf8));
    }
    if let Some(span) = source.related.iter().find_map(|related| {
        if related.role != tex_render_model::SourceSpanRole::Invocation {
            return None;
        }
        match &related.span {
            ProvenanceSpan::File(span) => Some(span),
            ProvenanceSpan::Generated(_) => None,
        }
    }) {
        return Some((span.path.clone(), span.start_utf8, span.end_utf8));
    }
    match &source.primary {
        ProvenanceSpan::File(span) => Some((span.path.clone(), span.start_utf8, span.end_utf8)),
        ProvenanceSpan::Generated(_) => None,
    }
}

fn file_locations(source: &SourceProvenance) -> impl Iterator<Item = &SourceSpan> {
    std::iter::once(&source.primary)
        .chain(source.related.iter().map(|related| &related.span))
        .chain(
            source
                .expansion_stack
                .iter()
                .flat_map(|frame| std::iter::once(&frame.call_span).chain(&frame.definition_span)),
        )
        .filter_map(|span| match span {
            ProvenanceSpan::File(span) => Some(span),
            ProvenanceSpan::Generated(_) => None,
        })
}

#[cfg(test)]
mod tests {
    use tex_render_model::{
        ExpansionFrame, GeneratedBy, ProvenanceSpan, SourceProvenance, SourceSpan, SourceSpanRole,
    };

    use super::{call_invocation_primary_anchor, source_locations_overlap};

    #[test]
    fn overlap_identity_ignores_origin_metadata() {
        let left = SourceProvenance::file("main.tex", 10, 20)
            .with_generated_by(GeneratedBy::MacroExpansion);
        let mut right =
            SourceProvenance::file("main.tex", 15, 25).with_generated_by(GeneratedBy::Fallback);
        right.expansion_stack_truncated = true;

        assert!(source_locations_overlap(&left, &right));
    }

    #[test]
    fn overlap_identity_includes_related_and_expansion_locations() {
        let left = SourceProvenance::generated("left", "generated left")
            .with_related(SourceSpanRole::Argument, file_span("included.tex", 30, 40));
        let right = SourceProvenance::generated("right", "generated right").with_expansion_frame(
            ExpansionFrame {
                call_span: file_span("call.tex", 5, 10),
                definition_span: Some(file_span("included.tex", 35, 45)),
                command_name: Some("wrapper".to_string()),
            },
        );

        assert!(source_locations_overlap(&left, &right));
    }

    #[test]
    fn overlap_identity_rejects_touching_or_cross_file_locations() {
        let source = SourceProvenance::file("main.tex", 10, 20);
        let touching = SourceProvenance::file("main.tex", 20, 30);
        let other_file = SourceProvenance::file("chapter.tex", 15, 25);

        assert!(!source_locations_overlap(&source, &touching));
        assert!(!source_locations_overlap(&source, &other_file));
    }

    #[test]
    fn insertion_anchor_prefers_the_terminal_expansion_call() {
        let source = SourceProvenance::file("main.tex", 70, 80)
            .with_related(SourceSpanRole::Invocation, file_span("main.tex", 50, 60))
            .with_expansion_frame(expansion_frame(file_span("main.tex", 20, 30)))
            .with_expansion_frame(expansion_frame(file_span("main.tex", 10, 20)));

        assert_eq!(
            call_invocation_primary_anchor(&source),
            Some(("main.tex".into(), 10, 20))
        );
    }

    #[test]
    fn insertion_anchor_does_not_search_past_a_generated_terminal_call() {
        let source = SourceProvenance::file("main.tex", 70, 80)
            .with_related(SourceSpanRole::Invocation, file_span("main.tex", 50, 60))
            .with_expansion_frame(expansion_frame(file_span("main.tex", 10, 20)))
            .with_expansion_frame(expansion_frame(ProvenanceSpan::Generated(
                tex_render_model::GeneratedSpan {
                    stable_id: "terminal".to_string(),
                    description: "generated terminal frame".to_string(),
                },
            )));

        assert_eq!(
            call_invocation_primary_anchor(&source),
            Some(("main.tex".into(), 50, 60))
        );
    }

    #[test]
    fn insertion_anchor_falls_back_from_invocation_to_primary() {
        let invocation = SourceProvenance::file("main.tex", 70, 80)
            .with_related(SourceSpanRole::Invocation, file_span("main.tex", 50, 60));
        let primary = SourceProvenance::file("main.tex", 70, 80);
        let generated = SourceProvenance::generated("generated", "no file anchor");

        assert_eq!(
            call_invocation_primary_anchor(&invocation),
            Some(("main.tex".into(), 50, 60))
        );
        assert_eq!(
            call_invocation_primary_anchor(&primary),
            Some(("main.tex".into(), 70, 80))
        );
        assert_eq!(call_invocation_primary_anchor(&generated), None);
    }

    fn file_span(path: &str, start_utf8: u32, end_utf8: u32) -> ProvenanceSpan {
        ProvenanceSpan::File(SourceSpan {
            path: path.into(),
            start_utf8,
            end_utf8,
        })
    }

    fn expansion_frame(call_span: ProvenanceSpan) -> ExpansionFrame {
        ExpansionFrame {
            call_span,
            definition_span: None,
            command_name: Some("wrapper".to_string()),
        }
    }
}
