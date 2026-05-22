use tex_layout::{DocumentLayout, LayoutOptions, PageLayout};
use tex_render_model::{DrawOp, FontFamilyRequest, FontSeries, FontShape, PageDisplayList};

pub const PAGE_TEXT_LEFT_PT: f32 = 72.0;
pub const PAGE_TEXT_TOP_PT: f32 = 72.0;
pub const PAGE_LINE_HEIGHT_PT: f32 = 14.0;
pub const PAGE_FONT_SIZE_PT: f32 = 12.0;

pub fn render_pdf(layout: &DocumentLayout) -> Vec<u8> {
    let mut objects = Vec::new();
    objects.push("1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string());
    objects.push(format!(
        "2 0 obj << /Type /Pages /Kids [{}] /Count {} >> endobj\n",
        layout
            .pages
            .iter()
            .enumerate()
            .map(|(index, _)| format!("{} 0 R", page_object_id(index)))
            .collect::<Vec<_>>()
            .join(" "),
        layout.pages.len()
    ));
    objects.push(
        "3 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_string(),
    );

    for (index, page) in layout.pages.iter().enumerate() {
        let content_id = content_object_id(index);
        let page_id = page_object_id(index);
        let stream = build_page_stream(page, layout.options.page_height_pt);
        objects.push(format!(
            "{content_id} 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ));
        objects.push(format!(
            "{page_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >> endobj\n",
            layout.options.page_width_pt,
            layout.options.page_height_pt
        ));
    }

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0usize];
    for object in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object.as_bytes());
    }

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );

    pdf
}

pub fn render_single_page_pdf(page: &PageLayout, options: &LayoutOptions) -> Vec<u8> {
    render_pdf(&DocumentLayout {
        pages: vec![page.clone()],
        options: options.clone(),
    })
}

