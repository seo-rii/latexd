use tex_render_model::{
    AbstractBlock, AuxView, BibliographyBlock, BibliographyItemIr, CitationInline, DocumentIr,
    GraphicBlock, HeadingBlock, InlineNode, IrBlock, MetadataField, ParagraphBlock, RenderEvent,
    RenderEventEnvelope, RenderEventStream, SourceProvenance, SourceSpanRole, TitleBlock,
};

pub fn build_document_ir(stream: &RenderEventStream, aux: &impl AuxView) -> DocumentIr {
    DocumentIrBuilder::new(aux).build(stream)
}

pub struct DocumentIrBuilder<'a, A: AuxView> {
    aux: &'a A,
    blocks: Vec<IrBlock>,
    paragraph: Vec<InlineNode>,
    paragraph_source: Option<SourceProvenance>,
    abstract_content: Option<(Vec<InlineNode>, SourceProvenance)>,
    bibliography_items: Option<(Vec<BibliographyItemIr>, SourceProvenance)>,
    title: Option<String>,
    authors: Vec<String>,
    date: Option<String>,
    metadata_sources: Vec<SourceProvenance>,
}

impl<'a, A: AuxView> DocumentIrBuilder<'a, A> {
    pub fn new(aux: &'a A) -> Self {
        Self {
            aux,
            blocks: Vec::new(),
            paragraph: Vec::new(),
            paragraph_source: None,
            abstract_content: None,
            bibliography_items: None,
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
                        self.title = Some(event.value.clone());
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Author => {
                        self.authors.push(event.value.clone());
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Date => {
                        self.date = Some(event.value.clone());
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                },
                RenderEvent::FlushTitleBlock(_) => {
                    self.flush_paragraph();
                    let mut source = envelope.meta.source.clone();
                    for metadata_source in std::mem::take(&mut self.metadata_sources) {
                        source = source.with_related(
                            SourceSpanRole::MetadataDefinition,
                            metadata_source.primary,
                        );
                    }
                    self.blocks.push(IrBlock::TitleBlock(TitleBlock {
                        title: self.title.take(),
                        authors: std::mem::take(&mut self.authors),
                        date: self.date.take(),
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
                        _ => {}
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
                    _ => {}
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
                RenderEvent::ParagraphBreak(_) => {
                    self.flush_paragraph();
                }
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
                        source: envelope.meta.source.clone(),
                    }));
                }
                RenderEvent::Caption(event) => {
                    if let Some(IrBlock::Graphic(block)) = self.blocks.last_mut() {
                        block.caption = Some(event.text.clone());
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
        if let Some((items, source)) = self.bibliography_items.take() {
            self.blocks
                .push(IrBlock::Bibliography(BibliographyBlock { items, source }));
        }
        DocumentIr::new(self.blocks)
    }

    fn push_inline(&mut self, node: InlineNode, envelope: &RenderEventEnvelope) {
        if let Some((content, _)) = &mut self.abstract_content {
            content.push(node);
            return;
        }
        if self.paragraph_source.is_none() {
            self.paragraph_source = Some(envelope.meta.source.clone());
        }
        self.paragraph.push(node);
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
        InlineCitationEvent, IrBlock, MathSourceEvent, MetadataField, ParagraphBreakEvent,
        ParagraphBreakReason, RawFallbackEvent, RenderEvent, RenderEventEnvelope,
        RenderEventStream, SetDocumentMetadataEvent, SourceProvenance, TextEvent,
    };

    use super::build_document_ir;

    struct Labels {
        labels: BTreeMap<String, String>,
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

        fn label_target(&self, _key: &str) -> Option<tex_render_model::LabelTargetView> {
            None
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
            },
        );

        assert_eq!(ir.extracted_text(), "[1,2]");
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
                    expanded_text: None,
                    normalized_visible_text: Some("Fallback text.".to_string()),
                    environment: Some("unknownenv".to_string()),
                    reason: tex_render_model::FallbackReason::UnsupportedEnvironment,
                    source_hash: None,
                    full_source_artifact: None,
                    truncated: false,
                }),
                SourceProvenance::file("main.tex", 0, 48),
            )],
        );
        let ir = build_document_ir(&stream, &());

        assert_eq!(ir.extracted_text(), "Fallback text.");
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
        ));
        assert_eq!(ir.extracted_text(), "Plot caption.");
    }
}
