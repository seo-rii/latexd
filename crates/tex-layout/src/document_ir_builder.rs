use std::collections::{BTreeMap, BTreeSet};

use tex_render_model::{
    AbstractBlock, AuxView, BibliographyBlock, BibliographyItemIr, CitationInline,
    CitationLabelForm, CitationStyleHint, DocumentClassIr, DocumentIr, DocumentLayoutIntent,
    EnvironmentBlock, FloatBlock, FloatKind, FloatPlacement, FootnoteAnchor, FootnoteCommandKind,
    FootnoteId, FootnoteIr, GraphicBlock, HeadingBlock, InlineNode, IrBlock, LabelDefinitionIr,
    LayoutContainerBlock, LinkInline, ListBlock, ListItemIr, ListKind, MetadataField,
    PageBreakBlock, ParagraphBlock, ReferenceInline, RenderEvent, RenderEventEnvelope,
    RenderEventStream, SourceProvenance, SourceSpanRole, TableBlock, TableCell, TableRow,
    TableRulePosition, TitleBlock,
};

use crate::math_ir::parse_display_math_structure;

pub fn build_document_ir(stream: &RenderEventStream, aux: &impl AuxView) -> DocumentIr {
    DocumentIrBuilder::new(aux).build(stream)
}

fn trim_trailing_spaces(content: &mut Vec<InlineNode>) {
    while matches!(content.last(), Some(InlineNode::Space { .. })) {
        content.pop();
    }
}

struct ActiveFootnote {
    note_id: FootnoteId,
    marker: String,
    command: FootnoteCommandKind,
    content: Vec<InlineNode>,
    source: SourceProvenance,
}

struct ActiveFloat {
    kind: FloatKind,
    full_width: bool,
    children: Vec<IrBlock>,
    captions: Vec<ActiveFloatCaption>,
    source: SourceProvenance,
}

struct ActiveFloatCaption {
    text: String,
    source: SourceProvenance,
    after_child_count: usize,
}

impl ActiveFloat {
    fn into_block(mut self) -> IrBlock {
        let outer_caption_index = match self.captions.as_slice() {
            [] => None,
            [_] => Some(0),
            captions => (captions[captions.len() - 1].after_child_count
                == captions[captions.len() - 2].after_child_count)
                .then_some(captions.len() - 1),
        };
        let outer_caption = outer_caption_index.map(|index| self.captions.remove(index));
        for caption in self.captions {
            let preceding_end = caption.after_child_count.min(self.children.len());
            let target_index = self.children[..preceding_end]
                .iter()
                .rposition(|block| match block {
                    IrBlock::Graphic(graphic) | IrBlock::FullWidthGraphic(graphic)
                        if graphic.caption.is_none() =>
                    {
                        true
                    }
                    _ => false,
                })
                .or_else(|| {
                    self.children[preceding_end..]
                        .iter()
                        .position(|block| match block {
                            IrBlock::Graphic(graphic) | IrBlock::FullWidthGraphic(graphic)
                                if graphic.caption.is_none() =>
                            {
                                true
                            }
                            _ => false,
                        })
                        .map(|index| preceding_end + index)
                });
            if let Some(target_index) = target_index {
                let graphic = match &mut self.children[target_index] {
                    IrBlock::Graphic(graphic) | IrBlock::FullWidthGraphic(graphic) => graphic,
                    _ => unreachable!("selected graphic child"),
                };
                graphic.caption = Some(caption.text);
                graphic.caption_source = Some(caption.source);
            } else {
                self.children.push(IrBlock::Paragraph(ParagraphBlock {
                    content: vec![InlineNode::Text {
                        text: caption.text,
                        source: caption.source.clone(),
                    }],
                    source: caption.source,
                }));
            }
        }
        IrBlock::Float(FloatBlock {
            kind: self.kind,
            placement: FloatPlacement::Top,
            full_width: self.full_width,
            children: self.children,
            caption: outer_caption.as_ref().map(|caption| caption.text.clone()),
            caption_source: outer_caption.map(|caption| caption.source),
            source: self.source,
        })
    }
}

fn float_kind_and_width(block: &tex_render_model::BlockKind) -> Option<(FloatKind, bool)> {
    match block {
        tex_render_model::BlockKind::Figure => Some((FloatKind::Figure, false)),
        tex_render_model::BlockKind::FullWidthFigure => Some((FloatKind::Figure, true)),
        tex_render_model::BlockKind::Table => Some((FloatKind::Table, false)),
        tex_render_model::BlockKind::FullWidthTable => Some((FloatKind::Table, true)),
        _ => None,
    }
}

pub struct DocumentIrBuilder<'a, A: AuxView> {
    aux: &'a A,
    blocks: Vec<IrBlock>,
    layout_container_stack: Vec<LayoutContainerBlock>,
    labels: Vec<LabelDefinitionIr>,
    footnotes: Vec<FootnoteIr>,
    active_footnote: Option<ActiveFootnote>,
    footnote_markers: BTreeMap<FootnoteId, String>,
    footnote_anchor_ids: BTreeSet<FootnoteId>,
    next_footnote_number: u32,
    paragraph: Vec<InlineNode>,
    paragraph_source: Option<SourceProvenance>,
    abstract_content: Option<(Vec<InlineNode>, SourceProvenance)>,
    environment_content: Option<(String, Vec<InlineNode>, SourceProvenance)>,
    bibliography_items: Option<(Vec<BibliographyItemIr>, SourceProvenance)>,
    list: Option<(ListKind, Vec<ListItemIr>, SourceProvenance)>,
    list_item: Option<(Vec<InlineNode>, SourceProvenance, Option<String>)>,
    float_stack: Vec<ActiveFloat>,
    document_class: Option<DocumentClassIr>,
    layout: Option<DocumentLayoutIntent>,
    title: Option<(String, SourceProvenance)>,
    authors: Vec<(String, SourceProvenance)>,
    author_notes: Vec<(String, SourceProvenance)>,
    affiliations: Vec<(String, SourceProvenance)>,
    correspondence: Vec<(String, SourceProvenance)>,
    date: Option<(String, SourceProvenance)>,
    keywords: Vec<(String, SourceProvenance)>,
    pacs: Vec<(String, SourceProvenance)>,
    metadata_sources: Vec<SourceProvenance>,
}

impl<'a, A: AuxView> DocumentIrBuilder<'a, A> {
    pub fn new(aux: &'a A) -> Self {
        Self {
            aux,
            blocks: Vec::new(),
            layout_container_stack: Vec::new(),
            labels: Vec::new(),
            footnotes: Vec::new(),
            active_footnote: None,
            footnote_markers: BTreeMap::new(),
            footnote_anchor_ids: BTreeSet::new(),
            next_footnote_number: 1,
            paragraph: Vec::new(),
            paragraph_source: None,
            abstract_content: None,
            environment_content: None,
            bibliography_items: None,
            list: None,
            list_item: None,
            float_stack: Vec::new(),
            document_class: None,
            layout: None,
            title: None,
            authors: Vec::new(),
            author_notes: Vec::new(),
            affiliations: Vec::new(),
            correspondence: Vec::new(),
            date: None,
            keywords: Vec::new(),
            pacs: Vec::new(),
            metadata_sources: Vec::new(),
        }
    }