pub fn render_display_list_pdf(pages: &[PageDisplayList]) -> Vec<u8> {
    let mut objects = Vec::new();
    let mut destination_entries = Vec::new();
    let content_object_id = |index: usize| 15 + index * 2;
    let page_object_id = |index: usize| 16 + index * 2;
    let font_resources = (1..=12)
        .map(|slot| format!("/F{slot} {} 0 R", slot + 2))
        .collect::<Vec<_>>()
        .join(" ");
    for (index, page) in pages.iter().enumerate() {
        for op in &page.ops {
            if let DrawOp::NamedDestination(destination) = op {
                destination_entries.push(format!(
                    "({}) [{} 0 R /XYZ {} {} null]",
                    escape_pdf_text(&destination.name),
                    page_object_id(index),
                    destination.point.x,
                    page.height_pt - destination.point.y
                ));
            }
        }
    }
    let names = if destination_entries.is_empty() {
        String::new()
    } else {
        format!(
            " /Names << /Dests << /Names [{}] >> >>",
            destination_entries.join(" ")
        )
    };
    objects.push(format!(
        "1 0 obj << /Type /Catalog /Pages 2 0 R{} >> endobj\n",
        names
    ));
    objects.push(format!(
        "2 0 obj << /Type /Pages /Kids [{}] /Count {} >> endobj\n",
        pages
            .iter()
            .enumerate()
            .map(|(index, _)| format!("{} 0 R", page_object_id(index)))
            .collect::<Vec<_>>()
            .join(" "),
        pages.len()
    ));
    for (object_id, base_font) in [
        (3, "Times-Roman"),
        (4, "Times-Bold"),
        (5, "Times-Italic"),
        (6, "Times-BoldItalic"),
        (7, "Helvetica"),
        (8, "Helvetica-Bold"),
        (9, "Helvetica-Oblique"),
        (10, "Helvetica-BoldOblique"),
        (11, "Courier"),
        (12, "Courier-Bold"),
        (13, "Courier-Oblique"),
        (14, "Courier-BoldOblique"),
    ] {
        objects.push(format!(
            "{object_id} 0 obj << /Type /Font /Subtype /Type1 /BaseFont /{base_font} >> endobj\n"
        ));
    }

    let mut annotation_objects = Vec::new();
    let mut next_annotation_object_id = 15 + pages.len() * 2;
    for (index, page) in pages.iter().enumerate() {
        let content_id = content_object_id(index);
        let page_id = page_object_id(index);
        let mut stream = String::new();
        let mut annotation_refs = Vec::new();
        for op in &page.ops {
            match op {
                DrawOp::Save => {
                    stream.push_str("q ");
                }
                DrawOp::Restore => {
                    stream.push_str("Q ");
                }
                DrawOp::ClipRect(rect) => {
                    stream.push_str(&format!(
                        "{} {} {} {} re W n ",
                        rect.x,
                        page.height_pt - rect.y - rect.height,
                        rect.width,
                        rect.height
                    ));
                }
                DrawOp::TextRun(run) => {
                    let font_resource = match (&run.font.family, run.font.series, run.font.shape) {
                        (
                            FontFamilyRequest::Serif | FontFamilyRequest::Math,
                            FontSeries::Regular,
                            FontShape::Upright,
                        ) => "F1",
                        (
                            FontFamilyRequest::Serif | FontFamilyRequest::Math,
                            FontSeries::Bold,
                            FontShape::Upright,
                        ) => "F2",
                        (
                            FontFamilyRequest::Serif | FontFamilyRequest::Math,
                            FontSeries::Regular,
                            FontShape::Italic,
                        ) => "F3",
                        (
                            FontFamilyRequest::Serif | FontFamilyRequest::Math,
                            FontSeries::Bold,
                            FontShape::Italic,
                        ) => "F4",
                        (
                            FontFamilyRequest::Sans | FontFamilyRequest::Named(_),
                            FontSeries::Regular,
                            FontShape::Upright,
                        ) => "F5",
                        (
                            FontFamilyRequest::Sans | FontFamilyRequest::Named(_),
                            FontSeries::Bold,
                            FontShape::Upright,
                        ) => "F6",
                        (
                            FontFamilyRequest::Sans | FontFamilyRequest::Named(_),
                            FontSeries::Regular,
                            FontShape::Italic,
                        ) => "F7",
                        (
                            FontFamilyRequest::Sans | FontFamilyRequest::Named(_),
                            FontSeries::Bold,
                            FontShape::Italic,
                        ) => "F8",
                        (FontFamilyRequest::Mono, FontSeries::Regular, FontShape::Upright) => "F9",
                        (FontFamilyRequest::Mono, FontSeries::Bold, FontShape::Upright) => "F10",
                        (FontFamilyRequest::Mono, FontSeries::Regular, FontShape::Italic) => "F11",
                        (FontFamilyRequest::Mono, FontSeries::Bold, FontShape::Italic) => "F12",
                    };
                    stream.push_str("BT ");
                    stream.push_str(&format!("/{font_resource} {} Tf ", run.size_pt));
                    stream.push_str(&format!(
                        "1 0 0 1 {} {} Tm ",
                        run.origin.x,
                        page.height_pt - run.origin.y
                    ));
                    stream.push('(');
                    stream.push_str(&escape_pdf_text(&run.text));
                    stream.push_str(") Tj ET ");
                }
                DrawOp::Rule(rect) => {
                    stream.push_str(&format!(
                        "q {} {} {} {} re f Q ",
                        rect.x,
                        page.height_pt - rect.y - rect.height,
                        rect.width,
                        rect.height
                    ));
                }
                DrawOp::Image(image) => {
                    stream.push_str(&format!(
                        "q 0.92 g {} {} {} {} re f 0 G {} {} {} {} re S Q ",
                        image.rect.x,
                        page.height_pt - image.rect.y - image.rect.height,
                        image.rect.width,
                        image.rect.height,
                        image.rect.x,
                        page.height_pt - image.rect.y - image.rect.height,
                        image.rect.width,
                        image.rect.height
                    ));
                    stream.push_str("BT /F1 8 Tf ");
                    stream.push_str(&format!(
                        "1 0 0 1 {} {} Tm ",
                        image.rect.x + 4.0,
                        page.height_pt - image.rect.y - image.rect.height / 2.0
                    ));
                    stream.push('(');
                    stream.push_str(&escape_pdf_text(&format!("[image: {}]", image.asset_ref)));
                    stream.push_str(") Tj ET ");
                }
                DrawOp::LinkAnnotation(link) => {
                    let annotation_id = next_annotation_object_id;
                    next_annotation_object_id += 1;
                    annotation_refs.push(format!("{annotation_id} 0 R"));
                    annotation_objects.push(format!(
                        "{annotation_id} 0 obj << /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] /A << /S /URI /URI ({}) >> >> endobj\n",
                        link.rect.x,
                        page.height_pt - link.rect.y - link.rect.height,
                        link.rect.x + link.rect.width,
                        page.height_pt - link.rect.y,
                        escape_pdf_text(&link.target)
                    ));
                }
                _ => {}
            }
        }
        objects.push(format!(
            "{content_id} 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ));
        let annotations = if annotation_refs.is_empty() {
            String::new()
        } else {
            format!(" /Annots [{}]", annotation_refs.join(" "))
        };
        objects.push(format!(
            "{page_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {} >> >> /Contents {content_id} 0 R{} >> endobj\n",
            page.width_pt,
            page.height_pt,
            font_resources,
            annotations
        ));
    }
    objects.extend(annotation_objects);

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0usize];
    for object in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object.as_bytes());
    }

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );

    pdf
}

