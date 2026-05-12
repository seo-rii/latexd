use tex_layout::{DocumentLayout, LayoutOptions, PageLayout};
use tex_render_model::{DrawOp, PageDisplayList};

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
    objects.push("1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string());
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
    objects.push(
        "3 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_string(),
    );

    for (index, page) in pages.iter().enumerate() {
        let content_id = content_object_id(index);
        let page_id = page_object_id(index);
        let mut stream = String::new();
        for op in &page.ops {
            match op {
                DrawOp::TextRun(run) => {
                    stream.push_str("BT ");
                    stream.push_str(&format!("/F1 {} Tf ", run.size_pt));
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
                _ => {}
            }
        }
        objects.push(format!(
            "{content_id} 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ));
        objects.push(format!(
            "{page_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >> endobj\n",
            page.width_pt,
            page.height_pt
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

pub fn render_display_list_svg(page: &PageDisplayList) -> String {
    let mut body = String::new();
    for op in &page.ops {
        match op {
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
                if !run.source.related.is_empty() {
                    let role_name = |role| match role {
                        tex_render_model::SourceSpanRole::Invocation => "invocation",
                        tex_render_model::SourceSpanRole::Argument => "argument",
                        tex_render_model::SourceSpanRole::ArgumentContent => "argument_content",
                        tex_render_model::SourceSpanRole::Definition => "definition",
                        tex_render_model::SourceSpanRole::EmitSite => "emit_site",
                        tex_render_model::SourceSpanRole::CitationKey => "citation_key",
                        tex_render_model::SourceSpanRole::MetadataDefinition => {
                            "metadata_definition"
                        }
                        tex_render_model::SourceSpanRole::SyntheticNumbering => {
                            "synthetic_numbering"
                        }
                        tex_render_model::SourceSpanRole::FallbackSource => "fallback_source",
                    };
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
            _ => {}
        }
    }

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{}</svg>",
        page.width_pt, page.height_pt, page.width_pt, page.height_pt, body
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
        DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape, PageDisplayList,
        Point, PositionedTextRun, ProvenanceSpan, Rect, SourceProvenance, SourceSpan,
        SourceSpanRole,
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
                    source: SourceProvenance::file("main.tex", 0, 10).with_related(
                        SourceSpanRole::MetadataDefinition,
                        ProvenanceSpan::File(SourceSpan {
                            path: "main.tex".into(),
                            start_utf8: 20,
                            end_utf8: 30,
                        }),
                    ),
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
        assert!(svg.contains("Rule &amp; text"));
    }
}