    pub fn build(mut self, stream: &RenderEventStream) -> DocumentIr {
        for envelope in &stream.events {
            match &envelope.event {
                RenderEvent::DocumentClass(event) => {
                    self.document_class = Some(DocumentClassIr {
                        name: event.name.clone(),
                        options: event.options.clone(),
                        source: envelope.meta.source.clone(),
                    });
                }
                RenderEvent::SetDocumentLayout(event) => {
                    let layout = self
                        .layout
                        .get_or_insert_with(DocumentLayoutIntent::default);
                    if event.profile.is_some() {
                        layout.profile.clone_from(&event.profile);
                    }
                    if event.text_font_family.is_some() {
                        layout.text_font_family.clone_from(&event.text_font_family);
                    }
                    if event.page_width_pt_milli.is_some() {
                        layout.page_width_pt_milli = event.page_width_pt_milli;
                    }
                    if event.page_height_pt_milli.is_some() {
                        layout.page_height_pt_milli = event.page_height_pt_milli;
                    }
                    if event.text_width_pt_milli.is_some() {
                        layout.text_width_pt_milli = event.text_width_pt_milli;
                    }
                    if event.text_height_pt_milli.is_some() {
                        layout.text_height_pt_milli = event.text_height_pt_milli;
                    }
                    if event.margin_left_pt_milli.is_some() {
                        layout.margin_left_pt_milli = event.margin_left_pt_milli;
                    }
                    if event.margin_top_pt_milli.is_some() {
                        layout.margin_top_pt_milli = event.margin_top_pt_milli;
                    }
                    if event.front_matter_top_pt_milli.is_some() {
                        layout.front_matter_top_pt_milli = event.front_matter_top_pt_milli;
                    }
                    if event.column_count.is_some() {
                        layout.column_count = event.column_count;
                    }
                    if event.column_gap_pt_milli.is_some() {
                        layout.column_gap_pt_milli = event.column_gap_pt_milli;
                    }
                    if event.body_font_size_pt_milli.is_some() {
                        layout.body_font_size_pt_milli = event.body_font_size_pt_milli;
                    }
                    if event.line_height_pt_milli.is_some() {
                        layout.line_height_pt_milli = event.line_height_pt_milli;
                    }
                    if event.heading_font_size_pt_milli.is_some() {
                        layout.heading_font_size_pt_milli = event.heading_font_size_pt_milli;
                    }
                    if event.title_font_size_pt_milli.is_some() {
                        layout.title_font_size_pt_milli = event.title_font_size_pt_milli;
                    }
                    if event.block_gap_pt_milli.is_some() {
                        layout.block_gap_pt_milli = event.block_gap_pt_milli;
                    }
                    if event.abstract_indent_pt_milli.is_some() {
                        layout.abstract_indent_pt_milli = event.abstract_indent_pt_milli;
                    }
                }
                RenderEvent::PageBreak(event) => {
                    self.flush_paragraph();
                    self.push_block(IrBlock::PageBreak(PageBreakBlock {
                        kind: event.kind,
                        source: envelope.meta.source.clone(),
                    }));
                }
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
                    MetadataField::AuthorNote => {
                        self.author_notes
                            .push((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Affiliation => {
                        self.affiliations
                            .push((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Correspondence => {
                        self.correspondence
                            .push((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Date => {
                        self.date = Some((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Keywords => {
                        self.keywords
                            .push((event.value.clone(), envelope.meta.source.clone()));
                        self.metadata_sources.push(envelope.meta.source.clone());
                    }
                    MetadataField::Pacs => {
                        self.pacs
                            .push((event.value.clone(), envelope.meta.source.clone()));
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
                    let author_notes = std::mem::take(&mut self.author_notes);
                    let author_note_sources = author_notes
                        .iter()
                        .map(|(_, source)| {
                            source
                                .clone()
                                .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                        })
                        .collect::<Vec<_>>();
                    let affiliations = std::mem::take(&mut self.affiliations);
                    let affiliation_sources = affiliations
                        .iter()
                        .map(|(_, source)| {
                            source
                                .clone()
                                .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                        })
                        .collect::<Vec<_>>();
                    let correspondence = std::mem::take(&mut self.correspondence);
                    let correspondence_sources = correspondence
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
                    let keywords = std::mem::take(&mut self.keywords);
                    let keyword_sources = keywords
                        .iter()
                        .map(|(_, source)| {
                            source
                                .clone()
                                .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                        })
                        .collect::<Vec<_>>();
                    let pacs = std::mem::take(&mut self.pacs);
                    let pacs_sources = pacs
                        .iter()
                        .map(|(_, source)| {
                            source
                                .clone()
                                .with_related(SourceSpanRole::EmitSite, emit_span.clone())
                        })
                        .collect::<Vec<_>>();
                    self.push_block(IrBlock::TitleBlock(TitleBlock {
                        title: title.map(|(value, _)| value),
                        title_source,
                        authors: authors.into_iter().map(|(value, _)| value).collect(),
                        author_sources,
                        author_notes: author_notes.into_iter().map(|(value, _)| value).collect(),
                        author_note_sources,
                        affiliations: affiliations.into_iter().map(|(value, _)| value).collect(),
                        affiliation_sources,
                        correspondence: correspondence
                            .into_iter()
                            .map(|(value, _)| value)
                            .collect(),
                        correspondence_sources,
                        date: date.map(|(value, _)| value),
                        date_source,
                        keywords: keywords.into_iter().map(|(value, _)| value).collect(),
                        keyword_sources,
                        pacs: pacs.into_iter().map(|(value, _)| value).collect(),
                        pacs_sources,
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
                        | tex_render_model::BlockKind::FullWidthFigure
                        | tex_render_model::BlockKind::Table
                        | tex_render_model::BlockKind::FullWidthTable => {
                            let (kind, full_width) = float_kind_and_width(&event.block)
                                .expect("matched float block kind");
                            self.float_stack.push(ActiveFloat {
                                kind,
                                full_width,
                                children: Vec::new(),
                                captions: Vec::new(),
                                source: envelope.meta.source.clone(),
                            });
                        }
                        tex_render_model::BlockKind::Environment { name } => {
                            if let Some((name, content, source)) = self.environment_content.take() {
                                self.push_block(IrBlock::Environment(EnvironmentBlock {
                                    name,
                                    content,
                                    source,
                                }));
                            }
                            if matches!(name.as_str(), "algorithm" | "algorithm*") {
                                self.layout_container_stack.push(LayoutContainerBlock {
                                    name: name.clone(),
                                    width_spec: "\\linewidth".to_string(),
                                    alignment: Some(tex_render_model::LayoutAlignment::Top),
                                    height_spec: None,
                                    inner_alignment: Some(tex_render_model::LayoutAlignment::Top),
                                    children: Vec::new(),
                                    source: envelope.meta.source.clone(),
                                });
                            } else {
                                self.environment_content =
                                    Some((name.clone(), Vec::new(), envelope.meta.source.clone()));
                            }
                        }
                    }
                }
                RenderEvent::EndBlock(event) => match &event.block {
                    tex_render_model::BlockKind::Abstract => {
                        if let Some((mut content, source)) = self.abstract_content.take() {
                            trim_trailing_spaces(&mut content);
                            self.push_block(IrBlock::Abstract(AbstractBlock { content, source }));
                        }
                    }
                    tex_render_model::BlockKind::Bibliography => {
                        if let Some((items, source)) = self.bibliography_items.take() {
                            self.push_block(IrBlock::Bibliography(BibliographyBlock {
                                items,
                                source,
                            }));
                        }
                    }
                    tex_render_model::BlockKind::List { .. } => {
                        self.flush_list_item();
                        if let Some((kind, items, source)) = self.list.take() {
                            self.push_block(IrBlock::List(ListBlock {
                                kind,
                                items,
                                source,
                            }));
                        }
                    }
                    tex_render_model::BlockKind::Environment { name } => {
                        if matches!(name.as_str(), "algorithm" | "algorithm*") {
                            self.flush_paragraph();
                            if let Some(position) = self
                                .layout_container_stack
                                .iter()
                                .rposition(|container| container.name == *name)
                            {
                                while self.layout_container_stack.len() > position + 1 {
                                    let child = self
                                        .layout_container_stack
                                        .pop()
                                        .expect("nested algorithm container");
                                    self.layout_container_stack
                                        .last_mut()
                                        .expect("parent algorithm container")
                                        .children
                                        .push(IrBlock::LayoutContainer(child));
                                }
                                let container = self
                                    .layout_container_stack
                                    .pop()
                                    .expect("matching algorithm container");
                                self.push_block(IrBlock::LayoutContainer(container));
                            }
                        } else if let Some((open_name, mut content, source)) =
                            self.environment_content.take()
                        {
                            trim_trailing_spaces(&mut content);
                            self.push_block(IrBlock::Environment(EnvironmentBlock {
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
                    tex_render_model::BlockKind::Figure
                    | tex_render_model::BlockKind::FullWidthFigure
                    | tex_render_model::BlockKind::Table
                    | tex_render_model::BlockKind::FullWidthTable => {
                        self.flush_paragraph();
                        let Some((kind, full_width)) = float_kind_and_width(&event.block) else {
                            unreachable!("matched float block kind");
                        };
                        let Some(position) = self.float_stack.iter().rposition(|active| {
                            active.kind == kind && active.full_width == full_width
                        }) else {
                            continue;
                        };
                        while self.float_stack.len() > position + 1 {
                            let nested = self.float_stack.pop().expect("nested float");
                            self.float_stack
                                .last_mut()
                                .expect("parent float")
                                .children
                                .push(nested.into_block());
                        }
                        let active = self.float_stack.pop().expect("matching float");
                        self.push_block(active.into_block());
                    }
                },
                RenderEvent::BeginLayoutContainer(event) => {
                    self.flush_paragraph();
                    self.layout_container_stack.push(LayoutContainerBlock {
                        name: event.name.clone(),
                        width_spec: event.width_spec.clone(),
                        alignment: event.alignment,
                        height_spec: event.height_spec.clone(),
                        inner_alignment: event.inner_alignment,
                        children: Vec::new(),
                        source: envelope.meta.source.clone(),
                    });
                }
                RenderEvent::EndLayoutContainer(event) => {
                    self.flush_paragraph();
                    if let Some(position) = self
                        .layout_container_stack
                        .iter()
                        .rposition(|container| container.name == event.name)
                    {
                        while self.layout_container_stack.len() > position + 1 {
                            let child = self
                                .layout_container_stack
                                .pop()
                                .expect("nested layout container");
                            self.layout_container_stack
                                .last_mut()
                                .expect("parent layout container")
                                .children
                                .push(IrBlock::LayoutContainer(child));
                        }
                        let container = self
                            .layout_container_stack
                            .pop()
                            .expect("matching layout container");
                        self.push_block(IrBlock::LayoutContainer(container));
                    }
                }
                RenderEvent::Heading(event) => {
                    self.flush_paragraph();
                    self.push_block(IrBlock::Heading(HeadingBlock {
                        level: event.level,
                        number: event.number.clone(),
                        content: vec![InlineNode::Text {
                            text: event.text.clone(),
                            source: envelope.meta.source.clone(),
                        }],
                        source: envelope.meta.source.clone(),
                    }));
                }
                RenderEvent::BeginFootnote(event) => {
                    self.finish_active_footnote(None);
                    let marker = self.resolve_footnote_marker(event.note_id, event.marker.as_ref());
                    if event.draw_reference || !self.footnote_anchor_ids.contains(&event.note_id) {
                        self.push_inline(
                            InlineNode::FootnoteAnchor(FootnoteAnchor {
                                note_id: event.note_id,
                                marker: marker.clone(),
                                draw_reference: event.draw_reference,
                                source: envelope.meta.source.clone(),
                            }),
                            envelope,
                        );
                        self.footnote_anchor_ids.insert(event.note_id);
                    }
                    self.active_footnote = Some(ActiveFootnote {
                        note_id: event.note_id,
                        marker,
                        command: event.command,
                        content: Vec::new(),
                        source: envelope.meta.source.clone(),
                    });
                }
                RenderEvent::EndFootnote(event) => {
                    self.finish_active_footnote(Some(event.note_id));
                }
                RenderEvent::FootnoteMark(event) => {
                    let marker = self.resolve_footnote_marker(event.note_id, event.marker.as_ref());
                    if self.footnote_anchor_ids.insert(event.note_id) {
                        self.push_inline(
                            InlineNode::FootnoteAnchor(FootnoteAnchor {
                                note_id: event.note_id,
                                marker,
                                draw_reference: true,
                                source: envelope.meta.source.clone(),
                            }),
                            envelope,
                        );
                    }
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
                            labels.push(label);
                        }
                    }
                    let resolved_label = if labels.len() == event.keys.len() && !labels.is_empty() {
                        let form = labels[0].form;
                        if labels.iter().any(|label| label.form != form) {
                            None
                        } else {
                            let texts = labels
                                .iter()
                                .map(|label| label.text.clone())
                                .collect::<Vec<_>>();
                            match form {
                                CitationLabelForm::Numeric => {
                                    let mut compacted_labels = Vec::new();
                                    let mut index = 0usize;
                                    while index < texts.len() {
                                        let Some(start) = texts[index].parse::<i64>().ok() else {
                                            compacted_labels.push(texts[index].clone());
                                            index += 1;
                                            continue;
                                        };
                                        let mut end_index = index;
                                        let mut end = start;
                                        while end_index + 1 < texts.len()
                                            && texts[end_index + 1].parse::<i64>().ok()
                                                == Some(end + 1)
                                        {
                                            end_index += 1;
                                            end += 1;
                                        }
                                        if end_index - index + 1 >= 3 {
                                            compacted_labels.push(format!("{start}-{end}"));
                                        } else {
                                            compacted_labels
                                                .extend(texts[index..=end_index].iter().cloned());
                                        }
                                        index = end_index + 1;
                                    }
                                    Some(format!("[{}]", compacted_labels.join(",")))
                                }
                                CitationLabelForm::Textual => Some(texts.join("; ")),
                                CitationLabelForm::Parenthetical => {
                                    Some(format!("({})", texts.join("; ")))
                                }
                            }
                        }
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
                    if !self.layout_container_stack.is_empty()
                        && let Some((name, mut content, source)) = self.environment_content.take()
                    {
                        trim_trailing_spaces(&mut content);
                        if !content.is_empty() {
                            self.push_block(IrBlock::Environment(EnvironmentBlock {
                                name: name.clone(),
                                content,
                                source: source.clone(),
                            }));
                        }
                        self.environment_content = Some((name, Vec::new(), source));
                    }
                    self.flush_paragraph();
                    self.push_block(IrBlock::DisplayMath(tex_render_model::DisplayMathBlock {
                        raw_source: event.raw_source.clone(),
                        normalized_text: event.normalized_text.clone(),
                        structure: parse_display_math_structure(&event.raw_source),
                        source: envelope.meta.source.clone(),
                    }));
                }
                RenderEvent::BibliographyItem(event) => {
                    let item = BibliographyItemIr {
                        key: event.key.clone(),
                        label: self
                            .aux
                            .citation_label(&event.key, CitationStyleHint::Numeric)
                            .map(|label| label.text)
                            .or_else(|| event.label_hint.clone()),
                        content: event.text.clone(),
                        source: envelope.meta.source.clone(),
                    };
                    if let Some((items, _)) = &mut self.bibliography_items {
                        items.push(item);
                    } else {
                        self.flush_paragraph();
                        self.push_block(IrBlock::Bibliography(BibliographyBlock {
                            items: vec![item],
                            source: envelope.meta.source.clone(),
                        }));
                    }
                }
                RenderEvent::ParagraphBreak(_) if self.active_footnote.is_some() => {
                    self.push_inline(
                        InlineNode::LineBreak {
                            source: envelope.meta.source.clone(),
                        },
                        envelope,
                    );
                }
                RenderEvent::ParagraphBreak(_) if self.list_item.is_none() => {
                    self.flush_paragraph();
                }
                RenderEvent::ParagraphBreak(_) => {}
                RenderEvent::RawFallback(event) => {
                    self.flush_paragraph();
                    if matches!(
                        event.environment.as_deref(),
                        Some(
                            "array"
                                | "tabular"
                                | "tabular*"
                                | "tabularx"
                                | "longtable"
                                | "tabu"
                                | "longtabu",
                        )
                    ) {
                        let visible = event
                            .normalized_visible_text
                            .clone()
                            .unwrap_or_else(|| event.source_excerpt.clone());
                        let split_nested_table_cell_lines = |text: &str| {
                            let mut result = String::with_capacity(text.len());
                            let mut wrapper_stack = Vec::new();
                            let mut index = 0usize;
                            while index < text.len() {
                                let ch = text[index..].chars().next().expect("table cell char");
                                if ch.is_ascii_alphabetic() {
                                    let identifier_start = index;
                                    index += ch.len_utf8();
                                    while index < text.len()
                                        && text[index..].chars().next().is_some_and(|next| {
                                            next.is_ascii_alphanumeric() || next == '_'
                                        })
                                    {
                                        index += text[index..]
                                            .chars()
                                            .next()
                                            .expect("table cell identifier char")
                                            .len_utf8();
                                    }
                                    let identifier = &text[identifier_start..index];
                                    result.push_str(identifier);
                                    if text.as_bytes().get(index).copied() == Some(b'(') {
                                        result.push('(');
                                        wrapper_stack.push(matches!(
                                            identifier,
                                            "array"
                                                | "matrix"
                                                | "cases"
                                                | "subarray"
                                                | "aligned"
                                                | "split"
                                                | "gathered"
                                                | "multlined"
                                                | "alignedat"
                                                | "substack"
                                                | "bordermatrix"
                                        ));
                                        index += 1;
                                    }
                                    continue;
                                }
                                match ch {
                                    '(' => wrapper_stack.push(false),
                                    ')' => {
                                        wrapper_stack.pop();
                                    }
                                    ';' if wrapper_stack.last().copied().unwrap_or(false) => {
                                        result.push('\n');
                                        index += ch.len_utf8();
                                        while index < text.len()
                                            && text[index..]
                                                .chars()
                                                .next()
                                                .is_some_and(char::is_whitespace)
                                        {
                                            index += text[index..]
                                                .chars()
                                                .next()
                                                .expect("table cell whitespace")
                                                .len_utf8();
                                        }
                                        continue;
                                    }
                                    _ => {}
                                }
                                result.push(ch);
                                index += ch.len_utf8();
                            }
                            result
                        };
                        let mut serialized_rows = Vec::new();
                        let mut row_start = 0usize;
                        let mut parenthesis_depth = 0usize;
                        let mut bracket_depth = 0usize;
                        let mut brace_depth = 0usize;
                        for (index, ch) in visible.char_indices() {
                            match ch {
                                '(' => parenthesis_depth += 1,
                                ')' => parenthesis_depth = parenthesis_depth.saturating_sub(1),
                                '[' => bracket_depth += 1,
                                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                                '{' => brace_depth += 1,
                                '}' => brace_depth = brace_depth.saturating_sub(1),
                                ';' if parenthesis_depth == 0
                                    && bracket_depth == 0
                                    && brace_depth == 0 =>
                                {
                                    serialized_rows.push(&visible[row_start..index]);
                                    row_start = index + ch.len_utf8();
                                }
                                _ => {}
                            }
                        }
                        serialized_rows.push(&visible[row_start..]);
                        let mut rows = serialized_rows
                            .into_iter()
                            .filter_map(|row| {
                                let cells = row
                                    .split(" | ")
                                    .map(str::trim)
                                    .filter(|cell| !cell.is_empty())
                                    .map(|text| TableCell {
                                        text: split_nested_table_cell_lines(text),
                                        column_span: None,
                                        row_span: None,
                                        alignment: None,
                                        rule_before_count: 0,
                                        rule_after_count: 0,
                                        cell_prefix: None,
                                        cell_suffix: None,
                                    })
                                    .collect::<Vec<_>>();
                                (!cells.is_empty()).then_some(TableRow {
                                    rule_above: false,
                                    partial_rules_above: Vec::new(),
                                    cells,
                                    rule_below: false,
                                    partial_rules_below: Vec::new(),
                                })
                            })
                            .collect::<Vec<_>>();
                        let mut caption = None;
                        let mut caption_source = None;
                        if caption.is_none()
                            && event.environment.as_deref() == Some("longtable")
                            && event.source_excerpt.contains(r"\caption")
                            && rows.len() > 1
                            && rows.first().is_some_and(|row| row.cells.len() == 1)
                        {
                            caption = rows
                                .first()
                                .and_then(|row| row.cells.first())
                                .map(|cell| cell.text.clone());
                            caption_source = Some(envelope.meta.source.clone());
                            rows.remove(0);
                        }
                        for rule in &event.table_rules {
                            if rows.is_empty() {
                                break;
                            }
                            match rule.position {
                                TableRulePosition::Above => {
                                    if let Some(row) = rows.get_mut(rule.row_index) {
                                        if let Some(span) = rule.column_span {
                                            row.partial_rules_above.push(span);
                                        } else {
                                            row.rule_above = true;
                                        }
                                    } else if let Some(row) = rows.last_mut() {
                                        if let Some(span) = rule.column_span {
                                            row.partial_rules_below.push(span);
                                        } else {
                                            row.rule_below = true;
                                        }
                                    }
                                }
                                TableRulePosition::Below => {
                                    if let Some(row) = rows.get_mut(rule.row_index) {
                                        if let Some(span) = rule.column_span {
                                            row.partial_rules_below.push(span);
                                        } else {
                                            row.rule_below = true;
                                        }
                                    } else if let Some(row) = rows.last_mut() {
                                        if let Some(span) = rule.column_span {
                                            row.partial_rules_below.push(span);
                                        } else {
                                            row.rule_below = true;
                                        }
                                    }
                                }
                            }
                        }
                        for cell_span in &event.table_cell_spans {
                            if let Some(row) = rows.get_mut(cell_span.row_index) {
                                if let Some(cell) = row.cells.get_mut(cell_span.column_index) {
                                    if cell_span.column_span > 1 {
                                        cell.column_span = Some(cell_span.column_span);
                                    }
                                    if let Some(row_span) = cell_span.row_span
                                        && row_span > 1
                                    {
                                        cell.row_span = Some(row_span);
                                    }
                                    if let Some(alignment) = cell_span.alignment {
                                        cell.alignment = Some(alignment);
                                    }
                                    cell.rule_before_count =
                                        cell.rule_before_count.max(cell_span.rule_before_count);
                                    cell.rule_after_count =
                                        cell.rule_after_count.max(cell_span.rule_after_count);
                                    if let Some(prefix) = &cell_span.cell_prefix {
                                        cell.cell_prefix = Some(prefix.clone());
                                    }
                                    if let Some(suffix) = &cell_span.cell_suffix {
                                        cell.cell_suffix = Some(suffix.clone());
                                    }
                                }
                            }
                        }
                        let table = TableBlock {
                            environment: event
                                .environment
                                .clone()
                                .unwrap_or_else(|| "tabular".to_string()),
                            width_spec: event.table_width_spec.clone(),
                            columns: event.table_columns.clone(),
                            rows,
                            caption,
                            caption_source,
                            source: envelope.meta.source.clone(),
                        };
                        self.push_block(IrBlock::Table(table));
                    } else {
                        self.push_block(IrBlock::RawFallback(
                            tex_render_model::RawFallbackIr::from_event(
                                event,
                                envelope.meta.source.clone(),
                            ),
                        ));
                    }
                }
                RenderEvent::GraphicRef(event) => {
                    self.flush_paragraph();
                    let option_requests_full_width =
                        event.options.as_deref().is_some_and(|options| {
                            options.split(',').any(|part| {
                                part.split_once('=').is_some_and(|(key, value)| {
                                    key.trim() == "width" && value.contains("\\textwidth")
                                })
                            })
                        });
                    let full_width = option_requests_full_width && self.float_stack.is_empty();
                    let graphic = GraphicBlock {
                        path: event.path.clone(),
                        options: event.options.clone(),
                        page_selection: event.page_selection.clone(),
                        asset_format: event.asset_format,
                        asset_hash: event.asset_hash.clone(),
                        asset_dimensions: event.asset_dimensions,
                        caption: None,
                        caption_source: None,
                        source: envelope.meta.source.clone(),
                    };
                    self.push_block(if full_width {
                        IrBlock::FullWidthGraphic(graphic)
                    } else {
                        IrBlock::Graphic(graphic)
                    });
                }
                RenderEvent::IncludePdf(event) => {
                    self.flush_paragraph();
                    self.push_block(IrBlock::IncludedPdfPage(GraphicBlock {
                        path: event.path.clone(),
                        options: event.options.clone(),
                        page_selection: event.page_selection.clone(),
                        asset_format: event.asset_format,
                        asset_hash: event.asset_hash.clone(),
                        asset_dimensions: event.asset_dimensions,
                        caption: None,
                        caption_source: None,
                        source: envelope.meta.source.clone(),
                    }));
                }
                RenderEvent::Caption(event) => {
                    if self.layout_container_stack.is_empty()
                        && let Some(active) = self.float_stack.last_mut()
                    {
                        active.captions.push(ActiveFloatCaption {
                            text: event.text.clone(),
                            source: envelope.meta.source.clone(),
                            after_child_count: active.children.len(),
                        });
                        continue;
                    }
                    let target_blocks =
                        if let Some(container) = self.layout_container_stack.last_mut() {
                            &mut container.children
                        } else {
                            &mut self.blocks
                        };
                    if let Some(IrBlock::Graphic(block) | IrBlock::FullWidthGraphic(block)) =
                        target_blocks.last_mut()
                        && block.caption.is_none()
                    {
                        block.caption = Some(event.text.clone());
                        block.caption_source = Some(envelope.meta.source.clone());
                    } else {
                        self.flush_paragraph();
                        self.push_block(IrBlock::Paragraph(ParagraphBlock {
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
        self.finish_active_footnote(None);
        self.flush_paragraph();
        if let Some((mut content, source)) = self.abstract_content.take() {
            trim_trailing_spaces(&mut content);
            self.push_block(IrBlock::Abstract(AbstractBlock { content, source }));
        }
        if let Some((name, mut content, source)) = self.environment_content.take() {
            trim_trailing_spaces(&mut content);
            self.push_block(IrBlock::Environment(EnvironmentBlock {
                name,
                content,
                source,
            }));
        }
        if let Some((items, source)) = self.bibliography_items.take() {
            self.push_block(IrBlock::Bibliography(BibliographyBlock { items, source }));
        }
        self.flush_list_item();
        if let Some((kind, items, source)) = self.list.take() {
            self.push_block(IrBlock::List(ListBlock {
                kind,
                items,
                source,
            }));
        }
        while let Some(container) = self.layout_container_stack.pop() {
            self.push_block(IrBlock::LayoutContainer(container));
        }
        while let Some(active) = self.float_stack.pop() {
            self.push_block(active.into_block());
        }
        let mut document = DocumentIr::with_document_class_layout_and_labels(
            self.blocks,
            self.document_class,
            self.layout,
            self.labels,
        );
        document.footnotes = self.footnotes;
        document
    }

    fn push_block(&mut self, block: IrBlock) {
        if let Some(container) = self.layout_container_stack.last_mut() {
            container.children.push(block);
        } else if let Some(active) = self.float_stack.last_mut() {
            active.children.push(block);
        } else {
            self.blocks.push(block);
        }
    }

    fn push_inline(&mut self, node: InlineNode, envelope: &RenderEventEnvelope) {
        if let Some(footnote) = &mut self.active_footnote {
            if matches!(node, InlineNode::Space { .. })
                && (footnote.content.is_empty()
                    || matches!(footnote.content.last(), Some(InlineNode::Space { .. })))
            {
                return;
            }
            footnote.content.push(node);
            return;
        }
        if let Some((content, _)) = &mut self.abstract_content {
            if matches!(node, InlineNode::Space { .. })
                && (content.is_empty()
                    || matches!(
                        content.last(),
                        Some(InlineNode::Space { .. } | InlineNode::LineBreak { .. })
                    ))
            {
                return;
            }
            content.push(node);
            return;
        }
        if let Some((content, _, _)) = &mut self.list_item {
            if matches!(node, InlineNode::Space { .. })
                && (content.is_empty()
                    || matches!(
                        content.last(),
                        Some(InlineNode::Space { .. } | InlineNode::LineBreak { .. })
                    ))
            {
                return;
            }
            content.push(node);
            return;
        }
        if let Some((_, content, _)) = &mut self.environment_content {
            if matches!(node, InlineNode::Space { .. })
                && (content.is_empty()
                    || matches!(
                        content.last(),
                        Some(InlineNode::Space { .. } | InlineNode::LineBreak { .. })
                    ))
            {
                return;
            }
            content.push(node);
            return;
        }
        if matches!(node, InlineNode::Space { .. })
            && (self.paragraph.is_empty()
                || matches!(
                    self.paragraph.last(),
                    Some(InlineNode::Space { .. } | InlineNode::LineBreak { .. })
                ))
        {
            return;
        }
        if self.paragraph_source.is_none() {
            self.paragraph_source = Some(envelope.meta.source.clone());
        }
        self.paragraph.push(node);
    }

    fn resolve_footnote_marker(
        &mut self,
        note_id: FootnoteId,
        explicit_marker: Option<&String>,
    ) -> String {
        if let Some(marker) = self.footnote_markers.get(&note_id) {
            return marker.clone();
        }
        let marker = explicit_marker.cloned().unwrap_or_else(|| {
            let marker = self.next_footnote_number.to_string();
            self.next_footnote_number += 1;
            marker
        });
        self.footnote_markers.insert(note_id, marker.clone());
        marker
    }

    fn finish_active_footnote(&mut self, expected_note_id: Option<FootnoteId>) {
        let Some(mut footnote) = self.active_footnote.take() else {
            return;
        };
        if expected_note_id.is_some_and(|note_id| note_id != footnote.note_id) {
            self.active_footnote = Some(footnote);
            return;
        }
        trim_trailing_spaces(&mut footnote.content);
        self.footnotes.push(FootnoteIr {
            note_id: footnote.note_id,
            marker: footnote.marker,
            command: footnote.command,
            content: footnote.content,
            source: footnote.source,
        });
    }

    fn flush_list_item(&mut self) {
        let Some((mut content, source, marker_hint)) = self.list_item.take() else {
            return;
        };
        trim_trailing_spaces(&mut content);
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
        let mut content = std::mem::take(&mut self.paragraph);
        trim_trailing_spaces(&mut content);
        if content.is_empty() {
            return;
        }
        self.push_block(IrBlock::Paragraph(ParagraphBlock { content, source }));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tex_render_model::{
        BeginBlockEvent, BeginFootnoteEvent, BibliographyBlock, BibliographyItemEvent, BlockKind,
        CaptionEvent, CitationLabel, CitationLabelForm, CitationStyleHint, DocumentClassEvent,
        DocumentLayoutIntent, EndBlockEvent, EndFootnoteEvent, FloatKind, FloatPlacement,
        FlushTitleBlockEvent, FootnoteCommandKind, GraphicAssetDimensions, GraphicRefEvent,
        HeadingEvent, InlineCitationEvent, InlineLinkEvent, InlineNode, InlineReferenceEvent,
        IrBlock, LabelDefinitionEvent, LabelTargetView, MathSourceEvent, MetadataField,
        PageBreakEvent, PageBreakKind, ParagraphBreakEvent, ParagraphBreakReason, RawFallbackEvent,
        RenderEvent, RenderEventEnvelope, RenderEventStream, SetDocumentMetadataEvent,
        SourceProvenance, SpaceEvent, SpaceKind, TextEvent,
    };

    use super::build_document_ir;

    struct Labels {
        labels: BTreeMap<String, String>,
        targets: BTreeMap<String, String>,
    }

    impl tex_render_model::AuxView for Labels {
        fn citation_label(&self, key: &str, _style: CitationStyleHint) -> Option<CitationLabel> {
            self.labels.get(key).map(|text| CitationLabel {
                text: text.clone(),
                form: CitationLabelForm::Numeric,
            })
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

    struct FormattedLabels {
        labels: BTreeMap<String, (String, CitationLabelForm)>,
    }

    impl tex_render_model::AuxView for FormattedLabels {
        fn citation_label(&self, key: &str, _style: CitationStyleHint) -> Option<CitationLabel> {
            self.labels.get(key).map(|(text, form)| CitationLabel {
                text: text.clone(),
                form: *form,
            })
        }

        fn bibliography_record(
            &self,
            _key: &str,
        ) -> Option<tex_render_model::BibliographyRecordView> {
            None
        }

        fn label_target(&self, _key: &str) -> Option<LabelTargetView> {
            None
        }
    }

    #[test]
    fn preserves_document_class_layout_intent() {
        let source = SourceProvenance::file("main.tex", 0, 43);
        let stream = RenderEventStream::new(
            Some("document-class".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::DocumentClass(DocumentClassEvent {
                    name: "article".to_string(),
                    options: vec!["10pt".to_string(), "twocolumn".to_string()],
                }),
                source.clone(),
            )],
        );

        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
        );
        let document_class = ir.document_class.expect("document class");

        assert_eq!(document_class.name, "article");
        assert_eq!(
            document_class.options,
            vec!["10pt".to_string(), "twocolumn".to_string()]
        );
        assert_eq!(document_class.source, source);
    }

    #[test]
    fn footnote_boundaries_separate_note_content_from_the_outer_paragraph() {
        let source = SourceProvenance::file("main.tex", 0, 32);
        let stream = RenderEventStream::new(
            Some("footnote".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::Text(TextEvent {
                        text: "Before".to_string(),
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::BeginFootnote(BeginFootnoteEvent {
                        note_id: 2,
                        marker: None,
                        command: FootnoteCommandKind::Footnote,
                        draw_reference: true,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::Text(TextEvent {
                        text: "Note".to_string(),
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    4,
                    RenderEvent::ParagraphBreak(ParagraphBreakEvent {
                        reason: ParagraphBreakReason::ParCommand,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    5,
                    RenderEvent::Text(TextEvent {
                        text: "continued".to_string(),
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    6,
                    RenderEvent::EndFootnote(EndFootnoteEvent { note_id: 2 }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    7,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    8,
                    RenderEvent::Text(TextEvent {
                        text: "after.".to_string(),
                    }),
                    source,
                ),
            ],
        );

        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
        );
        let paragraph = ir
            .blocks
            .iter()
            .find_map(|block| match block {
                IrBlock::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .expect("outer paragraph");

        assert!(paragraph.content.iter().any(|node| matches!(
            node,
            InlineNode::FootnoteAnchor(anchor)
                if anchor.note_id == 2 && anchor.marker == "1" && anchor.draw_reference
        )));
        assert!(
            !paragraph
                .content
                .iter()
                .any(|node| matches!(node, InlineNode::Text { text, .. } if text == "Note"))
        );
        assert_eq!(ir.footnotes.len(), 1);
        assert_eq!(ir.footnotes[0].marker, "1");
        assert!(matches!(
            ir.footnotes[0].content.as_slice(),
            [
                InlineNode::Text { text, .. },
                InlineNode::LineBreak { .. },
                InlineNode::Text { text: continued, .. }
            ] if text == "Note" && continued == "continued"
        ));
        assert_eq!(ir.extracted_text(), "Before1 after.\n1 Note\ncontinued");
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
                push(RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                    field: MetadataField::AuthorNote,
                    value: "Corresponding author".to_string(),
                })),
                push(RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                    field: MetadataField::Affiliation,
                    value: "Analytical Engine Institute".to_string(),
                })),
                push(RenderEvent::SetDocumentMetadata(SetDocumentMetadataEvent {
                    field: MetadataField::Correspondence,
                    value: "ada@example.test".to_string(),
                })),
                push(RenderEvent::FlushTitleBlock(FlushTitleBlockEvent)),
                push(RenderEvent::BeginBlock(BeginBlockEvent {
                    block: BlockKind::Abstract,
                })),
                push(RenderEvent::Text(TextEvent {
                    text: "Short abstract.".to_string(),
                })),
                push(RenderEvent::EndBlock(EndBlockEvent {
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
                push(RenderEvent::EndBlock(EndBlockEvent {
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
        assert!(text.contains("Corresponding author"));
        assert!(text.contains("Analytical Engine Institute"));
        assert!(text.contains("ada@example.test"));
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
                                if span.start_utf8 == 6 && span.end_utf8 == 7
                        )
                }))
        );
        assert!(matches!(
            title_block.author_sources.first().map(|source| &source.primary),
            Some(tex_render_model::ProvenanceSpan::File(span))
                if span.start_utf8 == 2 && span.end_utf8 == 3
        ));
        assert_eq!(title_block.author_notes, ["Corresponding author"]);
        assert!(matches!(
            title_block
                .author_note_sources
                .first()
                .map(|source| &source.primary),
            Some(tex_render_model::ProvenanceSpan::File(span))
                if span.start_utf8 == 3 && span.end_utf8 == 4
        ));
        assert!(matches!(
            title_block
                .affiliation_sources
                .first()
                .map(|source| &source.primary),
            Some(tex_render_model::ProvenanceSpan::File(span))
                if span.start_utf8 == 4 && span.end_utf8 == 5
        ));
        assert!(matches!(
            title_block
                .correspondence_sources
                .first()
                .map(|source| &source.primary),
            Some(tex_render_model::ProvenanceSpan::File(span))
                if span.start_utf8 == 5 && span.end_utf8 == 6
        ));
        assert!(matches!(
            &title_block.source.primary,
            tex_render_model::ProvenanceSpan::File(span)
                if span.start_utf8 == 6 && span.end_utf8 == 7
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
    fn resolved_numeric_citations_compact_three_or_more_consecutive_labels() {
        let stream = RenderEventStream::new(
            Some("citation-range".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::InlineCitation(InlineCitationEvent {
                    keys: vec![
                        "alpha".to_string(),
                        "beta".to_string(),
                        "gamma".to_string(),
                        "delta".to_string(),
                    ],
                    command: "cite".to_string(),
                    style_hint: CitationStyleHint::Numeric,
                }),
                SourceProvenance::file("main.tex", 0, 30),
            )],
        );
        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::from([
                    ("alpha".to_string(), "1".to_string()),
                    ("beta".to_string(), "2".to_string()),
                    ("gamma".to_string(), "3".to_string()),
                    ("delta".to_string(), "5".to_string()),
                ]),
                targets: BTreeMap::new(),
            },
        );

        assert_eq!(ir.extracted_text(), "[1-3,5]");
    }

    #[test]
    fn bibliography_items_use_aux_resolved_numeric_labels() {
        let source = SourceProvenance::file("main.bbl", 0, 96);
        let stream = RenderEventStream::new(
            Some("numeric-bibliography-label".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::BeginBlock(BeginBlockEvent {
                        block: BlockKind::Bibliography,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::BibliographyItem(BibliographyItemEvent {
                        key: "bengio".to_string(),
                        label_hint: Some("Bengio(2009)Bengio".to_string()),
                        text: "Yoshua Bengio. Learning deep architectures.".to_string(),
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::EndBlock(EndBlockEvent {
                        block: BlockKind::Bibliography,
                    }),
                    source,
                ),
            ],
        );
        let ir = build_document_ir(
            &stream,
            &Labels {
                labels: BTreeMap::from([("bengio".to_string(), "1".to_string())]),
                targets: BTreeMap::new(),
            },
        );

        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Bibliography(BibliographyBlock { items, .. })]
                if items[0].label.as_deref() == Some("1")
                    && items[0].content.starts_with("Yoshua Bengio")
        ));
    }

    #[test]
    fn resolved_author_year_citations_preserve_requested_delimiters() {
        let source = SourceProvenance::file("main.tex", 0, 30);
        let labels = FormattedLabels {
            labels: BTreeMap::from([
                (
                    "parenthetical".to_string(),
                    ("Bengio, 2009".to_string(), CitationLabelForm::Parenthetical),
                ),
                (
                    "textual".to_string(),
                    (
                        "Goodfellow et al. (2014)".to_string(),
                        CitationLabelForm::Textual,
                    ),
                ),
            ]),
        };
        let stream = RenderEventStream::new(
            Some("author-year-citations".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::InlineCitation(InlineCitationEvent {
                        keys: vec!["parenthetical".to_string()],
                        command: "citep".to_string(),
                        style_hint: CitationStyleHint::Parenthetical,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::InlineCitation(InlineCitationEvent {
                        keys: vec!["textual".to_string()],
                        command: "citet".to_string(),
                        style_hint: CitationStyleHint::Textual,
                    }),
                    source,
                ),
            ],
        );

        let ir = build_document_ir(&stream, &labels);

        assert_eq!(
            ir.extracted_text(),
            "(Bengio, 2009) Goodfellow et al. (2014)"
        );
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
    fn paragraph_normalizes_interword_spaces() {
        let source = SourceProvenance::file("main.tex", 0, 11);
        let stream = RenderEventStream::new(
            Some("trailing-space".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::Text(TextEvent {
                        text: "Hello".to_string(),
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    4,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    5,
                    RenderEvent::Text(TextEvent {
                        text: "world".to_string(),
                    }),
                    source.clone(),
                ),
                RenderEventEnvelope::new(
                    6,
                    RenderEvent::Space(SpaceEvent {
                        kind: SpaceKind::Interword,
                    }),
                    source,
                ),
            ],
        );

        let ir = build_document_ir(&stream, &());

        assert_eq!(ir.extracted_text(), "Hello world");
        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Paragraph(paragraph)]
                if matches!(
                    paragraph.content.as_slice(),
                    [InlineNode::Text { .. }, InlineNode::Space { .. }, InlineNode::Text { .. }]
                )
        ));
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
                    table_rules: Vec::new(),
                    table_cell_spans: Vec::new(),
                    table_columns: Vec::new(),
                    table_width_spec: None,
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
                        page_selection: None,
                        asset_format: None,
                        asset_hash: None,
                        asset_dimensions: Some(GraphicAssetDimensions {
                            width_px: 640,
                            height_px: 320,
                            density: None,
                            natural_width_pt_milli: None,
                            natural_height_pt_milli: None,
                        }),
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
                    && block.asset_dimensions == Some(GraphicAssetDimensions { width_px: 640, height_px: 320, density: None, natural_width_pt_milli: None, natural_height_pt_milli: None })
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
                        page_selection: None,
                        asset_format: None,
                        asset_hash: None,
                        asset_dimensions: None,
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
                    RenderEvent::EndBlock(EndBlockEvent {
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
                    RenderEvent::EndBlock(EndBlockEvent {
                        block: BlockKind::Table,
                    }),
                    SourceProvenance::file("main.tex", 118, 130),
                ),
            ],
        );
        let ir = build_document_ir(&stream, &());

        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Float(figure), IrBlock::Float(table)]
                if figure.kind == FloatKind::Figure
                    && figure.caption.as_deref() == Some("Plot caption.")
                    && matches!(figure.children.as_slice(), [IrBlock::Graphic(graphic)] if graphic.path == "figures/plot.pdf")
                    && table.kind == FloatKind::Table
                    && table.caption.as_deref() == Some("Table caption.")
                    && table.children.is_empty()
        ));
        assert_eq!(ir.extracted_text(), "Plot caption.\nTable caption.");
    }

    #[test]
    fn figure_float_preserves_multiple_graphics_in_source_order() {
        let source = SourceProvenance::file("main.tex", 0, 120);
        let mut next_id = 1;
        let mut event = |event| {
            let envelope = RenderEventEnvelope::new(next_id, event, source.clone());
            next_id += 1;
            envelope
        };
        let graphic = |path: &str| {
            RenderEvent::GraphicRef(GraphicRefEvent {
                path: path.to_string(),
                options: Some("width=2cm".to_string()),
                page_selection: None,
                asset_format: None,
                asset_hash: None,
                asset_dimensions: None,
            })
        };
        let stream = RenderEventStream::new(
            Some("multi-graphic-float".to_string()),
            vec![
                event(RenderEvent::BeginBlock(BeginBlockEvent {
                    block: BlockKind::Figure,
                })),
                event(graphic("figures/a.pdf")),
                event(graphic("figures/b.pdf")),
                event(RenderEvent::Caption(CaptionEvent {
                    text: "Panels.".to_string(),
                })),
                event(RenderEvent::EndBlock(EndBlockEvent {
                    block: BlockKind::Figure,
                })),
            ],
        );

        let ir = build_document_ir(&stream, &());
        let [IrBlock::Float(float)] = ir.blocks.as_slice() else {
            panic!("expected one float: {:?}", ir.blocks);
        };
        let paths = float
            .children
            .iter()
            .filter_map(|block| match block {
                IrBlock::Graphic(graphic) => Some(graphic.path.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(float.kind, FloatKind::Figure);
        assert_eq!(float.placement, FloatPlacement::Top);
        assert!(!float.full_width);
        assert_eq!(paths, ["figures/a.pdf", "figures/b.pdf"]);
        assert_eq!(float.caption.as_deref(), Some("Panels."));
    }

    #[test]
    fn detached_caption_does_not_overwrite_previous_graphic_caption() {
        let stream = RenderEventStream::new(
            Some("detached-caption".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::GraphicRef(GraphicRefEvent {
                        path: "figures/plot.pdf".to_string(),
                        options: None,
                        page_selection: None,
                        asset_format: None,
                        asset_hash: None,
                        asset_dimensions: None,
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
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::Caption(CaptionEvent {
                        text: "Algorithm caption.".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 53, 80),
                ),
            ],
        );
        let ir = build_document_ir(&stream, &());

        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Graphic(graphic), IrBlock::Paragraph(paragraph)]
                if graphic.caption.as_deref() == Some("Plot caption.")
                    && matches!(
                        paragraph.content.as_slice(),
                        [InlineNode::Text { text, .. }] if text == "Algorithm caption."
                    )
        ));
        assert_eq!(ir.extracted_text(), "Plot caption.\nAlgorithm caption.");
    }

    #[test]
    fn algorithm_environment_groups_caption_text_and_display_math() {
        let source = SourceProvenance::file("main.tex", 0, 100);
        let mut next_id = 1;
        let mut event = |event| {
            let envelope = RenderEventEnvelope::new(next_id, event, source.clone());
            next_id += 1;
            envelope
        };
        let stream = RenderEventStream::new(
            Some("algorithm".to_string()),
            vec![
                event(RenderEvent::BeginBlock(BeginBlockEvent {
                    block: BlockKind::Environment {
                        name: "algorithm".to_string(),
                    },
                })),
                event(RenderEvent::Caption(CaptionEvent {
                    text: "Algorithm caption.".to_string(),
                })),
                event(RenderEvent::BeginBlock(BeginBlockEvent {
                    block: BlockKind::Environment {
                        name: "algorithmic".to_string(),
                    },
                })),
                event(RenderEvent::Text(TextEvent {
                    text: "First step.".to_string(),
                })),
                event(RenderEvent::DisplayMath(MathSourceEvent {
                    raw_source: "x = y".to_string(),
                    normalized_text: None,
                })),
                event(RenderEvent::Text(TextEvent {
                    text: "Second step.".to_string(),
                })),
                event(RenderEvent::EndBlock(EndBlockEvent {
                    block: BlockKind::Environment {
                        name: "algorithmic".to_string(),
                    },
                })),
                event(RenderEvent::EndBlock(EndBlockEvent {
                    block: BlockKind::Environment {
                        name: "algorithm".to_string(),
                    },
                })),
            ],
        );
        let ir = build_document_ir(&stream, &());

        let [IrBlock::LayoutContainer(algorithm)] = ir.blocks.as_slice() else {
            panic!("expected one algorithm layout container: {:?}", ir.blocks);
        };
        assert_eq!(algorithm.name, "algorithm");
        assert!(matches!(
            algorithm.children.as_slice(),
            [
                IrBlock::Paragraph(caption),
                IrBlock::Environment(first),
                IrBlock::DisplayMath(math),
                IrBlock::Environment(second),
            ] if matches!(
                    caption.content.as_slice(),
                    [InlineNode::Text { text, .. }] if text == "Algorithm caption."
                )
                && first.name == "algorithmic"
                && matches!(
                    first.content.as_slice(),
                    [InlineNode::Text { text, .. }] if text == "First step."
                )
                && math.raw_source == "x = y"
                && second.name == "algorithmic"
                && matches!(
                    second.content.as_slice(),
                    [InlineNode::Text { text, .. }] if text == "Second step."
                )
        ));
    }

    #[test]
    fn preserves_full_width_float_intent_on_graphics_and_tables() {
        let stream = RenderEventStream::new(
            Some("full-width-floats".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::BeginBlock(BeginBlockEvent {
                        block: BlockKind::FullWidthFigure,
                    }),
                    SourceProvenance::file("main.tex", 0, 15),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::GraphicRef(GraphicRefEvent {
                        path: "figures/wide.pdf".to_string(),
                        options: Some("width=\\textwidth".to_string()),
                        page_selection: None,
                        asset_format: None,
                        asset_hash: None,
                        asset_dimensions: None,
                    }),
                    SourceProvenance::file("main.tex", 16, 64),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::Caption(CaptionEvent {
                        text: "Wide figure.".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 65, 87),
                ),
                RenderEventEnvelope::new(
                    4,
                    RenderEvent::EndBlock(EndBlockEvent {
                        block: BlockKind::FullWidthFigure,
                    }),
                    SourceProvenance::file("main.tex", 88, 100),
                ),
                RenderEventEnvelope::new(
                    5,
                    RenderEvent::BeginBlock(BeginBlockEvent {
                        block: BlockKind::FullWidthTable,
                    }),
                    SourceProvenance::file("main.tex", 101, 115),
                ),
                RenderEventEnvelope::new(
                    6,
                    RenderEvent::Caption(CaptionEvent {
                        text: "Wide table.".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 116, 137),
                ),
                RenderEventEnvelope::new(
                    7,
                    RenderEvent::EndBlock(EndBlockEvent {
                        block: BlockKind::FullWidthTable,
                    }),
                    SourceProvenance::file("main.tex", 138, 149),
                ),
            ],
        );

        let ir = build_document_ir(&stream, &());

        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Float(figure), IrBlock::Float(table)]
                if figure.kind == FloatKind::Figure
                    && figure.full_width
                    && figure.caption.as_deref() == Some("Wide figure.")
                    && matches!(figure.children.as_slice(), [IrBlock::Graphic(graphic)] if graphic.path == "figures/wide.pdf")
                    && table.kind == FloatKind::Table
                    && table.full_width
                    && table.caption.as_deref() == Some("Wide table.")
        ));
    }

    #[test]
    fn preserves_layout_intent_and_forced_page_breaks() {
        let layout = DocumentLayoutIntent {
            profile: Some("conference-preview".to_string()),
            text_width_pt_milli: Some(396_000),
            text_height_pt_milli: Some(648_000),
            column_count: Some(2),
            column_gap_pt_milli: Some(18_000),
            body_font_size_pt_milli: Some(10_000),
            line_height_pt_milli: Some(11_000),
            ..DocumentLayoutIntent::default()
        };
        let stream = RenderEventStream::new(
            Some("layout-and-page-break".to_string()),
            vec![
                RenderEventEnvelope::new(
                    1,
                    RenderEvent::SetDocumentLayout(layout.clone()),
                    SourceProvenance::file("style.sty", 0, 20),
                ),
                RenderEventEnvelope::new(
                    2,
                    RenderEvent::SetDocumentLayout(DocumentLayoutIntent {
                        text_font_family: Some("times".to_string()),
                        ..DocumentLayoutIntent::default()
                    }),
                    SourceProvenance::file("times.sty", 0, 18),
                ),
                RenderEventEnvelope::new(
                    3,
                    RenderEvent::Text(TextEvent {
                        text: "Before".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 0, 6),
                ),
                RenderEventEnvelope::new(
                    4,
                    RenderEvent::PageBreak(PageBreakEvent {
                        kind: PageBreakKind::NewPage,
                    }),
                    SourceProvenance::file("main.tex", 6, 14),
                ),
                RenderEventEnvelope::new(
                    5,
                    RenderEvent::Text(TextEvent {
                        text: "After".to_string(),
                    }),
                    SourceProvenance::file("main.tex", 14, 19),
                ),
            ],
        );

        let ir = build_document_ir(&stream, &());

        let mut expected_layout = layout;
        expected_layout.text_font_family = Some("times".to_string());
        assert_eq!(ir.layout, Some(expected_layout));
        assert!(matches!(
            ir.blocks.as_slice(),
            [IrBlock::Paragraph(before), IrBlock::PageBreak(page_break), IrBlock::Paragraph(after)]
                if before.content.len() == 1
                    && page_break.kind == PageBreakKind::NewPage
                    && after.content.len() == 1
        ));
    }
}