pub fn render_display_list_svg(page: &PageDisplayList) -> String {
    let mut body = String::new();
    let mut clip_index = 0usize;
    let mut svg_group_stack = Vec::new();
    let role_name = |role| match role {
        tex_render_model::SourceSpanRole::Invocation => "invocation",
        tex_render_model::SourceSpanRole::Argument => "argument",
        tex_render_model::SourceSpanRole::ArgumentContent => "argument_content",
        tex_render_model::SourceSpanRole::Definition => "definition",
        tex_render_model::SourceSpanRole::EmitSite => "emit_site",
        tex_render_model::SourceSpanRole::CitationKey => "citation_key",
        tex_render_model::SourceSpanRole::ReferenceKey => "reference_key",
        tex_render_model::SourceSpanRole::MetadataDefinition => "metadata_definition",
        tex_render_model::SourceSpanRole::SyntheticNumbering => "synthetic_numbering",
        tex_render_model::SourceSpanRole::FallbackSource => "fallback_source",
    };
    let span_descriptor = |span: &tex_render_model::ProvenanceSpan| match span {
        tex_render_model::ProvenanceSpan::File(span) => format!(
            "file:{}:{}:{}",
            span.path.as_str(),
            span.start_utf8,
            span.end_utf8
        ),
        tex_render_model::ProvenanceSpan::Generated(span) => {
            format!("generated:{}:{}", span.stable_id, span.description)
        }
    };
    let source_attrs_for = |source: &tex_render_model::SourceProvenance| {
        let mut source_attrs = match &source.primary {
            tex_render_model::ProvenanceSpan::File(span) => format!(
                " data-source-kind=\"file\" data-source-path=\"{}\" data-source-start-utf8=\"{}\" data-source-end-utf8=\"{}\"",
                escape_xml_text(span.path.as_str()),
                span.start_utf8,
                span.end_utf8
            ),
            tex_render_model::ProvenanceSpan::Generated(span) => format!(
                " data-source-kind=\"generated\" data-source-generated-id=\"{}\" data-source-description=\"{}\"",
                escape_xml_text(&span.stable_id),
                escape_xml_text(&span.description)
            ),
        };
        if !source.related.is_empty() {
            let roles = source
                .related
                .iter()
                .map(|related| role_name(related.role))
                .collect::<Vec<_>>()
                .join(",");
            let spans = source
                .related
                .iter()
                .map(|related| match &related.span {
                    tex_render_model::ProvenanceSpan::File(span) => format!(
                        "{}:file:{}:{}:{}",
                        role_name(related.role),
                        span.path.as_str(),
                        span.start_utf8,
                        span.end_utf8
                    ),
                    tex_render_model::ProvenanceSpan::Generated(span) => format!(
                        "{}:generated:{}:{}",
                        role_name(related.role),
                        span.stable_id,
                        span.description
                    ),
                })
                .collect::<Vec<_>>()
                .join(";");
            source_attrs.push_str(&format!(
                " data-source-related-count=\"{}\" data-source-related-roles=\"{}\" data-source-related-spans=\"{}\"",
                source.related.len(),
                escape_xml_text(&roles),
                escape_xml_text(&spans)
            ));
        }
        if !source.expansion_stack.is_empty() {
            let commands = source
                .expansion_stack
                .iter()
                .filter_map(|frame| frame.command_name.as_deref())
                .collect::<Vec<_>>()
                .join(",");
            let calls = source
                .expansion_stack
                .iter()
                .map(|frame| span_descriptor(&frame.call_span))
                .collect::<Vec<_>>()
                .join(";");
            let definitions = source
                .expansion_stack
                .iter()
                .filter_map(|frame| frame.definition_span.as_ref())
                .map(span_descriptor)
                .collect::<Vec<_>>()
                .join(";");
            source_attrs.push_str(&format!(
                " data-source-expansion-depth=\"{}\" data-source-expansion-truncated=\"{}\" data-source-expansion-commands=\"{}\" data-source-expansion-calls=\"{}\" data-source-expansion-definitions=\"{}\"",
                source.expansion_stack.len(),
                source.expansion_stack_truncated,
                escape_xml_text(&commands),
                escape_xml_text(&calls),
                escape_xml_text(&definitions)
            ));
        }
        source_attrs
    };
    for op in &page.ops {
        match op {
            DrawOp::Save => {
                body.push_str("<g>");
                svg_group_stack.push(true);
            }
            DrawOp::Restore => {
                while let Some(is_save_group) = svg_group_stack.pop() {
                    body.push_str("</g>");
                    if is_save_group {
                        break;
                    }
                }
            }
            DrawOp::ClipRect(rect) => {
                let clip_id = format!("clip-{clip_index}");
                clip_index += 1;
                body.push_str(&format!(
                    "<defs><clipPath id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#{})\" data-clip-rect=\"{},{},{},{}\">",
                    clip_id,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    clip_id,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                ));
                svg_group_stack.push(false);
            }
            DrawOp::TextRun(run) => {
                let family = match &run.font.family {
                    tex_render_model::FontFamilyRequest::Serif => "serif",
                    tex_render_model::FontFamilyRequest::Sans => "sans-serif",
                    tex_render_model::FontFamilyRequest::Mono => "monospace",
                    tex_render_model::FontFamilyRequest::Math => "serif",
                    tex_render_model::FontFamilyRequest::Named(name) => name.as_str(),
                };
                let weight = match run.font.series {
                    tex_render_model::FontSeries::Regular => "400",
                    tex_render_model::FontSeries::Bold => "700",
                };
                let style = match run.font.shape {
                    tex_render_model::FontShape::Upright => "normal",
                    tex_render_model::FontShape::Italic => "italic",
                };
                let mut source_attrs = String::new();
                match &run.source.primary {
                    tex_render_model::ProvenanceSpan::File(span) => {
                        source_attrs.push_str(&format!(
                            " data-source-kind=\"file\" data-source-path=\"{}\" data-source-start-utf8=\"{}\" data-source-end-utf8=\"{}\"",
                            escape_xml_text(span.path.as_str()),
                            span.start_utf8,
                            span.end_utf8
                        ));
                    }
                    tex_render_model::ProvenanceSpan::Generated(span) => {
                        source_attrs.push_str(&format!(
                            " data-source-kind=\"generated\" data-source-generated-id=\"{}\" data-source-description=\"{}\"",
                            escape_xml_text(&span.stable_id),
                            escape_xml_text(&span.description)
                        ));
                    }
                }
                if let Some(clusters) = &run.clusters {
                    let encoded_clusters = clusters
                        .iter()
                        .map(|cluster| {
                            format!(
                                "{}:{}:{}:{}",
                                cluster.text_start_utf8,
                                cluster.text_end_utf8,
                                cluster.glyph_start,
                                cluster.glyph_end
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(";");
                    source_attrs.push_str(&format!(
                        " data-text-clusters=\"{}\"",
                        escape_xml_text(&encoded_clusters)
                    ));
                }
                if !run.source.related.is_empty() {
                    let roles = run
                        .source
                        .related
                        .iter()
                        .map(|related| role_name(related.role))
                        .collect::<Vec<_>>()
                        .join(",");
                    let spans = run
                        .source
                        .related
                        .iter()
                        .map(|related| match &related.span {
                            tex_render_model::ProvenanceSpan::File(span) => format!(
                                "{}:file:{}:{}:{}",
                                role_name(related.role),
                                span.path.as_str(),
                                span.start_utf8,
                                span.end_utf8
                            ),
                            tex_render_model::ProvenanceSpan::Generated(span) => format!(
                                "{}:generated:{}:{}",
                                role_name(related.role),
                                span.stable_id,
                                span.description
                            ),
                        })
                        .collect::<Vec<_>>()
                        .join(";");
                    source_attrs.push_str(&format!(
                        " data-source-related-count=\"{}\" data-source-related-roles=\"{}\" data-source-related-spans=\"{}\"",
                        run.source.related.len(),
                        escape_xml_text(&roles),
                        escape_xml_text(&spans)
                    ));
                }
                if !run.source.expansion_stack.is_empty() {
                    let commands = run
                        .source
                        .expansion_stack
                        .iter()
                        .filter_map(|frame| frame.command_name.as_deref())
                        .collect::<Vec<_>>()
                        .join(",");
                    let calls = run
                        .source
                        .expansion_stack
                        .iter()
                        .map(|frame| span_descriptor(&frame.call_span))
                        .collect::<Vec<_>>()
                        .join(";");
                    let definitions = run
                        .source
                        .expansion_stack
                        .iter()
                        .filter_map(|frame| frame.definition_span.as_ref())
                        .map(span_descriptor)
                        .collect::<Vec<_>>()
                        .join(";");
                    source_attrs.push_str(&format!(
                        " data-source-expansion-depth=\"{}\" data-source-expansion-truncated=\"{}\" data-source-expansion-commands=\"{}\" data-source-expansion-calls=\"{}\" data-source-expansion-definitions=\"{}\"",
                        run.source.expansion_stack.len(),
                        run.source.expansion_stack_truncated,
                        escape_xml_text(&commands),
                        escape_xml_text(&calls),
                        escape_xml_text(&definitions)
                    ));
                }
                body.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\"{}>{}</text>",
                    run.origin.x,
                    run.origin.y,
                    escape_xml_text(family),
                    run.size_pt,
                    weight,
                    style,
                    source_attrs,
                    escape_xml_text(&run.text)
                ));
            }
            DrawOp::Rule(rect) => {
                body.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"black\"/>",
                    rect.x, rect.y, rect.width, rect.height
                ));
            }
            DrawOp::Image(image) => {
                let asset_format_attr = image
                    .asset_format
                    .map(|format| format!(" data-image-asset-format=\"{}\"", format.as_str()))
                    .unwrap_or_default();
                let asset_hash_attr = image
                    .asset_hash
                    .as_deref()
                    .map(|hash| format!(" data-image-asset-hash=\"{}\"", escape_xml_text(hash)))
                    .unwrap_or_default();
                body.push_str(&format!(
                    "<g data-image-asset-ref=\"{}\"{}{}{}><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#e5e7eb\" stroke=\"#6b7280\" stroke-width=\"1\"/><text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"9\" fill=\"#374151\">{}</text></g>",
                    escape_xml_text(&image.asset_ref),
                    asset_format_attr,
                    asset_hash_attr,
                    source_attrs_for(&image.source),
                    image.rect.x,
                    image.rect.y,
                    image.rect.width,
                    image.rect.height,
                    image.rect.x + 4.0,
                    image.rect.y + image.rect.height / 2.0,
                    escape_xml_text(&format!("[image: {}]", image.asset_ref))
                ));
            }
            DrawOp::LinkAnnotation(link) => {
                body.push_str(&format!(
                    "<a href=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#1d4ed8\" stroke-width=\"1\" data-link-target=\"{}\"{}/></a>",
                    escape_xml_text(&link.target),
                    link.rect.x,
                    link.rect.y,
                    link.rect.width,
                    link.rect.height,
                    escape_xml_text(&link.target),
                    source_attrs_for(&link.source)
                ));
            }
            DrawOp::NamedDestination(destination) => {
                body.push_str(&format!(
                    "<g data-destination-name=\"{}\" data-destination-x=\"{}\" data-destination-y=\"{}\"{}><circle cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"#dc2626\"/></g>",
                    escape_xml_text(&destination.name),
                    destination.point.x,
                    destination.point.y,
                    source_attrs_for(&destination.source),
                    destination.point.x,
                    destination.point.y
                ));
            }
        }
    }
    while svg_group_stack.pop().is_some() {
        body.push_str("</g>");
    }

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" data-page-id=\"{}\" data-content-hash=\"{}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{}</svg>",
        page.width_pt,
        page.height_pt,
        page.width_pt,
        page.height_pt,
        escape_xml_text(&page.page_id),
        escape_xml_text(&page.content_hash),
        body
    )
}

