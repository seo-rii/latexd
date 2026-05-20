use tex_render_model::{
    AbstractBlock, AuxView, BibliographyBlock, BibliographyItemIr, CitationInline, DocumentIr,
    EnvironmentBlock, GraphicBlock, HeadingBlock, InlineNode, IrBlock, LabelDefinitionIr,
    LinkInline, ListBlock, ListItemIr, ListKind, MetadataField, ParagraphBlock, ReferenceInline,
    RenderEvent, RenderEventEnvelope, RenderEventStream, SourceProvenance, SourceSpanRole,
    TitleBlock,
};

pub fn build_document_ir(stream: &RenderEventStream, aux: &impl AuxView) -> DocumentIr {
    DocumentIrBuilder::new(aux).build(stream)
}

pub struct DocumentIrBuilder<'a, A: AuxView> {
    aux: &'a A,
    blocks: Vec<IrBlock>,
    labels: Vec<LabelDefinitionIr>,
    paragraph: Vec<InlineNode>,
    paragraph_source: Option<SourceProvenance>,
    abstract_content: Option<(Vec<InlineNode>, SourceProvenance)>,
    environment_content: Option<(String, Vec<InlineNode>, SourceProvenance)>,
    bibliography_items: Option<(Vec<BibliographyItemIr>, SourceProvenance)>,
    list: Option<(ListKind, Vec<ListItemIr>, SourceProvenance)>,
    list_item: Option<(Vec<InlineNode>, SourceProvenance, Option<String>)>,
    float_stack: Vec<tex_render_model::BlockKind>,
    title: Option<(String, SourceProvenance)>,
    authors: Vec<(String, SourceProvenance)>,
    date: Option<(String, SourceProvenance)>,
    metadata_sources: Vec<SourceProvenance>,
}

impl<'a, A: AuxView> DocumentIrBuilder<'a, A> {
    pub fn new(aux: &'a A) -> Self {
        Self {
            aux,
            blocks: Vec::new(),
            labels: Vec::new(),
            paragraph: Vec::new(),
            paragraph_source: None,
            abstract_content: None,
            environment_content: None,
            bibliography_items: None,
            list: None,
            list_item: None,
            float_stack: Vec::new(),
            title: None,
            authors: Vec::new(),
            date: None,
            metadata_sources: Vec::new(),
        }
    }

