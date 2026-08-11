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

    use super::source_locations_overlap;

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

    fn file_span(path: &str, start_utf8: u32, end_utf8: u32) -> ProvenanceSpan {
        ProvenanceSpan::File(SourceSpan {
            path: path.into(),
            start_utf8,
            end_utf8,
        })
    }
}