pub fn render_page_svg(page: &PageLayout, options: &LayoutOptions) -> String {
    let mut body = String::new();
    for (index, line) in page.lines.iter().enumerate() {
        let y = PAGE_TEXT_TOP_PT + PAGE_LINE_HEIGHT_PT * index as f32;
        body.push_str(&format!(
            "<text x=\"{}\" y=\"{y}\" font-family=\"Iowan Old Style, Palatino, serif\" font-size=\"{}\">{}</text>",
            PAGE_TEXT_LEFT_PT,
            PAGE_FONT_SIZE_PT,
            escape_xml_text(line)
        ));
    }

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{}</svg>",
        options.page_width_pt,
        options.page_height_pt,
        options.page_width_pt,
        options.page_height_pt,
        body
    )
}

fn build_page_stream(page: &PageLayout, page_height_pt: f32) -> String {
    let mut stream = String::new();
    stream.push_str(&format!(
        "BT /F1 {} Tf {} TL ",
        PAGE_FONT_SIZE_PT, PAGE_LINE_HEIGHT_PT
    ));
    stream.push_str(&format!(
        "{} {} Td ",
        PAGE_TEXT_LEFT_PT,
        page_height_pt - PAGE_TEXT_TOP_PT
    ));
    for (index, line) in page.lines.iter().enumerate() {
        if index > 0 {
            stream.push_str("T* ");
        }
        stream.push('(');
        stream.push_str(&escape_pdf_text(line));
        stream.push_str(") Tj ");
    }
    stream.push_str("ET");
    stream
}