    pub fn build(mut self, stream: &RenderEventStream) -> DocumentIr {
        for envelope in &stream.events {
            match &envelope.event {
                RenderEvent::SetDocumentMetadata(event) => match event.field {
                    MetadataField::Title => {
                        self.title = Some((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Author => {
                        self.authors
                            .push((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Date => {
                        self.date = Some((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                },
                RenderEvent::FlushTitleBlock(_) => {
                    self.flush_paragraph();
                    let mut source = envelope.meta.source.clone();
                    let emit_span = source.primary.clone();
                    for metadata_source in std::mem::take(&mut self.metadata_sources) {
                        source = source.with_related(
                            SourceSpanRole::MetadataDefinition,
                            metadata_source.primary,
                        );
                    }
                    let title = self.title.take();
                    let title_source = title.as_ref().map(|(_, source)| {
                        source
                            .clone()
                            .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                    });
                    let authors = std::mem::take(&mut self.authors);
                    let author_sources = authors
                        .iter()
                        .map(|(_, source)| {
                            source
                                .clone()
                                .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                        })
                        .collect::<Vec<_>>();
                    let date = self.date.take();
                    let date_source = date.as_ref().map(|(_, source)| {
                        source
                            .clone()
                            .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                    });
                    self.blocks.push(IrBlock::TitleBlock(TitleBlock {
                        title: title.map(|(value, _)| value),
                        title_source,
                        authors: authors.into_iter().map(|(value, _)| value).collect(),
                        author_sources,
                        date: date.map(|(value, _)| value),
                        date_source,
                        source,
                    }));
                }
                RenderEvent::BeginBlock(event) => {
                    self.flush_paragraph();
                    match &event.block {
                        tex_render_model::BlockKind::Abstract => {
                            self.abstract_content =
                                Some((Vec::new(), envelope.meta.source.clone()));
                        }
                        tex_render_model::BlockKind::Bibliography => {
                            self.bibliography_items =
                                Some((Vec::new(), envelope.meta.source.clone()));
                        }
                        tex_render_model::BlockKind::List { list_kind } => {
                            self.list =
                                Some((*list_kind, Vec::new(), envelope.meta.source.clone()));
                            self.list_item = None;
                        }
                        tex_render_model::BlockKind::Figure
                        | tex_render_model::BlockKind::Table => {
                            self.float_stack.push(event.block.clone());
                        }
                        tex_render_model::BlockKind::Environment { name } => {
                            if let Some((name, content, source)) = self.environment_content.take() {
                                self.blocks.push(IrBlock::Environment(EnvironmentBlock {
                                    name,
                                    content,
                                    source,
                                }));
                            }
                            self.environment_content =
                                Some((name.clone(), Vec::new(), envelope.meta.source.clone()));
                        }
                    }
                }
                RenderEvent::EndBlock(event) => match &event.block {
                    tex_render_model::BlockKind::Abstract => {
                        if let Some((content, source)) = self.abstract_content.take() {
                            self.blocks
                                .push(IrBlock::Abstract(AbstractBlock { content, source }));
                        }
                    }
                    tex_render_model::BlockKind::Bibliography => {
                        if let Some((items, source)) = self.bibliography_items.take() {
                            self.blocks
                                .push(IrBlock::Bibliography(BibliographyBlock { items, source }));
                        }
                    }
                    tex_render_model::BlockKind::List { .. } => {
                        self.flush_list_item();
                        if let Some((kind, items, source)) = self.list.take() {
                            self.blocks.push(IrBlock::List(ListBlock {
                                kind,
                                items,
                                source,
                            }));
                        }
                    }
                    tex_render_model::BlockKind::Environment { name } => {
                        if let Some((open_name, content, source)) = self.environment_content.take()
                        {
                            self.blocks.push(IrBlock::Environment(EnvironmentBlock {
                                name: if open_name == *name {
                                    open_name
                                } else {
                                    name.clone()
                                },
                                content,
                                source,
                            }));
                        }
                    }
                    tex_render_model::BlockKind::Figure | tex_render_model::BlockKind::Table => {
                        if self.float_stack.last() == Some(&event.block) {
                            self.float_stack.pop();
                        } else if let Some(position) = self
                            .float_stack
                            .iter()
                            .rposition(|block| block == &event.block)
                        {
                            self.float_stack.remove(position);
                        }
                    }
                },
                RenderEvent::Heading(event) => {
                    self.flush_paragraph();
                    self.blocks.push(IrBlock::Heading(HeadingBlock {
                        level: event.level,
                        number: event.number.clone(),
                        content: vec![InlineNode::Text {
                            text: event.text.clone(),
                            source: envelope.meta.source.clone(),
                        }],
                        source: envelope.meta.source.clone(),
                    }));
                }
                RenderEvent::Text(event) => {
                    self.push_inline(
                        InlineNode::Text {
                            text: event.text.clone(),
                            source: envelope.meta.source.clone(),
                        },
                        envelope,
                    );
                }
                RenderEvent::Space(_) => {
                    self.push_inline(
                        InlineNode::Space {
                            source: envelope.meta.source.clone(),
                        },
                        envelope,
                    );
                }
                RenderEvent::LineBreak(_) => {
                    self.push_inline(
                        InlineNode::LineBreak {
                            source: envelope.meta.source.clone(),
                        },
                        envelope,
                    );
                }
                RenderEvent::InlineCitation(event) => {
                    let mut labels = Vec::new();
                    for key in &event.keys {
                        if let Some(label) = self.aux.citation_label(key, event.style_hint) {
                            labels.push(label.text);
                        }
                    }
                    let resolved_label = if labels.len() == event.keys.len() && !labels.is_empty() {
                        Some(format!("[{}]", labels.join(",")))
                    } else {
                        None
                    };
                    self.push_inline(
                        InlineNode::Citation(CitationInline {
                            keys: event.keys.clone(),
                            style_hint: event.style_hint,
                            resolved_label: resolved_label.clone(),
                            display_text: resolved_label.unwrap_or_else(|| "[?]".to_string()),
                            source: envelope.meta.source.clone(),
                        }),
                        envelope,
                    );
                }
                RenderEvent::InlineReference(event) => {
                    let mut labels = Vec::new();
                    for key in &event.keys {
                        if let Some(target) = self.aux.label_target(key) {
                            labels.push(target.number);
                        }
                    }
                    let resolved_target = if labels.len() == event.keys.len() && !labels.is_empty()
                    {
                        Some(labels.join(","))
                    } else {
                        None
                    };
                    let display_text = match (event.command.as_str(), &resolved_target) {
                        ("eqref", Some(target)) => format!("({target})"),
                        ("eqref", None) => "(?)".to_string(),
                        (_, Some(target)) => target.clone(),
                        (_, None) => "[?]".to_string(),
                    };
                    self.push_inline(
                        InlineNode::Reference(ReferenceInline {
                            keys: event.keys.clone(),
                            command: event.command.clone(),
                            resolved_target,
                            display_text,
                            source: envelope.meta.source.clone(),
                        }),
                        envelope,
                    );
                }
                RenderEvent::InlineLink(event) => {
                    self.push_inline(
                        InlineNode::Link(LinkInline {
                            target: event.target.clone(),
                            display_text: event.text.clone(),
                            source: envelope.meta.source.clone(),
                        }),
                        envelope,
                    );
                }
                RenderEvent::LabelDefinition(event) => {
                    self.labels.push(LabelDefinitionIr {
                        key: event.key.clone(),
                        source: envelope.meta.source.clone(),
                    });
                }
                RenderEvent::ListItem(event) => {
                    self.flush_list_item();
                    if self.list.is_some() {
                        self.list_item = Some((
                            Vec::new(),
                            envelope.meta.source.clone(),
                            event.marker.clone(),
                        ));
                    }
                }
                RenderEvent::InlineMath(event) => {
                    self.push_inline(
                        InlineNode::InlineMath {
                            raw_source: event.raw_source.clone(),
                            normalized_text: event.normalized_text.clone(),
                            source: envelope.meta.source.clone(),
                        },
                        envelope,
                    );
                }
                RenderEvent::DisplayMath(event) => {
                    self.flush_paragraph();
                    self.blocks
                        .push(IrBlock::DisplayMath(tex_render_model::DisplayMathBlock {
                            raw_source: event.raw_source.clone(),
                            normalized_text: event.normalized_text.clone(),
                            source: envelope.meta.source.clone(),
                        }));
                }
                RenderEvent::BibliographyItem(event) => {
                    let item = BibliographyItemIr {
                        key: event.key.clone(),
                        label: event.label_hint.clone(),
                        content: event.text.clone(),
                        source: envelope.meta.source.clone(),
                    };
                    if let Some((items, _)) = &mut self.bibliography_items {
                        items.push(item);
                    } else {
                        self.flush_paragraph();
                        self.blocks.push(IrBlock::Bibliography(BibliographyBlock {
                            items: vec![item],
                            source: envelope.meta.source.clone(),
                        }));
                    }
                }
                RenderEvent::ParagraphBreak(_) if self.list_item.is_none() => {
                    self.flush_paragraph();
                }
                RenderEvent::ParagraphBreak(_) => {}
                RenderEvent::RawFallback(event) => {
                    self.flush_paragraph();
                    self.blocks.push(IrBlock::RawFallback(
                        tex_render_model::RawFallbackIr::from_event(
                            event,
                            envelope.meta.source.clone(),
                        ),
                    ));
                }
                RenderEvent::GraphicRef(event) => {
                    self.flush_paragraph();
                    self.blocks.push(IrBlock::Graphic(GraphicBlock {
                        path: event.path.clone(),
                        options: event.options.clone(),
                        caption: None,
                        caption_source: None,
                        source: envelope.meta.source.clone(),
                    }));
                }
                RenderEvent::Caption(event) => {
                    if !matches!(
                        self.float_stack.last(),
                        Some(tex_render_model::BlockKind::Table)
                    ) && let Some(IrBlock::Graphic(block)) = self.blocks.last_mut()
                    {
                        block.caption = Some(event.text.clone());
                        block.caption_source = Some(envelope.meta.source.clone());
                    } else {
                        self.flush_paragraph();
                        self.blocks.push(IrBlock::Paragraph(ParagraphBlock {
                            content: vec![InlineNode::Text {
                                text: event.text.clone(),
                                source: envelope.meta.source.clone(),
                            }],
                            source: envelope.meta.source.clone(),
                        }));
                    }
                }
                RenderEvent::Diagnostic(_) => {}
            }
        }
        self.flush_paragraph();
        if let Some((content, source)) = self.abstract_content.take() {
            self.blocks
                .push(IrBlock::Abstract(AbstractBlock { content, source }));
        }
        if let Some((name, content, source)) = self.environment_content.take() {
            self.blocks.push(IrBlock::Environment(EnvironmentBlock {
                name,
                content,
                source,
            }));
        }
        if let Some((items, source)) = self.bibliography_items.take() {
            self.blocks
                .push(IrBlock::Bibliography(BibliographyBlock { items, source }));
        }
        self.flush_list_item();
        if let Some((kind, items, source)) = self.list.take() {
            self.blocks.push(IrBlock::List(ListBlock {
                kind,
                items,
                source,
            }));
        }
        DocumentIr::with_labels(self.blocks, self.labels)
    }

    fn push_inline(&mut self, node: InlineNode, envelope: &RenderEventEnvelope) {
        if let Some((content, _)) = &mut self.abstract_content {
            content.push(node);
            return;
        }
        if let Some((content, _, _)) = &mut self.list_item {
            if content.is_empty() && matches!(node, InlineNode::Space { .. }) {
                return;
            }
            content.push(node);
            return;
        }
        if let Some((_, content, _)) = &mut self.environment_content {
            content.push(node);
            return;
        }
        if self.paragraph_source.is_none() {
            self.paragraph_source = Some(envelope.meta.source.clone());
        }
        self.paragraph.push(node);
    }

    fn flush_list_item(&mut self) {
        let Some((content, source, marker_hint)) = self.list_item.take() else {
            return;
        };
        if let Some((kind, items, _)) = &mut self.list {
            let marker = marker_hint.unwrap_or_else(|| match kind {
                ListKind::Unordered => "-".to_string(),
                ListKind::Ordered => format!("{}.", items.len() + 1),
                ListKind::Description => String::new(),
            });
            items.push(ListItemIr {
                marker,
                content,
                source,
            });
        }
    }

    fn flush_paragraph(&mut self) {
        if self.paragraph.is_empty() {
            return;
        }
        let source = self
            .paragraph_source
            .take()
            .unwrap_or_else(|| SourceProvenance::generated("paragraph", "paragraph builder"));
        self.blocks.push(IrBlock::Paragraph(ParagraphBlock {
            content: std::mem::take(&mut self.paragraph),
            source,
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tex_render_model::{
        BeginBlockEvent, BibliographyItemEvent, BlockKind, CaptionEvent, CitationLabel,
        CitationStyleHint, FlushTitleBlockEvent, GraphicRefEvent, HeadingEvent,
        InlineCitationEvent, InlineLinkEvent, InlineNode, InlineReferenceEvent, IrBlock,
        LabelDefinitionEvent, LabelTargetView, MathSourceEvent, MetadataField, ParagraphBreakEvent,
        ParagraphBreakReason, RawFallbackEvent, RenderEvent, RenderEventEnvelope,
        RenderEventStream, SetDocumentMetadataEvent, SourceProvenance, TextEvent,
    };

    use super::build_document_ir;

    struct Labels {
        labels: BTreeMap<String, String>,
        targets: BTreeMap<String, String>,
    }

    impl tex_render_model::AuxView for Labels {
        fn citation_label(&self, key: &str, _style: CitationStyleHint) -> Option<CitationLabel> {
            self.labels
                .get(key)
                .map(|text| CitationLabel { text: text.clone() })
        }

        fn bibliography_record(
            &self,
            _key: &str,
        ) -> Option<tex_render_model::BibliographyRecordView> {
            None
        }

        fn label_target(&self, key: &str) -> Option<LabelTargetView> {
            self.targets.get(key).map(|number| LabelTargetView {
                key: key.to_string(),
                number: number.clone(),
                page: None,
            })
        }
    }

    #[test]
    fn builds_compact_paper_ir_from_events() {
        let mut next_id = 1;
        let mut push = |event| {
            let envelope = RenderEventEnvelope::new(
                next_id,
                event,
                SourceProvenance::file("main.tex", next_id as u32, next_id as u32 + 1),
            );
            next_id += 1;
            envelope
        };
        let stream = RenderEventStream::new(
            Some("compact".to_string()),
            vec![
                push(RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                    field: MetadataField::Title,
                    value: "A Paper".to_string(),
                })),
                push(RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                    field: MetadataField::Author,
                    value: "Ada Lovelace".to_string(),
                })),
                push(RenderEvent::FlushTitleBlock(FlushTitleBlockEvent)),
                push(RenderEvent::BeginBlock(BeginBlockEvent {
                    block: BlockKind::Abstract,
                })),
                push(RenderEvent::Text(TextEvent {
                    text: "Short abstract.".to_string(),
                })),
                push(RenderEvent::EndBlock(BeginBlockEvent {
                    block: BlockKind::Abstract,
                })),
                push(RenderEvent::Heading(HeadingEvent {
                    level: 1,
                    text: "Intro".to_string(),
                    number: None,
                })),
                push(RenderEvent::Text(TextEvent {
                    text: "Hello".to_string(),
                })),
                push(RenderEvent::InlineCitation(InlineCitationEvent {
                    keys: vec!["key".to_string()],
                    command: "cite".to_string(),
                    style_hint: CitationStyleHint::Numeric,
                })),
                push(RenderEvent::ParagraphBreak(ParagraphBreakEvent {
                    reason: ParagraphBreakReason::BlankLine,
                })),
                push(RenderEvent::DisplayMath(MathSourceEvent {
                    raw_source: "x^2".to_string(),
                    normalized_text: None,
                })),
                push(RenderEvent::BeginBlock(BeginBlockEvent {
                    block: BlockKind::Bibliography,
                })),
                push(RenderEvent::BibliographyItem(BibliographyItemEvent {
                    key: "key".to_string(),
                    label_hint: None,
                    text: "Author. Title.".to_string(),
                })),
                push(RenderEvent::EndBlock(BeginBlockEvent {
                    block: BlockKind::Bibliography,
                })),
            ],
        );

        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
        );

        assert_eq!(ir.blocks.len(), 6);
        let text = ir.extracted_text();
        assert!(text.contains("A Paper"));
        assert!(text.contains("Ada Lovelace"));
        assert!(text.contains("Short abstract."));
        assert!(text.contains("Intro"));
        assert!(text.contains("Hello[?]"));
        assert!(text.contains("Author. Title."));
        assert!(!text.contains("key."));
        let title_block = ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::TitleBlock(title) => Some(title),
                _ => None,
            })
            .expect("title block");
        assert!(matches!(
            title_block.title_source.as_ref().map(|source| &source.primary),
            Some(tex_render_model::ProvenanceSpan::File(span))
                if span.start_utf8 == 1 && span.end_utf8 == 2
        ));
        assert!(
            title_block
                .title_source
                .as_ref()
                .is_some_and(|source| source.related.iter().any(|related| {
                    related.role == tex_render_model::SourceSpanRole::EmitSite
                        && matches!(
                            &related.span,
                            tex_render_model::ProvenanceSpan::File(span)
                                if span.start_utf8 == 3 && span.end_utf8 == 4
                        )
                }))
        );
        assert!(matches!(
            title_block.author_sources.first().map(|source| &source.primary),
            Some(tex_render_model::ProvenanceSpan::File(span))
                if span.start_utf8 == 2 && span.end_utf8 == 3
        ));
        assert!(matches!(
            &title_block.source.primary,
            tex_render_model::ProvenanceSpan::File(span)
                if span.start_utf8 == 3 && span.end_utf8 == 4
        ));
    }

    #[test]
    fn resolved_numeric_citations_render_labels() {
        let stream = RenderEventStream::new(
            Some("citation".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::InlineCitation(InlineCitationEvent {
                    keys: vec!["alpha".to_string(), "beta".to_string()],
                    command: "cite".to_string(),
                    style_hint: CitationStyleHint::Numeric,
                }),
                SourceProvenance::file("main.tex", 0, 12),
            )],
        );
        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::from([
                    ("alpha".to_string(), "1".to_string()),
                    ("beta".to_string(), "2".to_string()),
                ]),
                targets: BTreeMap::new(),
            },
        );

        assert_eq!(ir.extracted_text(), "[1,2]");
    }

    #[test]
    fn references_resolve_through_aux_targets() {
        let stream = RenderEventStream::new(
            Some("reference".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::InlineReference(InlineReferenceEvent {
                    keys: vec!["eq:main".to_string()],
                    command: "eqref".to_string(),
                }),
                SourceProvenance::file("main.tex", 0, 12),
            )],
        );
        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::new(),
                targets: BTreeMap::from([("eq:main".to_string(), "2.1".to_string())]),
            },
        );

        assert_eq!(ir.extracted_text(), "(2.1)");
    }