fn escape_pdf_text(text: &str) -> String {
    let mut escaped = String::new();
    for ch in text.chars() {
        match ch {
            '(' | ')' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\r' | '\n' => escaped.push(' '),
            other if other.is_control() => escaped.push('?'),
            other => escaped.push(other),
        }
    }
    escaped
}

fn escape_xml_text(text: &str) -> String {
    let mut escaped = String::new();
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '\r' | '\n' => escaped.push(' '),
            other if other.is_control() => escaped.push('?'),
            other => escaped.push(other),
        }
    }
    escaped
}

fn content_object_id(index: usize) -> usize {
    4 + index * 2
}

fn page_object_id(index: usize) -> usize {
    5 + index * 2
}

#[cfg(test)]
mod tests {
    use tex_layout::{LayoutOptions, layout_text};
    use tex_render_model::{
        Destination, DrawOp, ExpansionFrame, FontFamilyRequest, FontRequest, FontRole, FontSeries,
        FontShape, GraphicAssetFormat, LinkAnnotation, PageDisplayList, Point, PositionedImage,
        PositionedTextRun, ProvenanceSpan, Rect, SourceProvenance, SourceSpan, SourceSpanRole,
        TextCluster,
    };

    use super::{
        render_display_list_pdf, render_display_list_svg, render_page_svg, render_pdf,
        render_single_page_pdf,
    };

    #[test]
    fn emits_valid_pdf_header_and_trailer() {
        let layout = layout_text("hello pdf", LayoutOptions::default());
        let pdf = render_pdf(&layout);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("trailer << /Size "));
        assert!(text.contains("/Type /Page"));
    }

    #[test]
    fn renders_multiple_pages() {
        let layout = layout_text(
            "a\nb\nc\nd\ne",
            LayoutOptions {
                chars_per_line: 10,
                lines_per_page: 2,
                ..LayoutOptions::default()
            },
        );
        let pdf = render_pdf(&layout);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.contains("/Count 3"));
    }

    #[test]
    fn escapes_pdf_sensitive_characters_in_stream() {
        let layout = layout_text(r#"hello (pdf) \ world"#, LayoutOptions::default());
        let pdf = render_pdf(&layout);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.contains(r#"(hello \(pdf\) \\ world) Tj"#));
    }

    #[test]
    fn emits_one_text_draw_per_line() {
        let layout = layout_text(
            "alpha\nbeta\ngamma",
            LayoutOptions {
                chars_per_line: 20,
                lines_per_page: 10,
                ..LayoutOptions::default()
            },
        );
        let pdf = render_pdf(&layout);
        let text = String::from_utf8_lossy(&pdf);

        assert_eq!(text.matches(" Tj ").count(), 3);
        assert_eq!(text.matches("T* ").count(), 2);
    }

    #[test]
    fn renders_single_page_pdf() {
        let layout = layout_text("alpha\nbeta", LayoutOptions::default());
        let pdf = render_single_page_pdf(&layout.pages[0], &layout.options);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 1"));
    }

    #[test]
    fn renders_page_svg() {
        let layout = layout_text("alpha & beta", LayoutOptions::default());
        let svg = render_page_svg(&layout.pages[0], &layout.options);

        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("&amp;"));
        assert!(svg.contains("<text "));
    }

    #[test]
    fn renders_display_list_text_runs_as_pdf_text() {
        let pdf = render_display_list_pdf(&[PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::TextRun(PositionedTextRun {
                origin: Point { x: 72.0, y: 72.0 },
                text: "Hello display list".to_string(),
                font: FontRequest {
                    family: FontFamilyRequest::Serif,
                    series: FontSeries::Regular,
                    shape: FontShape::Upright,
                    size_pt: 11.0,
                    role: FontRole::Body,
                },
                size_pt: 11.0,
                approximate_advance_pt: 99.0,
                glyphs: None,
                clusters: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        }]);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 1"));
        assert!(text.contains("/F1 11 Tf 1 0 0 1 72 720 Tm (Hello display list) Tj"));
    }

    #[test]
    fn display_list_pdf_escapes_text_runs() {
        let pdf = render_display_list_pdf(&[PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::TextRun(PositionedTextRun {
                origin: Point { x: 72.0, y: 72.0 },
                text: r#"hello (pdf) \ display"#.to_string(),
                font: FontRequest {
                    family: FontFamilyRequest::Serif,
                    series: FontSeries::Regular,
                    shape: FontShape::Upright,
                    size_pt: 11.0,
                    role: FontRole::Body,
                },
                size_pt: 11.0,
                approximate_advance_pt: 99.0,
                glyphs: None,
                clusters: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        }]);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.contains(r#"(hello \(pdf\) \\ display) Tj"#));
    }

    #[test]
    fn display_list_pdf_uses_text_run_font_style_resources() {
        let pdf = render_display_list_pdf(&[PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![
                DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 72.0, y: 72.0 },
                    text: "Bold".to_string(),
                    font: FontRequest {
                        family: FontFamilyRequest::Serif,
                        series: FontSeries::Bold,
                        shape: FontShape::Upright,
                        size_pt: 14.0,
                        role: FontRole::Heading,
                    },
                    size_pt: 14.0,
                    approximate_advance_pt: 28.0,
                    glyphs: None,
                    clusters: None,
                    source: SourceProvenance::file("main.tex", 0, 4),
                }),
                DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 72.0, y: 90.0 },
                    text: "Italic".to_string(),
                    font: FontRequest {
                        family: FontFamilyRequest::Serif,
                        series: FontSeries::Regular,
                        shape: FontShape::Italic,
                        size_pt: 10.0,
                        role: FontRole::Body,
                    },
                    size_pt: 10.0,
                    approximate_advance_pt: 30.0,
                    glyphs: None,
                    clusters: None,
                    source: SourceProvenance::file("main.tex", 5, 11),
                }),
            ],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        }]);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.contains("/F2 14 Tf 1 0 0 1 72 720 Tm (Bold) Tj"));
        assert!(text.contains("/F3 10 Tf 1 0 0 1 72 702 Tm (Italic) Tj"));
        assert!(text.contains("/BaseFont /Times-Bold"));
        assert!(text.contains("/BaseFont /Times-Italic"));
    }

    #[test]
    fn display_list_pdf_uses_text_run_font_family_resources() {
        let pdf = render_display_list_pdf(&[PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::TextRun(PositionedTextRun {
                origin: Point { x: 72.0, y: 72.0 },
                text: "Code".to_string(),
                font: FontRequest {
                    family: FontFamilyRequest::Mono,
                    series: FontSeries::Bold,
                    shape: FontShape::Upright,
                    size_pt: 9.0,
                    role: FontRole::Mono,
                },
                size_pt: 9.0,
                approximate_advance_pt: 18.0,
                glyphs: None,
                clusters: None,
                source: SourceProvenance::file("main.tex", 0, 4),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        }]);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.contains("/F10 9 Tf 1 0 0 1 72 720 Tm (Code) Tj"));
        assert!(text.contains("/BaseFont /Courier-Bold"));
    }

    #[test]
    fn display_list_svg_exposes_text_clusters() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::TextRun(PositionedTextRun {
                origin: Point { x: 72.0, y: 72.0 },
                text: "aé".to_string(),
                font: FontRequest {
                    family: FontFamilyRequest::Serif,
                    series: FontSeries::Regular,
                    shape: FontShape::Upright,
                    size_pt: 11.0,
                    role: FontRole::Body,
                },
                size_pt: 11.0,
                approximate_advance_pt: 11.0,
                glyphs: None,
                clusters: Some(vec![TextCluster {
                    text_start_utf8: 0,
                    text_end_utf8: 3,
                    glyph_start: 0,
                    glyph_end: 2,
                }]),
                source: SourceProvenance::file("main.tex", 0, 3),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg(&page);

        assert!(svg.contains("data-text-clusters=\"0:3:0:2\""));
    }

    #[test]
    fn display_list_svg_exposes_page_identity_metadata() {
        let page = PageDisplayList {
            page_id: "page-1&\"".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: Vec::new(),
            source_spans: Vec::new(),
            content_hash: "hash<&\"".to_string(),
        };
        let svg = render_display_list_svg(&page);

        assert!(svg.contains("data-page-id=\"page-1&amp;&quot;\""));
        assert!(svg.contains("data-content-hash=\"hash&lt;&amp;&quot;\""));
    }

    #[test]
    fn renders_display_list_rules_to_pdf_and_svg() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![
                DrawOp::Rule(Rect {
                    x: 72.0,
                    y: 90.0,
                    width: 144.0,
                    height: 2.0,
                }),
                DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 72.0, y: 72.0 },
                    text: "Rule & text".to_string(),
                    font: FontRequest {
                        family: FontFamilyRequest::Serif,
                        series: FontSeries::Bold,
                        shape: FontShape::Italic,
                        size_pt: 11.0,
                        role: FontRole::Body,
                    },
                    size_pt: 11.0,
                    approximate_advance_pt: 60.0,
                    glyphs: None,
                    clusters: None,
                    source: SourceProvenance::file("main.tex", 0, 10)
                        .with_related(
                            SourceSpanRole::MetadataDefinition,
                            ProvenanceSpan::File(SourceSpan {
                                path: "main.tex".into(),
                                start_utf8: 20,
                                end_utf8: 30,
                            }),
                        )
                        .with_expansion_frame(ExpansionFrame {
                            call_span: ProvenanceSpan::File(SourceSpan {
                                path: "main.tex".into(),
                                start_utf8: 40,
                                end_utf8: 50,
                            }),
                            definition_span: Some(ProvenanceSpan::File(SourceSpan {
                                path: "macros.tex".into(),
                                start_utf8: 3,
                                end_utf8: 13,
                            })),
                            command_name: Some("mysection".to_string()),
                        }),
                }),
            ],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("q 72 700 144 2 re f Q"));
        assert!(
            svg.contains("<rect x=\"72\" y=\"90\" width=\"144\" height=\"2\" fill=\"black\"/>")
        );
        assert!(svg.contains("font-weight=\"700\""));
        assert!(svg.contains("font-style=\"italic\""));
        assert!(svg.contains("data-source-kind=\"file\""));
        assert!(svg.contains("data-source-path=\"main.tex\""));
        assert!(svg.contains("data-source-start-utf8=\"0\""));
        assert!(svg.contains("data-source-end-utf8=\"10\""));
        assert!(svg.contains("data-source-related-count=\"1\""));
        assert!(svg.contains("data-source-related-roles=\"metadata_definition\""));
        assert!(
            svg.contains("data-source-related-spans=\"metadata_definition:file:main.tex:20:30\"")
        );
        assert!(svg.contains("data-source-expansion-depth=\"1\""));
        assert!(svg.contains("data-source-expansion-truncated=\"false\""));
        assert!(svg.contains("data-source-expansion-commands=\"mysection\""));
        assert!(svg.contains("data-source-expansion-calls=\"file:main.tex:40:50\""));
        assert!(svg.contains("data-source-expansion-definitions=\"file:macros.tex:3:13\""));
        assert!(svg.contains("Rule &amp; text"));
    }

    #[test]
    fn renders_display_list_clip_scope_to_pdf_and_svg() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![
                DrawOp::Save,
                DrawOp::ClipRect(Rect {
                    x: 72.0,
                    y: 80.0,
                    width: 100.0,
                    height: 40.0,
                }),
                DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 72.0, y: 96.0 },
                    text: "Clipped text".to_string(),
                    font: FontRequest {
                        family: FontFamilyRequest::Serif,
                        series: FontSeries::Regular,
                        shape: FontShape::Upright,
                        size_pt: 10.0,
                        role: FontRole::Body,
                    },
                    size_pt: 10.0,
                    approximate_advance_pt: 60.0,
                    glyphs: None,
                    clusters: None,
                    source: SourceProvenance::file("main.tex", 0, 12),
                }),
                DrawOp::Restore,
            ],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("q 72 672 100 40 re W n BT"));
        assert!(pdf_text.contains("(Clipped text) Tj ET Q"));
        assert!(
            svg.contains(
                "<clipPath id=\"clip-0\"><rect x=\"72\" y=\"80\" width=\"100\" height=\"40\"/></clipPath>"
            )
        );
        assert!(svg.contains("<g clip-path=\"url(#clip-0)\" data-clip-rect=\"72,80,100,40\">"));
        assert!(svg.contains("Clipped text"));
        assert!(svg.contains("</g></g>"));
    }

    #[test]
    fn renders_display_list_images_to_pdf_and_svg_debug_placeholders() {
        let source = SourceProvenance::file("main.tex", 0, 10).with_related(
            SourceSpanRole::Argument,
            ProvenanceSpan::File(SourceSpan {
                path: "main.tex".into(),
                start_utf8: 30,
                end_utf8: 48,
            }),
        );
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 78.0,
                    width: 144.0,
                    height: 72.0,
                },
                asset_ref: "figures/a(b)&c.pdf".to_string(),
                asset_format: Some(GraphicAssetFormat::Pdf),
                asset_hash: Some("blake3:asset-hash".to_string()),
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("q 0.92 g 72 642 144 72 re f 0 G 72 642 144 72 re S Q"));
        assert!(
            pdf_text
                .contains(r#"BT /F1 8 Tf 1 0 0 1 76 678 Tm ([image: figures/a\(b\)&c.pdf]) Tj ET"#)
        );
        assert!(svg.contains("data-image-asset-ref=\"figures/a(b)&amp;c.pdf\""));
        assert!(svg.contains("data-image-asset-format=\"pdf\""));
        assert!(svg.contains("data-image-asset-hash=\"blake3:asset-hash\""));
        assert!(
            svg.contains("<rect x=\"72\" y=\"78\" width=\"144\" height=\"72\" fill=\"#e5e7eb\"")
        );
        assert!(svg.contains("data-source-kind=\"file\""));
        assert!(svg.contains("data-source-path=\"main.tex\""));
        assert!(svg.contains("data-source-start-utf8=\"0\""));
        assert!(svg.contains("data-source-end-utf8=\"10\""));
        assert!(svg.contains("data-source-related-count=\"1\""));
        assert!(svg.contains("data-source-related-roles=\"argument\""));
        assert!(svg.contains("data-source-related-spans=\"argument:file:main.tex:30:48\""));
        assert!(svg.contains("[image: figures/a(b)&amp;c.pdf]"));
    }

    #[test]
    fn renders_display_list_link_annotations_to_pdf_and_svg() {
        let source = SourceProvenance::file("main.tex", 0, 10)
            .with_related(
                SourceSpanRole::Argument,
                tex_render_model::ProvenanceSpan::File(SourceSpan {
                    path: "macros.tex".into(),
                    start_utf8: 20,
                    end_utf8: 45,
                }),
            )
            .with_expansion_frame(ExpansionFrame {
                call_span: ProvenanceSpan::File(SourceSpan {
                    path: "main.tex".into(),
                    start_utf8: 60,
                    end_utf8: 88,
                }),
                definition_span: Some(ProvenanceSpan::File(SourceSpan {
                    path: "macros.tex".into(),
                    start_utf8: 0,
                    end_utf8: 20,
                })),
                command_name: Some("defaulttargetlink".to_string()),
            });
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::LinkAnnotation(LinkAnnotation {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 80.0,
                    height: 12.0,
                },
                target: "https://example.com/a?b=1&c=2".to_string(),
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("/Annots [17 0 R]"));
        assert!(pdf_text.contains("/Subtype /Link"));
        assert!(pdf_text.contains("/Rect [72 708 152 720]"));
        assert!(pdf_text.contains("/URI (https://example.com/a?b=1&c=2)"));
        assert!(svg.contains("<a href=\"https://example.com/a?b=1&amp;c=2\">"));
        assert!(svg.contains("data-link-target=\"https://example.com/a?b=1&amp;c=2\""));
        assert!(svg.contains("data-source-kind=\"file\""));
        assert!(svg.contains("data-source-path=\"main.tex\""));
        assert!(svg.contains("data-source-start-utf8=\"0\""));
        assert!(svg.contains("data-source-end-utf8=\"10\""));
        assert!(svg.contains("data-source-related-count=\"1\""));
        assert!(svg.contains("data-source-related-roles=\"argument\""));
        assert!(svg.contains("data-source-related-spans=\"argument:file:macros.tex:20:45\""));
        assert!(svg.contains("data-source-expansion-depth=\"1\""));
        assert!(svg.contains("data-source-expansion-truncated=\"false\""));
        assert!(svg.contains("data-source-expansion-commands=\"defaulttargetlink\""));
        assert!(svg.contains("data-source-expansion-calls=\"file:main.tex:60:88\""));
        assert!(svg.contains("data-source-expansion-definitions=\"file:macros.tex:0:20\""));
    }

    #[test]
    fn renders_display_list_named_destinations_to_pdf_and_svg() {
        let source = SourceProvenance::file("main.tex", 0, 10).with_related(
            SourceSpanRole::Invocation,
            ProvenanceSpan::File(SourceSpan {
                path: "main.tex".into(),
                start_utf8: 50,
                end_utf8: 72,
            }),
        );
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::NamedDestination(Destination {
                name: "sec:intro&more".to_string(),
                point: Point { x: 72.0, y: 72.0 },
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("/Names << /Dests << /Names ["));
        assert!(pdf_text.contains("(sec:intro&more) [16 0 R /XYZ 72 720 null]"));
        assert!(svg.contains("data-destination-name=\"sec:intro&amp;more\""));
        assert!(svg.contains("data-destination-x=\"72\""));
        assert!(svg.contains("data-destination-y=\"72\""));
        assert!(svg.contains("data-source-kind=\"file\""));
        assert!(svg.contains("data-source-path=\"main.tex\""));
        assert!(svg.contains("data-source-start-utf8=\"0\""));
        assert!(svg.contains("data-source-end-utf8=\"10\""));
        assert!(svg.contains("data-source-related-count=\"1\""));
        assert!(svg.contains("data-source-related-roles=\"invocation\""));
        assert!(svg.contains("data-source-related-spans=\"invocation:file:main.tex:50:72\""));
        assert!(svg.contains("<circle cx=\"72\" cy=\"72\" r=\"3\" fill=\"#dc2626\"/>"));
    }
}