    #[test]
    fn inline_links_preserve_display_text_and_target() {
        let stream = RenderEventStream::new(
            Some("link".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::InlineLink(InlineLinkEvent {
                    target: "https://example.test/paper".to_string(),
                    text: "paper link".to_string(),
                    command: "href".to_string(),
                }),
                SourceProvenance::file("main.tex", 0, 12),
            )],
        );
        let ir = build_document_ir(&stream, &());

        assert_eq!(ir.extracted_text(), "paper link");
        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Paragraph(paragraph)]
                if matches!(
                    paragraph.content.as_slice(),
                    [tex_render_model::InlineNode::Link(link)]
                        if link.target == "https://example.test/paper"
                            && link.display_text == "paper link"
                )
        ));
    }

    #[test]
    fn label_definitions_are_invisible_ir_metadata() {
        let stream = RenderEventStream::new(
            Some("label".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::Text(TextEvent {
                        text: "Intro".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 0, 5),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::LabelDefinition(LabelDefinitionEvent {
                        key: "sec:intro".to_string(),
                        command: "label".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 12, 21),
                ),
            ],
        );
        let ir = build_document_ir(&stream, &());

        assert_eq!(ir.labels.len(), 1);
        assert_eq!(ir.labels[0].key, "sec:intro");
        assert_eq!(ir.extracted_text(), "Intro");
        assert!(!ir.extracted_text().contains("sec:intro"));
    }

    #[test]
    fn raw_fallback_becomes_block_without_losing_visible_text() {
        let stream = RenderEventStream::new(
            Some("fallback".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::RawFallback(RawFallbackEvent {
                    source_excerpt: "\\begin{unknownenv}Fallback text.\\end{unknownenv}"
                        .to_string(),
                    expanded_text: Some("Expanded fallback text.".to_string()),
                    normalized_visible_text: Some("Fallback text.".to_string()),
                    environment: Some("unknownenv".to_string()),
                    reason: tex_render_model::FallbackReason::UnsupportedEnvironment,
                    source_hash: Some("blake3:raw-fallback".to_string()),
                    full_source_artifact: Some("fallbacks/raw-1.tex".to_string()),
                    truncated: true,
                }),
                SourceProvenance::file("main.tex", 0, 48),
            )],
        );
        let ir = build_document_ir(&stream, &());

        assert_eq!(ir.extracted_text(), "Fallback text.");
        let IrBlock::RawFallback(fallback) = &ir.blocks[0] else {
            panic!("expected raw fallback block");
        };
        assert_eq!(
            fallback.expanded_text.as_deref(),
            Some("Expanded fallback text.")
        );
        assert_eq!(fallback.source_hash.as_deref(), Some("blake3:raw-fallback"));
        assert_eq!(
            fallback.full_source_artifact.as_deref(),
            Some("fallbacks/raw-1.tex")
        );
        assert!(fallback.truncated);
    }

    #[test]
    fn graphic_ref_and_caption_become_graphic_block() {
        let stream = RenderEventStream::new(
            Some("graphic".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::GraphicRef(GraphicRefEvent {
                        path: "figures/plot.pdf".to_string(),
                        options: Some("width=0.8\\linewidth".to_string()),
                    }),
                    SourceProvenance::file("main.tex", 0, 30),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::Caption(CaptionEvent {
                        text: "Plot caption.".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 31, 52),
                ),
            ],
        );
        let ir = build_document_ir(&stream, &());

        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Graphic(block)]
                if block.path == "figures/plot.pdf"
                    && block.options.as_deref() == Some("width=0.8\\linewidth")
                    && block.caption.as_deref() == Some("Plot caption.")
                    && block.caption_source.is_some()
        ));
        assert_eq!(ir.extracted_text(), "Plot caption.");
    }

    #[test]
    fn table_caption_does_not_overwrite_previous_graphic_caption() {
        let stream = RenderEventStream::new(
            Some("table-caption".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::BeginBlock(BeginBlockEvent {
                        block: BlockKind::Figure,
                    }),
                    SourceProvenance::file("main.tex", 0, 14),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::GraphicRef(GraphicRefEvent {
                        path: "figures/plot.pdf".to_string(),
                        options: None,
                    }),
                    SourceProvenance::file("main.tex", 15, 45),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::Caption(CaptionEvent {
                        text: "Plot caption.".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 46, 67),
                ),
                RenderEventEnvelope::new(
                    4,
                    RenderEvent::EndBlock(BeginBlockEvent {
                        block: BlockKind::Figure,
                    }),
                    SourceProvenance::file("main.tex", 68, 80),
                ),
                RenderEventEnvelope::new(
                    5,
                    RenderEvent::BeginBlock(BeginBlockEvent {
                        block: BlockKind::Table,
                    }),
                    SourceProvenance::file("main.tex", 81, 94),
                ),
                RenderEventEnvelope::new(
                    6,
                    RenderEvent::Caption(CaptionEvent {
                        text: "Table caption.".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 95, 117),
                ),
                RenderEventEnvelope::new(
                    7,
                    RenderEvent::EndBlock(BeginBlockEvent {
                        block: BlockKind::Table,
                    }),
                    SourceProvenance::file("main.tex", 118, 130),
                ),
            ],
        );
        let ir = build_document_ir(&stream, &());

        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Graphic(graphic), IrBlock::Paragraph(paragraph)]
                if graphic.caption.as_deref() == Some("Plot caption.")
                    && paragraph.content.iter().any(|node| {
                        matches!(
                            node,
                            InlineNode::Text { text, .. } if text == "Table caption."
                        )
                    })
        ));
        assert_eq!(ir.extracted_text(), "Plot caption.\nTable caption.");
    }
}
