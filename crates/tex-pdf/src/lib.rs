use tex_layout::{DocumentLayout, LayoutOptions, PageLayout};
use tex_render_model::{
    DrawOp, FontFamilyRequest, FontSeries, FontShape, GraphicAssetFormat, ImageCrop,
    PageDisplayList, Point, PositionedImage, Rect,
};

pub const PAGE_TEXT_LEFT_PT: f32 = 72.0;
pub const PAGE_TEXT_TOP_PT: f32 = 72.0;
pub const PAGE_LINE_HEIGHT_PT: f32 = 14.0;
pub const PAGE_FONT_SIZE_PT: f32 = 12.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedImageAsset {
    pub bytes: Vec<u8>,
    pub format: GraphicAssetFormat,
}

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
    render_display_list_pdf_with_assets(pages, |_| None)
}

pub fn render_display_list_pdf_with_assets(
    pages: &[PageDisplayList],
    resolve_asset: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<u8> {
    render_display_list_pdf_with_converted_assets(pages, resolve_asset, |_, _| None)
}

pub fn render_display_list_pdf_with_converted_assets(
    pages: &[PageDisplayList],
    mut resolve_asset: impl FnMut(&str) -> Option<Vec<u8>>,
    mut convert_asset: impl FnMut(&PositionedImage, &[u8]) -> Option<ConvertedImageAsset>,
) -> Vec<u8> {
    let mut objects = Vec::<Vec<u8>>::new();
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
                destination_entries.push((
                    destination.name.clone(),
                    format!(
                        "({}) [{} 0 R /XYZ {} {} null]",
                        escape_pdf_text(&destination.name),
                        page_object_id(index),
                        destination.point.x,
                        page.height_pt - destination.point.y
                    ),
                ));
            }
        }
    }
    destination_entries.sort_by(|left, right| left.0.cmp(&right.0));
    let names = if destination_entries.is_empty() {
        String::new()
    } else {
        format!(
            " /Names << /Dests << /Names [{}] >> >>",
            destination_entries
                .iter()
                .map(|(_, entry)| entry.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    objects.push(
        format!(
            "1 0 obj << /Type /Catalog /Pages 2 0 R{} >> endobj\n",
            names
        )
        .into_bytes(),
    );
    objects.push(
        format!(
            "2 0 obj << /Type /Pages /Kids [{}] /Count {} >> endobj\n",
            pages
                .iter()
                .enumerate()
                .map(|(index, _)| format!("{} 0 R", page_object_id(index)))
                .collect::<Vec<_>>()
                .join(" "),
            pages.len()
        )
        .into_bytes(),
    );
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
        )
        .into_bytes());
    }

    let mut extra_objects = Vec::new();
    let mut next_extra_object_id = 15 + pages.len() * 2;
    for (index, page) in pages.iter().enumerate() {
        let content_id = content_object_id(index);
        let page_id = page_object_id(index);
        let mut stream = String::new();
        let mut annotation_refs = Vec::new();
        let mut image_resource_refs = Vec::new();
        let mut next_page_image_index = 1usize;
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
                    let placeholder_status = ImagePlaceholderStatus::from_image(image);
                    if placeholder_status == ImagePlaceholderStatus::Draft {
                        push_image_placeholder(
                            &mut stream,
                            page.height_pt,
                            image,
                            placeholder_status,
                        );
                    } else if let Some(bytes) = resolve_asset(&image.asset_ref) {
                        let mut rendered_svg_vector = false;
                        if image.asset_format == Some(GraphicAssetFormat::Svg)
                            && let Ok(svg_text) = std::str::from_utf8(&bytes)
                            && let Some(svg) = parse_simple_svg_asset(svg_text)
                            && (!svg.rects.is_empty() || !svg.lines.is_empty())
                        {
                            let (natural_width, natural_height) = image_natural_size_or_fallback(
                                image,
                                svg.natural_width_pt,
                                svg.natural_height_pt,
                            );
                            let dest_x = image.rect.x;
                            let dest_y = page.height_pt - image.rect.y - image.rect.height;
                            let draw = image_draw_placement(
                                Rect {
                                    x: dest_x,
                                    y: dest_y,
                                    width: image.rect.width,
                                    height: image.rect.height,
                                },
                                image.crop,
                                natural_width,
                                natural_height,
                                false,
                            );
                            let rotated =
                                push_pdf_image_rotation(&mut stream, page.height_pt, image);
                            if draw.clip_to_dest {
                                stream.push_str(&format!(
                                    "q {} {} {} {} re W n q ",
                                    dest_x, dest_y, image.rect.width, image.rect.height
                                ));
                            } else {
                                stream.push_str("q ");
                            }
                            let scale_x = draw.rect.width / natural_width;
                            let scale_y = draw.rect.height / natural_height;
                            if scale_x.is_finite()
                                && scale_y.is_finite()
                                && scale_x > 0.0
                                && scale_y > 0.0
                            {
                                for rect in svg.rects {
                                    let rect_x =
                                        draw.rect.x + rect.x_ratio * natural_width * scale_x;
                                    let rect_y = draw.rect.y
                                        + (1.0 - rect.y_ratio - rect.height_ratio)
                                            * natural_height
                                            * scale_y;
                                    let rect_width = rect.width_ratio * natural_width * scale_x;
                                    let rect_height = rect.height_ratio * natural_height * scale_y;
                                    if rect_width.is_finite()
                                        && rect_height.is_finite()
                                        && rect_width > 0.0
                                        && rect_height > 0.0
                                    {
                                        if let Some(fill_rgb) = rect.fill_rgb {
                                            stream.push_str(&format!(
                                                "{} {} {} rg {} {} {} {} re f ",
                                                fill_rgb.0,
                                                fill_rgb.1,
                                                fill_rgb.2,
                                                rect_x,
                                                rect_y,
                                                rect_width,
                                                rect_height
                                            ));
                                        }
                                        if let Some(stroke_rgb) = rect.stroke_rgb {
                                            let stroke_width =
                                                rect.stroke_width_ratio * natural_width * scale_x;
                                            if stroke_width.is_finite() && stroke_width > 0.0 {
                                                stream.push_str(&format!(
                                                    "{} {} {} RG {} w {} {} {} {} re S ",
                                                    stroke_rgb.0,
                                                    stroke_rgb.1,
                                                    stroke_rgb.2,
                                                    stroke_width,
                                                    rect_x,
                                                    rect_y,
                                                    rect_width,
                                                    rect_height
                                                ));
                                            }
                                        }
                                    }
                                }
                                for line in svg.lines {
                                    let x1 = draw.rect.x + line.x1_ratio * natural_width * scale_x;
                                    let y1 = draw.rect.y
                                        + (1.0 - line.y1_ratio) * natural_height * scale_y;
                                    let x2 = draw.rect.x + line.x2_ratio * natural_width * scale_x;
                                    let y2 = draw.rect.y
                                        + (1.0 - line.y2_ratio) * natural_height * scale_y;
                                    let stroke_width =
                                        line.stroke_width_ratio * natural_width * scale_x;
                                    if x1.is_finite()
                                        && y1.is_finite()
                                        && x2.is_finite()
                                        && y2.is_finite()
                                        && stroke_width.is_finite()
                                        && stroke_width > 0.0
                                    {
                                        stream.push_str(&format!(
                                            "{} {} {} RG {} w {} {} m {} {} l S ",
                                            line.stroke_rgb.0,
                                            line.stroke_rgb.1,
                                            line.stroke_rgb.2,
                                            stroke_width,
                                            x1,
                                            y1,
                                            x2,
                                            y2
                                        ));
                                    }
                                }
                                rendered_svg_vector = true;
                            }
                            if draw.clip_to_dest {
                                stream.push_str("Q Q ");
                            } else {
                                stream.push_str("Q ");
                            }
                            if rotated {
                                stream.push_str("Q ");
                            }
                        }
                        if rendered_svg_vector {
                            continue;
                        }
                        let decoded = decode_pdf_image(&bytes).or_else(|| {
                            convert_asset(image, &bytes).and_then(|converted| {
                                match converted.format {
                                    GraphicAssetFormat::Png | GraphicAssetFormat::Jpeg => {
                                        decode_pdf_image(&converted.bytes)
                                    }
                                    _ => None,
                                }
                            })
                        });
                        if let Some(decoded) = decoded {
                            let (natural_width, natural_height) = image_natural_size_or_fallback(
                                image,
                                decoded.width as f32,
                                decoded.height as f32,
                            );
                            let object_id = next_extra_object_id;
                            next_extra_object_id += 1;
                            let resource_name = format!("Im{next_page_image_index}");
                            next_page_image_index += 1;
                            image_resource_refs.push(format!("/{resource_name} {object_id} 0 R"));
                            extra_objects.push(build_image_xobject(object_id, &decoded));
                            let dest_x = image.rect.x;
                            let dest_y = page.height_pt - image.rect.y - image.rect.height;
                            let draw = image_draw_placement(
                                Rect {
                                    x: dest_x,
                                    y: dest_y,
                                    width: image.rect.width,
                                    height: image.rect.height,
                                },
                                image.crop,
                                natural_width,
                                natural_height,
                                false,
                            );
                            let rotated =
                                push_pdf_image_rotation(&mut stream, page.height_pt, image);
                            if draw.clip_to_dest {
                                stream.push_str(&format!(
                                    "q {} {} {} {} re W n q {} 0 0 {} {} {} cm /{} Do Q Q ",
                                    dest_x,
                                    dest_y,
                                    image.rect.width,
                                    image.rect.height,
                                    draw.rect.width,
                                    draw.rect.height,
                                    draw.rect.x,
                                    draw.rect.y,
                                    resource_name
                                ));
                            } else {
                                stream.push_str(&format!(
                                    "q {} 0 0 {} {} {} cm /{} Do Q ",
                                    draw.rect.width,
                                    draw.rect.height,
                                    draw.rect.x,
                                    draw.rect.y,
                                    resource_name
                                ));
                            }
                            if rotated {
                                stream.push_str("Q ");
                            }
                        } else {
                            push_image_placeholder(
                                &mut stream,
                                page.height_pt,
                                image,
                                ImagePlaceholderStatus::from_decode_failure(image),
                            );
                        }
                    } else {
                        push_image_placeholder(
                            &mut stream,
                            page.height_pt,
                            image,
                            ImagePlaceholderStatus::from_image(image),
                        );
                    }
                }
                DrawOp::LinkAnnotation(link) => {
                    let annotation_id = next_extra_object_id;
                    next_extra_object_id += 1;
                    annotation_refs.push(format!("{annotation_id} 0 R"));
                    extra_objects.push(format!(
                        "{annotation_id} 0 obj << /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] /A << /S /URI /URI ({}) >> >> endobj\n",
                        link.rect.x,
                        page.height_pt - link.rect.y - link.rect.height,
                        link.rect.x + link.rect.width,
                        page.height_pt - link.rect.y,
                        escape_pdf_text(&link.target)
                    )
                    .into_bytes());
                }
                _ => {}
            }
        }
        objects.push(
            format!(
                "{content_id} 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
                stream.len(),
                stream
            )
            .into_bytes(),
        );
        let annotations = if annotation_refs.is_empty() {
            String::new()
        } else {
            format!(" /Annots [{}]", annotation_refs.join(" "))
        };
        let xobjects = if image_resource_refs.is_empty() {
            String::new()
        } else {
            format!(" /XObject << {} >>", image_resource_refs.join(" "))
        };
        objects.push(format!(
            "{page_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {} >>{} >> /Contents {content_id} 0 R{} >> endobj\n",
            page.width_pt,
            page.height_pt,
            font_resources,
            xobjects,
            annotations
        )
        .into_bytes());
    }
    objects.extend(extra_objects);

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0usize];
    for object in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object);
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

struct DecodedPdfImage {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct ImageDrawPlacement {
    rect: Rect,
    clip_to_dest: bool,
}

#[derive(Debug, Clone)]
struct SimpleSvgAsset {
    natural_width_pt: f32,
    natural_height_pt: f32,
    rects: Vec<SimpleSvgRect>,
    lines: Vec<SimpleSvgLine>,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgRect {
    x_ratio: f32,
    y_ratio: f32,
    width_ratio: f32,
    height_ratio: f32,
    fill_rgb: Option<(f32, f32, f32)>,
    stroke_rgb: Option<(f32, f32, f32)>,
    stroke_width_ratio: f32,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgLine {
    x1_ratio: f32,
    y1_ratio: f32,
    x2_ratio: f32,
    y2_ratio: f32,
    stroke_rgb: (f32, f32, f32),
    stroke_width_ratio: f32,
}

fn image_natural_size_pt(image: &PositionedImage) -> Option<(f32, f32)> {
    let width = image.natural_width_pt?;
    let height = image.natural_height_pt?;
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
        .then_some((width, height))
}

fn image_natural_size_or_fallback(
    image: &PositionedImage,
    fallback_width: f32,
    fallback_height: f32,
) -> (f32, f32) {
    image_natural_size_pt(image).unwrap_or((fallback_width, fallback_height))
}

fn parse_simple_svg_asset(text: &str) -> Option<SimpleSvgAsset> {
    let tag_start = text.find("<svg")?;
    let tag_tail = &text[tag_start..];
    let tag_end = tag_tail.find('>')?;
    let svg_tag = &tag_tail[..tag_end];
    let attr_value = |tag: &str, name: &str| -> Option<String> {
        let mut offset = 0usize;
        while let Some(relative) = tag[offset..].find(name) {
            let index = offset + relative;
            let before = tag[..index].chars().next_back();
            if before
                .is_some_and(|ch| !(ch.is_whitespace() || matches!(ch, '<' | '/' | '\'' | '"')))
            {
                offset = index + name.len();
                continue;
            }
            let after = tag[index + name.len()..].trim_start();
            let after = after.strip_prefix('=')?.trim_start();
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let value_start = quote.len_utf8();
            let value_end = after[value_start..].find(quote)? + value_start;
            return Some(after[value_start..value_end].to_string());
        }
        None
    };
    let parse_number_prefix = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.is_empty() || raw.ends_with('%') {
            return None;
        }
        let split_at = raw
            .find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-')))
            .unwrap_or(raw.len());
        raw[..split_at]
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
    };
    let parse_length_pt = |raw: &str| -> Option<f32> {
        let value = parse_number_prefix(raw)?;
        let unit = raw
            .trim()
            .trim_start_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-'))
            .trim();
        let multiplier = match unit {
            "" | "px" => 72.0 / 96.0,
            "pt" | "bp" => 1.0,
            "in" => 72.0,
            "cm" => 72.0 / 2.54,
            "mm" => 72.0 / 25.4,
            "pc" => 12.0,
            _ => return None,
        };
        Some(value * multiplier)
    };
    let width_raw = attr_value(svg_tag, "width");
    let height_raw = attr_value(svg_tag, "height");
    let width_pt = width_raw.as_deref().and_then(parse_length_pt);
    let height_pt = height_raw.as_deref().and_then(parse_length_pt);
    let view_box = attr_value(svg_tag, "viewBox")
        .or_else(|| attr_value(svg_tag, "viewbox"))
        .and_then(|view_box| {
            let values = view_box
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter_map(|part| part.parse::<f32>().ok())
                .collect::<Vec<_>>();
            (values.len() >= 4
                && values[2].is_finite()
                && values[3].is_finite()
                && values[2] > 0.0
                && values[3] > 0.0)
                .then_some((values[0], values[1], values[2], values[3]))
        });
    let natural_size = if let (Some(width), Some(height)) = (width_pt, height_pt)
        && width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0
    {
        Some((width, height))
    } else {
        view_box.map(|(_, _, width, height)| (width * 72.0 / 96.0, height * 72.0 / 96.0))
    }?;
    let view_box = view_box.unwrap_or_else(|| {
        let width = width_raw
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(natural_size.0);
        let height = height_raw
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(natural_size.1);
        (0.0, 0.0, width.max(1.0), height.max(1.0))
    });
    let style_value = |tag: &str, name: &str| -> Option<String> {
        let style = attr_value(tag, "style")?;
        for declaration in style.split(';') {
            let Some((key, value)) = declaration.split_once(':') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case(name) {
                return Some(value.trim().to_string());
            }
        }
        None
    };
    let presentation_value = |tag: &str, name: &str| -> Option<String> {
        style_value(tag, name).or_else(|| attr_value(tag, name))
    };
    let parse_color = |raw: &str| -> Option<(f32, f32, f32)> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("none") {
            return None;
        }
        if let Some(hex) = raw.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
                return Some((r, g, b));
            }
            if hex.len() == 3 {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()? as f32 / 255.0;
                return Some((r, g, b));
            }
        }
        match raw.to_ascii_lowercase().as_str() {
            "black" => Some((0.0, 0.0, 0.0)),
            "white" => Some((1.0, 1.0, 1.0)),
            "gray" | "grey" => Some((0.5, 0.5, 0.5)),
            "red" => Some((1.0, 0.0, 0.0)),
            "green" => Some((0.0, 0.5, 0.0)),
            "blue" => Some((0.0, 0.0, 1.0)),
            _ => Some((0.0, 0.0, 0.0)),
        }
    };
    let stroke_width_ratio = |tag: &str| -> f32 {
        presentation_value(tag, "stroke-width")
            .as_deref()
            .and_then(parse_number_prefix)
            .filter(|width| *width > 0.0)
            .unwrap_or(1.0)
            / view_box.2
    };
    let svg_content_start = tag_start + tag_end + 1;
    let svg_content_end = text[svg_content_start..]
        .find("</svg>")
        .map(|relative| svg_content_start + relative)
        .unwrap_or(text.len());
    let svg_content = &text[svg_content_start..svg_content_end];
    let mut rects = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<rect") {
        let rect_start = search_index + relative;
        let rect_tail = &svg_content[rect_start..];
        let Some(rect_end) = rect_tail.find('>') else {
            break;
        };
        let rect_tag = &rect_tail[..rect_end];
        let x = attr_value(rect_tag, "x")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let y = attr_value(rect_tag, "y")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let Some(width) = attr_value(rect_tag, "width")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = rect_start + rect_end + 1;
            continue;
        };
        let Some(height) = attr_value(rect_tag, "height")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = rect_start + rect_end + 1;
            continue;
        };
        if width > 0.0 && height > 0.0 {
            let fill_rgb = presentation_value(rect_tag, "fill")
                .as_deref()
                .map(&parse_color)
                .unwrap_or(Some((0.0, 0.0, 0.0)));
            let stroke_rgb = presentation_value(rect_tag, "stroke")
                .as_deref()
                .and_then(&parse_color);
            if fill_rgb.is_some() || stroke_rgb.is_some() {
                rects.push(SimpleSvgRect {
                    x_ratio: (x - view_box.0) / view_box.2,
                    y_ratio: (y - view_box.1) / view_box.3,
                    width_ratio: width / view_box.2,
                    height_ratio: height / view_box.3,
                    fill_rgb,
                    stroke_rgb,
                    stroke_width_ratio: stroke_width_ratio(rect_tag),
                });
            }
        }
        search_index = rect_start + rect_end + 1;
    }
    let mut lines = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<line") {
        let line_start = search_index + relative;
        let line_tail = &svg_content[line_start..];
        let Some(line_end) = line_tail.find('>') else {
            break;
        };
        let line_tag = &line_tail[..line_end];
        let Some(x1) = attr_value(line_tag, "x1")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some(y1) = attr_value(line_tag, "y1")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some(x2) = attr_value(line_tag, "x2")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some(y2) = attr_value(line_tag, "y2")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        if (x1 != x2 || y1 != y2)
            && let Some(stroke_rgb) = presentation_value(line_tag, "stroke")
                .as_deref()
                .and_then(&parse_color)
        {
            lines.push(SimpleSvgLine {
                x1_ratio: (x1 - view_box.0) / view_box.2,
                y1_ratio: (y1 - view_box.1) / view_box.3,
                x2_ratio: (x2 - view_box.0) / view_box.2,
                y2_ratio: (y2 - view_box.1) / view_box.3,
                stroke_rgb,
                stroke_width_ratio: stroke_width_ratio(line_tag),
            });
        }
        search_index = line_start + line_end + 1;
    }
    Some(SimpleSvgAsset {
        natural_width_pt: natural_size.0,
        natural_height_pt: natural_size.1,
        rects,
        lines,
    })
}

fn image_draw_placement(
    dest: Rect,
    crop: Option<ImageCrop>,
    natural_width: f32,
    natural_height: f32,
    top_is_min_y: bool,
) -> ImageDrawPlacement {
    let mut placement = ImageDrawPlacement {
        rect: dest,
        clip_to_dest: false,
    };
    let Some(crop) = crop else {
        return placement;
    };
    let (mut source_left, mut source_bottom, mut source_right, mut source_top) =
        if let Some(viewport) = crop.viewport {
            (
                viewport.llx_pt,
                viewport.lly_pt,
                viewport.urx_pt,
                viewport.ury_pt,
            )
        } else {
            (0.0, 0.0, natural_width, natural_height)
        };
    if let Some(trim) = crop.trim {
        source_left += trim.left_pt;
        source_bottom += trim.bottom_pt;
        source_right -= trim.right_pt;
        source_top -= trim.top_pt;
    }
    let source_width = source_right - source_left;
    let source_height = source_top - source_bottom;
    if !source_width.is_finite()
        || !source_height.is_finite()
        || source_width <= 0.0
        || source_height <= 0.0
    {
        return placement;
    }
    let scale_x = dest.width / source_width;
    let scale_y = dest.height / source_height;
    if !scale_x.is_finite() || !scale_y.is_finite() || scale_x <= 0.0 || scale_y <= 0.0 {
        return placement;
    }
    placement.rect.x = dest.x - source_left * scale_x;
    placement.rect.y = if top_is_min_y {
        dest.y - (natural_height - source_top) * scale_y
    } else {
        dest.y - source_bottom * scale_y
    };
    placement.rect.width = natural_width * scale_x;
    placement.rect.height = natural_height * scale_y;
    placement.clip_to_dest = crop.clip;
    placement
}

fn decode_pdf_image(bytes: &[u8]) -> Option<DecodedPdfImage> {
    let image = image::load_from_memory(bytes).ok()?.to_rgb8();
    let (width, height) = image.dimensions();
    Some(DecodedPdfImage {
        width,
        height,
        rgb: image.into_raw(),
    })
}

fn build_image_xobject(object_id: usize, image: &DecodedPdfImage) -> Vec<u8> {
    let mut object = format!(
        "{object_id} 0 obj << /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >> stream\n",
        image.width,
        image.height,
        image.rgb.len()
    )
    .into_bytes();
    object.extend_from_slice(&image.rgb);
    object.extend_from_slice(b"\nendstream\nendobj\n");
    object
}

fn image_rotation_pivot(rect: Rect, origin: Option<&str>, top_is_min_y: bool) -> Point {
    let origin = origin.unwrap_or("lb");
    let x = if origin.contains('l') {
        rect.x
    } else if origin.contains('r') {
        rect.x + rect.width
    } else {
        rect.x + rect.width / 2.0
    };
    let y = if origin.contains('t') {
        if top_is_min_y {
            rect.y
        } else {
            rect.y + rect.height
        }
    } else if origin.contains('b') || origin.contains('B') {
        if top_is_min_y {
            rect.y + rect.height
        } else {
            rect.y
        }
    } else {
        rect.y + rect.height / 2.0
    };
    Point { x, y }
}

fn image_pdf_rotation_matrix(image: &PositionedImage, page_height_pt: f32) -> Option<[f32; 6]> {
    let rotation = image
        .rotation
        .as_ref()
        .filter(|rotation| rotation.angle_degrees != 0.0)?;
    let rect = Rect {
        x: image.rect.x,
        y: page_height_pt - image.rect.y - image.rect.height,
        width: image.rect.width,
        height: image.rect.height,
    };
    let pivot = image_rotation_pivot(rect, rotation.origin.as_deref(), false);
    let radians = rotation.angle_degrees.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let cosine = if cosine.abs() < 0.000_001 {
        0.0
    } else {
        cosine
    };
    let sine = if sine.abs() < 0.000_001 { 0.0 } else { sine };
    Some([
        cosine,
        sine,
        -sine,
        cosine,
        pivot.x - cosine * pivot.x + sine * pivot.y,
        pivot.y - sine * pivot.x - cosine * pivot.y,
    ])
}

fn push_pdf_image_rotation(
    stream: &mut String,
    page_height_pt: f32,
    image: &PositionedImage,
) -> bool {
    let Some([a, b, c, d, e, f]) = image_pdf_rotation_matrix(image, page_height_pt) else {
        return false;
    };
    stream.push_str(&format!("q {a} {b} {c} {d} {e} {f} cm "));
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImagePlaceholderStatus {
    Generic,
    Draft,
    Missing,
    Unsupported,
    Undecodable,
    Diagnostic,
}

impl ImagePlaceholderStatus {
    fn from_image(image: &PositionedImage) -> Self {
        match image.diagnostic.as_deref() {
            Some(message) if message.starts_with("draft graphic asset ") => Self::Draft,
            Some(message) if message.starts_with("missing graphic asset ") => Self::Missing,
            Some(message) if message.starts_with("unsupported graphic asset format ") => {
                Self::Unsupported
            }
            Some(_) => Self::Diagnostic,
            None => Self::Generic,
        }
    }

    fn from_decode_failure(image: &PositionedImage) -> Self {
        match Self::from_image(image) {
            Self::Generic => match image.asset_format {
                Some(GraphicAssetFormat::Png | GraphicAssetFormat::Jpeg) => Self::Undecodable,
                _ => Self::Unsupported,
            },
            status => status,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Draft => "draft",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
            Self::Undecodable => "undecodable",
            Self::Diagnostic => "diagnostic",
        }
    }

    fn label_prefix(self) -> &'static str {
        match self {
            Self::Generic => "image",
            Self::Draft => "draft image",
            Self::Missing => "missing image",
            Self::Unsupported => "unsupported image",
            Self::Undecodable => "undecodable image",
            Self::Diagnostic => "image warning",
        }
    }
}

fn image_placeholder_text(image: &PositionedImage, status: ImagePlaceholderStatus) -> String {
    format!("[{}: {}]", status.label_prefix(), image.asset_ref)
}

fn push_image_placeholder(
    stream: &mut String,
    page_height_pt: f32,
    image: &PositionedImage,
    status: ImagePlaceholderStatus,
) {
    let rotated = push_pdf_image_rotation(stream, page_height_pt, image);
    stream.push_str(&format!(
        "q 0.92 g {} {} {} {} re f 0 G {} {} {} {} re S Q ",
        image.rect.x,
        page_height_pt - image.rect.y - image.rect.height,
        image.rect.width,
        image.rect.height,
        image.rect.x,
        page_height_pt - image.rect.y - image.rect.height,
        image.rect.width,
        image.rect.height
    ));
    stream.push_str("BT /F1 8 Tf ");
    stream.push_str(&format!(
        "1 0 0 1 {} {} Tm ",
        image.rect.x + 4.0,
        page_height_pt - image.rect.y - image.rect.height / 2.0
    ));
    stream.push('(');
    stream.push_str(&escape_pdf_text(&image_placeholder_text(image, status)));
    stream.push_str(") Tj ET ");
    if rotated {
        stream.push_str("Q ");
    }
}

pub fn render_display_list_svg(page: &PageDisplayList) -> String {
    render_display_list_svg_with_assets(page, |_| None)
}

pub fn render_display_list_svg_with_assets(
    page: &PageDisplayList,
    resolve_asset: impl FnMut(&str) -> Option<Vec<u8>>,
) -> String {
    render_display_list_svg_with_converted_assets(page, resolve_asset, |_, _| None)
}

pub fn render_display_list_svg_with_converted_assets(
    page: &PageDisplayList,
    mut resolve_asset: impl FnMut(&str) -> Option<Vec<u8>>,
    mut convert_asset: impl FnMut(&PositionedImage, &[u8]) -> Option<ConvertedImageAsset>,
) -> String {
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
    let generated_by_name = |generated_by| match generated_by {
        tex_render_model::GeneratedBy::Source => "source",
        tex_render_model::GeneratedBy::MacroExpansion => "macro_expansion",
        tex_render_model::GeneratedBy::Command => "command",
        tex_render_model::GeneratedBy::Shim => "shim",
        tex_render_model::GeneratedBy::AuxFile => "aux_file",
        tex_render_model::GeneratedBy::Fallback => "fallback",
        tex_render_model::GeneratedBy::Generated => "generated",
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
        source_attrs.push_str(&format!(
            " data-source-generated-by=\"{}\"",
            generated_by_name(source.generated_by)
        ));
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
                let mut source_attrs = source_attrs_for(&run.source);
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
                let page_selection_attrs = image
                    .page_selection
                    .as_ref()
                    .map(|selection| {
                        let page_attr = selection
                            .page
                            .map(|page| format!(" data-image-page=\"{page}\""))
                            .unwrap_or_default();
                        let pagebox_attr = selection
                            .pagebox
                            .as_deref()
                            .map(|pagebox| {
                                format!(" data-image-pagebox=\"{}\"", escape_xml_text(pagebox))
                            })
                            .unwrap_or_default();
                        format!("{page_attr}{pagebox_attr}")
                    })
                    .unwrap_or_default();
                let asset_hash_attr = image
                    .asset_hash
                    .as_deref()
                    .map(|hash| format!(" data-image-asset-hash=\"{}\"", escape_xml_text(hash)))
                    .unwrap_or_default();
                let crop_attrs = image
                    .crop
                    .map(|crop| {
                        let mut attrs = format!(" data-image-crop-clip=\"{}\"", crop.clip);
                        if let Some(trim) = crop.trim {
                            attrs.push_str(&format!(
                                " data-image-crop-trim=\"{},{},{},{}\"",
                                trim.left_pt, trim.bottom_pt, trim.right_pt, trim.top_pt
                            ));
                        }
                        if let Some(viewport) = crop.viewport {
                            attrs.push_str(&format!(
                                " data-image-crop-viewport=\"{},{},{},{}\"",
                                viewport.llx_pt, viewport.lly_pt, viewport.urx_pt, viewport.ury_pt
                            ));
                        }
                        attrs
                    })
                    .unwrap_or_default();
                let rotation_attrs = image
                    .rotation
                    .as_ref()
                    .map(|rotation| {
                        let origin_attr = rotation
                            .origin
                            .as_deref()
                            .map(|origin| {
                                format!(
                                    " data-image-rotation-origin=\"{}\"",
                                    escape_xml_text(origin)
                                )
                            })
                            .unwrap_or_default();
                        format!(
                            " data-image-rotation-angle=\"{}\"{}",
                            rotation.angle_degrees, origin_attr
                        )
                    })
                    .unwrap_or_default();
                let scale_attrs = image
                    .scale
                    .map(|scale| {
                        format!(
                            " data-image-scale-x=\"{}\" data-image-scale-y=\"{}\"",
                            scale.x, scale.y
                        )
                    })
                    .unwrap_or_default();
                let transform_attr = image
                    .rotation
                    .as_ref()
                    .filter(|rotation| rotation.angle_degrees != 0.0)
                    .map(|rotation| {
                        let pivot =
                            image_rotation_pivot(image.rect, rotation.origin.as_deref(), true);
                        format!(
                            " transform=\"rotate({} {} {})\"",
                            -rotation.angle_degrees, pivot.x, pivot.y
                        )
                    })
                    .unwrap_or_default();
                let placeholder_status = ImagePlaceholderStatus::from_image(image);
                let mut embedded_decode_failure = false;
                let embedded_image = if placeholder_status == ImagePlaceholderStatus::Generic {
                    resolve_asset(&image.asset_ref).and_then(|bytes| {
                        let mut converted_format = None;
                        let (media_type, natural_size, data_bytes) = match image.asset_format {
                            Some(GraphicAssetFormat::Svg) => {
                                let parsed_svg = std::str::from_utf8(&bytes)
                                    .ok()
                                    .and_then(parse_simple_svg_asset);
                                let Some(parsed_svg) = parsed_svg else {
                                    embedded_decode_failure = true;
                                    return None;
                                };
                                let natural_size = Some(image_natural_size_or_fallback(
                                    image,
                                    parsed_svg.natural_width_pt,
                                    parsed_svg.natural_height_pt,
                                ));
                                ("image/svg+xml;charset=utf-8", natural_size, bytes)
                            }
                            Some(GraphicAssetFormat::Png) => {
                                let Some(decoded_image) = image::load_from_memory(&bytes).ok()
                                else {
                                    embedded_decode_failure = true;
                                    return None;
                                };
                                let natural_size = image_natural_size_or_fallback(
                                    image,
                                    decoded_image.width() as f32,
                                    decoded_image.height() as f32,
                                );
                                ("image/png", Some(natural_size), bytes)
                            }
                            Some(GraphicAssetFormat::Jpeg) => {
                                let Some(decoded_image) = image::load_from_memory(&bytes).ok()
                                else {
                                    embedded_decode_failure = true;
                                    return None;
                                };
                                let natural_size = image_natural_size_or_fallback(
                                    image,
                                    decoded_image.width() as f32,
                                    decoded_image.height() as f32,
                                );
                                ("image/jpeg", Some(natural_size), bytes)
                            }
                            Some(GraphicAssetFormat::Pdf | GraphicAssetFormat::Eps) => {
                                let Some(converted) = convert_asset(image, &bytes) else {
                                    embedded_decode_failure = true;
                                    return None;
                                };
                                converted_format = Some(converted.format);
                                match converted.format {
                                    GraphicAssetFormat::Png => {
                                        let Some(decoded_image) =
                                            image::load_from_memory(&converted.bytes).ok()
                                        else {
                                            embedded_decode_failure = true;
                                            return None;
                                        };
                                        let natural_size = image_natural_size_or_fallback(
                                            image,
                                            decoded_image.width() as f32,
                                            decoded_image.height() as f32,
                                        );
                                        ("image/png", Some(natural_size), converted.bytes)
                                    }
                                    GraphicAssetFormat::Jpeg => {
                                        let Some(decoded_image) =
                                            image::load_from_memory(&converted.bytes).ok()
                                        else {
                                            embedded_decode_failure = true;
                                            return None;
                                        };
                                        let natural_size = image_natural_size_or_fallback(
                                            image,
                                            decoded_image.width() as f32,
                                            decoded_image.height() as f32,
                                        );
                                        ("image/jpeg", Some(natural_size), converted.bytes)
                                    }
                                    _ => {
                                        embedded_decode_failure = true;
                                        return None;
                                    }
                                }
                            }
                            _ => {
                                embedded_decode_failure = true;
                                return None;
                            }
                        };
                        let mut data_uri = format!("data:{media_type},");
                        for byte in data_bytes {
                            match byte {
                                b'A'..=b'Z'
                                | b'a'..=b'z'
                                | b'0'..=b'9'
                                | b'-'
                                | b'_'
                                | b'.'
                                | b'~' => data_uri.push(byte as char),
                                _ => {
                                    data_uri.push_str(&format!("%{byte:02X}"));
                                }
                            }
                        }
                        let converted_format_attr = converted_format
                            .map(|format| {
                                format!(" data-image-converted-format=\"{}\"", format.as_str())
                            })
                            .unwrap_or_default();
                        Some((data_uri, natural_size, converted_format_attr))
                    })
                } else {
                    None
                };
                if let Some((href, natural_size, converted_format_attr)) = embedded_image {
                    let draw = natural_size
                        .map(|(natural_width, natural_height)| {
                            image_draw_placement(
                                image.rect,
                                image.crop,
                                natural_width,
                                natural_height,
                                true,
                            )
                        })
                        .unwrap_or(ImageDrawPlacement {
                            rect: image.rect,
                            clip_to_dest: false,
                        });
                    let (clip_prefix, clip_attrs) = if draw.clip_to_dest {
                        let clip_id = format!("image-clip-{clip_index}");
                        clip_index += 1;
                        (
                            format!(
                                "<defs><clipPath id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs>",
                                clip_id,
                                image.rect.x,
                                image.rect.y,
                                image.rect.width,
                                image.rect.height
                            ),
                            format!(
                                " clip-path=\"url(#{clip_id})\" data-image-crop-rendered=\"true\""
                            ),
                        )
                    } else {
                        (String::new(), String::new())
                    };
                    body.push_str(&format!(
                        "{}<g data-image-asset-ref=\"{}\"{}{}{}{}{}{} data-image-embedded=\"true\"{}{}{}{}><image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{}\" preserveAspectRatio=\"none\"/></g>",
                        clip_prefix,
                        escape_xml_text(&image.asset_ref),
                        asset_format_attr,
                        page_selection_attrs,
                        asset_hash_attr,
                        crop_attrs,
                        rotation_attrs,
                        scale_attrs,
                        transform_attr,
                        clip_attrs,
                        converted_format_attr,
                        source_attrs_for(&image.source),
                        draw.rect.x,
                        draw.rect.y,
                        draw.rect.width,
                        draw.rect.height,
                        escape_xml_text(&href)
                    ));
                    continue;
                }
                let placeholder_status = if embedded_decode_failure {
                    ImagePlaceholderStatus::from_decode_failure(image)
                } else {
                    placeholder_status
                };
                let placeholder_attrs = if placeholder_status == ImagePlaceholderStatus::Generic {
                    String::new()
                } else {
                    let diagnostic_attr = image
                        .diagnostic
                        .as_deref()
                        .map(|diagnostic| {
                            format!(" data-image-diagnostic=\"{}\"", escape_xml_text(diagnostic))
                        })
                        .unwrap_or_default();
                    format!(
                        " data-image-placeholder-kind=\"{}\"{}",
                        placeholder_status.as_str(),
                        diagnostic_attr
                    )
                };
                let placeholder_text = image_placeholder_text(image, placeholder_status);
                body.push_str(&format!(
                    "<g data-image-asset-ref=\"{}\"{}{}{}{}{}{}{}{}{}><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#e5e7eb\" stroke=\"#6b7280\" stroke-width=\"1\"/><text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"9\" fill=\"#374151\">{}</text></g>",
                    escape_xml_text(&image.asset_ref),
                    asset_format_attr,
                    page_selection_attrs,
                    asset_hash_attr,
                    crop_attrs,
                    rotation_attrs,
                    scale_attrs,
                    placeholder_attrs,
                    transform_attr,
                    source_attrs_for(&image.source),
                    image.rect.x,
                    image.rect.y,
                    image.rect.width,
                    image.rect.height,
                    image.rect.x + 4.0,
                    image.rect.y + image.rect.height / 2.0,
                    escape_xml_text(&placeholder_text)
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
        FontShape, GraphicAssetFormat, ImageCrop, ImageRotation, ImageScale, ImageTrim,
        ImageViewport, LinkAnnotation, PageDisplayList, Point, PositionedImage, PositionedTextRun,
        ProvenanceSpan, Rect, SourceProvenance, SourceSpan, SourceSpanRole, TextCluster,
    };

    use super::{
        ConvertedImageAsset, render_display_list_pdf, render_display_list_pdf_with_assets,
        render_display_list_pdf_with_converted_assets, render_display_list_svg,
        render_display_list_svg_with_assets, render_display_list_svg_with_converted_assets,
        render_page_svg, render_pdf, render_single_page_pdf,
    };

    fn tiny_png_bytes() -> Vec<u8> {
        use image::ImageEncoder;

        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &[
                    255, 0, 0, 0, 255, 0, //
                    0, 0, 255, 255, 255, 0,
                ],
                2,
                2,
                image::ExtendedColorType::Rgb8,
            )
            .expect("encode png");
        bytes
    }

    fn tiny_jpeg_bytes() -> Vec<u8> {
        use image::ImageEncoder;

        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .write_image(
                &[
                    255, 0, 0, 0, 255, 0, //
                    0, 0, 255, 255, 255, 0,
                ],
                2,
                2,
                image::ExtendedColorType::Rgb8,
            )
            .expect("encode jpeg");
        bytes
    }

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
    fn display_list_svg_escapes_generated_source_provenance() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::TextRun(PositionedTextRun {
                origin: Point { x: 72.0, y: 72.0 },
                text: "Shim text".to_string(),
                font: FontRequest {
                    family: FontFamilyRequest::Serif,
                    series: FontSeries::Regular,
                    shape: FontShape::Upright,
                    size_pt: 11.0,
                    role: FontRole::Body,
                },
                size_pt: 11.0,
                approximate_advance_pt: 49.5,
                glyphs: None,
                clusters: None,
                source: SourceProvenance::generated(
                    "shim:article<&\"",
                    "article <class> & \"title\"",
                )
                .with_related(
                    SourceSpanRole::EmitSite,
                    ProvenanceSpan::Generated(tex_render_model::GeneratedSpan {
                        stable_id: "emit<&\"".to_string(),
                        description: "emit <site> & \"flush\"".to_string(),
                    }),
                ),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg(&page);

        assert!(svg.contains("data-source-kind=\"generated\""));
        assert!(svg.contains("data-source-generated-by=\"generated\""));
        assert!(svg.contains("data-source-generated-id=\"shim:article&lt;&amp;&quot;\""));
        assert!(
            svg.contains(
                "data-source-description=\"article &lt;class&gt; &amp; &quot;title&quot;\""
            )
        );
        assert!(svg.contains("data-source-related-count=\"1\""));
        assert!(svg.contains("data-source-related-roles=\"emit_site\""));
        assert!(svg.contains(
            "data-source-related-spans=\"emit_site:generated:emit&lt;&amp;&quot;:emit &lt;site&gt; &amp; &quot;flush&quot;\""
        ));
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
                page_selection: None,
                asset_hash: Some("blake3:asset-hash".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: Some(ImageCrop {
                    trim: Some(ImageTrim {
                        left_pt: 1.0,
                        bottom_pt: 2.0,
                        right_pt: 3.0,
                        top_pt: 4.0,
                    }),
                    viewport: Some(ImageViewport {
                        llx_pt: 0.0,
                        lly_pt: 0.0,
                        urx_pt: 144.0,
                        ury_pt: 72.0,
                    }),
                    clip: true,
                }),
                scale: Some(ImageScale { x: -1.0, y: 2.0 }),
                rotation: Some(ImageRotation {
                    angle_degrees: 90.0,
                    origin: Some("c".to_string()),
                }),
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("q 0 1 -1 0 822 534 cm q 0.92 g"));
        assert!(pdf_text.contains("q 0.92 g 72 642 144 72 re f 0 G 72 642 144 72 re S Q"));
        assert!(
            pdf_text
                .contains(r#"BT /F1 8 Tf 1 0 0 1 76 678 Tm ([image: figures/a\(b\)&c.pdf]) Tj ET"#)
        );
        assert!(svg.contains("data-image-asset-ref=\"figures/a(b)&amp;c.pdf\""));
        assert!(svg.contains("data-image-asset-format=\"pdf\""));
        assert!(svg.contains("data-image-asset-hash=\"blake3:asset-hash\""));
        assert!(svg.contains("data-image-crop-clip=\"true\""));
        assert!(svg.contains("data-image-crop-trim=\"1,2,3,4\""));
        assert!(svg.contains("data-image-crop-viewport=\"0,0,144,72\""));
        assert!(svg.contains("data-image-rotation-angle=\"90\""));
        assert!(svg.contains("data-image-rotation-origin=\"c\""));
        assert!(svg.contains("data-image-scale-x=\"-1\""));
        assert!(svg.contains("data-image-scale-y=\"2\""));
        assert!(svg.contains("transform=\"rotate(-90 144 114)\""));
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
    fn renders_resolved_svg_assets_as_svg_image_elements() {
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
                asset_ref: "figures/vector.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:vector".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/vector.svg").then(|| {
                br#"<svg width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec()
            })
        });

        assert!(svg.contains("data-image-asset-ref=\"figures/vector.svg\""));
        assert!(svg.contains("data-image-asset-format=\"svg\""));
        assert!(svg.contains("data-image-embedded=\"true\""));
        assert!(svg.contains("<image x=\"72\" y=\"78\" width=\"144\" height=\"72\""));
        assert!(svg.contains("href=\"data:image/svg+xml;charset=utf-8,%3Csvg"));
        assert!(!svg.contains("[image: figures/vector.svg]"));
        assert!(!svg.contains("data-image-placeholder-kind="));
    }

    #[test]
    fn renders_simple_svg_rect_assets_as_pdf_vector_content() {
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
                asset_ref: "figures/vector.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:vector".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| {
            (asset_ref == "figures/vector.svg").then(|| {
                br#"<svg width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 0 rg 72 642 144 72 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/vector.svg]"));
        assert!(!pdf_text.contains("[image: figures/vector.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_style_strokes_and_lines_as_pdf_vector_content() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 300.0,
            height_pt: 300.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 10.0,
                    y: 20.0,
                    width: 200.0,
                    height: 100.0,
                },
                asset_ref: "figures/stroked.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:stroked".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| {
            (asset_ref == "figures/stroked.svg").then(|| {
                br##"<svg width="20" height="10">
  <rect x="2" y="1" width="10" height="4" style="fill:#ff0000;stroke:#0000ff;stroke-width:0.5"/>
  <line x1="0" y1="10" x2="20" y2="0" stroke="#00ff00" stroke-width="1"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 30 230 100 40 re f"));
        assert!(pdf_text.contains("0 0 1 RG 5 w 30 230 100 40 re S"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 180 m 210 280 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/stroked.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_resolved_bitmap_assets_as_svg_image_elements() {
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
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });

        assert!(svg.contains("data-image-asset-ref=\"figures/tiny.png\""));
        assert!(svg.contains("data-image-asset-format=\"png\""));
        assert!(svg.contains("data-image-embedded=\"true\""));
        assert!(svg.contains("<image x=\"72\" y=\"78\" width=\"144\" height=\"72\""));
        assert!(svg.contains("href=\"data:image/png,%89PNG"));
        assert!(!svg.contains("[image: figures/tiny.png]"));
        assert!(!svg.contains("data-image-placeholder-kind="));
    }

    #[test]
    fn renders_converted_pdf_assets_as_pdf_and_svg_images() {
        let source = SourceProvenance::file("main.tex", 0, 10);
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
                asset_ref: "figures/vector.pdf".to_string(),
                asset_format: Some(GraphicAssetFormat::Pdf),
                page_selection: None,
                asset_hash: Some("blake3:vector-pdf".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_converted_assets(
            &[page.clone()],
            |asset_ref| (asset_ref == "figures/vector.pdf").then(|| b"%PDF-1.4".to_vec()),
            |image, bytes| {
                (image.asset_ref == "figures/vector.pdf" && bytes.starts_with(b"%PDF")).then(|| {
                    ConvertedImageAsset {
                        bytes: tiny_png_bytes(),
                        format: GraphicAssetFormat::Png,
                    }
                })
            },
        );
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg_with_converted_assets(
            &page,
            |asset_ref| (asset_ref == "figures/vector.pdf").then(|| b"%PDF-1.4".to_vec()),
            |image, bytes| {
                (image.asset_ref == "figures/vector.pdf" && bytes.starts_with(b"%PDF")).then(|| {
                    ConvertedImageAsset {
                        bytes: tiny_png_bytes(),
                        format: GraphicAssetFormat::Png,
                    }
                })
            },
        );

        assert!(pdf_text.contains("/Subtype /Image"));
        assert!(!pdf_text.contains("[unsupported image: figures/vector.pdf]"));
        assert!(svg.contains("data-image-asset-ref=\"figures/vector.pdf\""));
        assert!(svg.contains("data-image-asset-format=\"pdf\""));
        assert!(svg.contains("data-image-converted-format=\"png\""));
        assert!(svg.contains("data-image-embedded=\"true\""));
        assert!(svg.contains("href=\"data:image/png,%89PNG"));
        assert!(!svg.contains("[unsupported image: figures/vector.pdf]"));
    }

    #[test]
    fn renders_converted_pdf_crop_with_original_natural_size() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 50.0,
                },
                asset_ref: "figures/vector.pdf".to_string(),
                asset_format: Some(GraphicAssetFormat::Pdf),
                page_selection: None,
                asset_hash: Some("blake3:vector-pdf".to_string()),
                natural_width_pt: Some(200.0),
                natural_height_pt: Some(100.0),
                crop: Some(ImageCrop {
                    trim: None,
                    viewport: Some(ImageViewport {
                        llx_pt: 50.0,
                        lly_pt: 25.0,
                        urx_pt: 150.0,
                        ury_pt: 75.0,
                    }),
                    clip: true,
                }),
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_converted_assets(
            &[page.clone()],
            |asset_ref| (asset_ref == "figures/vector.pdf").then(|| b"%PDF-1.4".to_vec()),
            |image, bytes| {
                (image.asset_ref == "figures/vector.pdf" && bytes.starts_with(b"%PDF")).then(|| {
                    ConvertedImageAsset {
                        bytes: tiny_png_bytes(),
                        format: GraphicAssetFormat::Png,
                    }
                })
            },
        );
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg_with_converted_assets(
            &page,
            |asset_ref| (asset_ref == "figures/vector.pdf").then(|| b"%PDF-1.4".to_vec()),
            |image, bytes| {
                (image.asset_ref == "figures/vector.pdf" && bytes.starts_with(b"%PDF")).then(|| {
                    ConvertedImageAsset {
                        bytes: tiny_png_bytes(),
                        format: GraphicAssetFormat::Png,
                    }
                })
            },
        );

        assert!(pdf_text.contains("q 72 670 100 50 re W n q 200 0 0 100 22 645 cm /Im1 Do Q Q"));
        assert!(pdf_text.contains("/Width 2"));
        assert!(pdf_text.contains("/Height 2"));
        assert!(svg.contains("<clipPath id=\"image-clip-0\"><rect x=\"72\" y=\"72\" width=\"100\" height=\"50\"/></clipPath>"));
        assert!(svg.contains("data-image-converted-format=\"png\""));
        assert!(svg.contains("data-image-crop-rendered=\"true\""));
        assert!(svg.contains("<image x=\"22\" y=\"47\" width=\"200\" height=\"100\""));
        assert!(!svg.contains("[unsupported image: figures/vector.pdf]"));
    }

    #[test]
    fn resolved_unconverted_pdf_assets_surface_unsupported_placeholder() {
        let source = SourceProvenance::file("main.tex", 0, 10);
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
                asset_ref: "figures/vector.pdf".to_string(),
                asset_format: Some(GraphicAssetFormat::Pdf),
                page_selection: None,
                asset_hash: Some("blake3:vector-pdf".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page.clone()], |asset_ref| {
            (asset_ref == "figures/vector.pdf").then(|| b"%PDF-1.4".to_vec())
        });
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/vector.pdf").then(|| b"%PDF-1.4".to_vec())
        });

        assert!(pdf_text.contains("[unsupported image: figures/vector.pdf]"));
        assert!(svg.contains("data-image-placeholder-kind=\"unsupported\""));
        assert!(svg.contains("[unsupported image: figures/vector.pdf]"));
        assert!(!svg.contains("data-image-embedded=\"true\""));
    }

    #[test]
    fn renders_clip_enabled_png_crop_with_svg_clipping() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 100.0,
                },
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: Some(ImageCrop {
                    trim: Some(ImageTrim {
                        left_pt: 1.0,
                        bottom_pt: 0.0,
                        right_pt: 0.0,
                        top_pt: 0.0,
                    }),
                    viewport: None,
                    clip: true,
                }),
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });

        assert!(svg.contains("<clipPath id=\"image-clip-0\"><rect x=\"72\" y=\"72\" width=\"100\" height=\"100\"/></clipPath>"));
        assert!(svg.contains("clip-path=\"url(#image-clip-0)\""));
        assert!(svg.contains("data-image-crop-rendered=\"true\""));
        assert!(svg.contains("<image x=\"-28\" y=\"72\" width=\"200\" height=\"100\""));
        assert!(svg.contains("href=\"data:image/png,%89PNG"));
        assert!(!svg.contains("[image: figures/tiny.png]"));
    }

    #[test]
    fn renders_clip_enabled_svg_crop_with_svg_clipping() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 50.0,
                },
                asset_ref: "figures/vector.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:vector".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: Some(ImageCrop {
                    trim: None,
                    viewport: Some(ImageViewport {
                        llx_pt: 50.0,
                        lly_pt: 25.0,
                        urx_pt: 150.0,
                        ury_pt: 75.0,
                    }),
                    clip: true,
                }),
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/vector.svg")
                .then(|| br#"<svg width="200pt" height="100pt" viewBox="0 0 200 100"><rect width="200" height="100"/></svg>"#.to_vec())
        });

        assert!(svg.contains("<clipPath id=\"image-clip-0\"><rect x=\"72\" y=\"72\" width=\"100\" height=\"50\"/></clipPath>"));
        assert!(svg.contains("clip-path=\"url(#image-clip-0)\""));
        assert!(svg.contains("data-image-crop-rendered=\"true\""));
        assert!(svg.contains("<image x=\"22\" y=\"47\" width=\"200\" height=\"100\""));
        assert!(svg.contains("href=\"data:image/svg+xml;charset=utf-8,%3Csvg"));
        assert!(!svg.contains("[image: figures/vector.svg]"));
    }

    #[test]
    fn renders_clip_disabled_png_viewport_with_svg_offset_without_clipping() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 50.0,
                },
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: Some(ImageCrop {
                    trim: None,
                    viewport: Some(ImageViewport {
                        llx_pt: 0.5,
                        lly_pt: 0.5,
                        urx_pt: 1.5,
                        ury_pt: 1.5,
                    }),
                    clip: false,
                }),
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });

        assert!(!svg.contains("<clipPath"));
        assert!(!svg.contains("data-image-crop-rendered=\"true\""));
        assert!(svg.contains("<image x=\"22\" y=\"47\" width=\"200\" height=\"100\""));
        assert!(svg.contains("href=\"data:image/png,%89PNG"));
        assert!(!svg.contains("[image: figures/tiny.png]"));
    }

    #[test]
    fn renders_resolved_png_assets_as_pdf_image_xobjects() {
        let source = SourceProvenance::file("main.tex", 0, 10);
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
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("/XObject << /Im1 17 0 R >>"));
        assert!(pdf_text.contains("q 144 0 0 72 72 642 cm /Im1 Do Q"));
        assert!(pdf_text.contains("/Subtype /Image"));
        assert!(pdf_text.contains("/Width 2"));
        assert!(pdf_text.contains("/Height 2"));
        assert!(pdf_text.contains("/ColorSpace /DeviceRGB"));
        assert!(pdf_text.contains("/BitsPerComponent 8"));
        assert!(!pdf_text.contains("[image: figures/tiny.png]"));
    }

    #[test]
    fn renders_resolved_jpeg_assets_as_pdf_and_svg_images() {
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
                asset_ref: "figures/photo.jpg".to_string(),
                asset_format: Some(GraphicAssetFormat::Jpeg),
                page_selection: None,
                asset_hash: Some("blake3:photo".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source: SourceProvenance::file("main.tex", 0, 10),
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page.clone()], |asset_ref| {
            (asset_ref == "figures/photo.jpg").then(tiny_jpeg_bytes)
        });
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/photo.jpg").then(tiny_jpeg_bytes)
        });

        assert!(pdf_text.contains("/Subtype /Image"));
        assert!(pdf_text.contains("/Width 2"));
        assert!(pdf_text.contains("/Height 2"));
        assert!(!pdf_text.contains("[image: figures/photo.jpg]"));
        assert!(svg.contains("data-image-asset-format=\"jpeg\""));
        assert!(svg.contains("data-image-embedded=\"true\""));
        assert!(svg.contains("href=\"data:image/jpeg,%FF%D8"));
        assert!(!svg.contains("[image: figures/photo.jpg]"));
    }

    #[test]
    fn renders_rotated_png_assets_with_pdf_matrix() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 50.0,
                },
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: Some(ImageRotation {
                    angle_degrees: 90.0,
                    origin: Some("c".to_string()),
                }),
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("q 0 1 -1 0 817 573 cm q 100 0 0 50 72 670 cm /Im1 Do Q Q"));
        assert!(pdf_text.contains("/Subtype /Image"));
        assert!(!pdf_text.contains("[image: figures/tiny.png]"));
    }

    #[test]
    fn renders_clip_enabled_png_crop_with_pdf_clipping() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 100.0,
                },
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: Some(ImageCrop {
                    trim: Some(ImageTrim {
                        left_pt: 1.0,
                        bottom_pt: 0.0,
                        right_pt: 0.0,
                        top_pt: 0.0,
                    }),
                    viewport: None,
                    clip: true,
                }),
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("q 72 620 100 100 re W n q 200 0 0 100 -28 620 cm /Im1 Do Q Q"));
        assert!(pdf_text.contains("/Subtype /Image"));
        assert!(!pdf_text.contains("[image: figures/tiny.png]"));
    }

    #[test]
    fn renders_clip_disabled_png_viewport_with_pdf_offset_without_clipping() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![DrawOp::Image(PositionedImage {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 100.0,
                    height: 50.0,
                },
                asset_ref: "figures/tiny.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:tiny".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: Some(ImageCrop {
                    trim: None,
                    viewport: Some(ImageViewport {
                        llx_pt: 0.5,
                        lly_pt: 0.5,
                        urx_pt: 1.5,
                        ury_pt: 1.5,
                    }),
                    clip: false,
                }),
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| {
            (asset_ref == "figures/tiny.png").then(tiny_png_bytes)
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(!pdf_text.contains(" re W n "));
        assert!(pdf_text.contains("q 200 0 0 100 22 645 cm /Im1 Do Q"));
        assert!(pdf_text.contains("/Subtype /Image"));
        assert!(!pdf_text.contains("[image: figures/tiny.png]"));
    }

    #[test]
    fn unresolved_or_undecodable_display_list_images_keep_pdf_placeholder() {
        let source = SourceProvenance::file("main.tex", 0, 10);
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
                asset_ref: "figures/bad.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: Some("blake3:bad".to_string()),
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: None,
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page.clone()], |asset_ref| {
            (asset_ref == "figures/bad.png").then(|| b"not an image".to_vec())
        });
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg_with_assets(&page, |asset_ref| {
            (asset_ref == "figures/bad.png").then(|| b"not an image".to_vec())
        });

        assert!(pdf_text.contains("[undecodable image: figures/bad.png]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
        assert!(svg.contains("data-image-placeholder-kind=\"undecodable\""));
        assert!(svg.contains("[undecodable image: figures/bad.png]"));
        assert!(!svg.contains("data-image-embedded=\"true\""));
    }

    #[test]
    fn missing_display_list_images_surface_diagnostics_in_pdf_and_svg() {
        let source = SourceProvenance::file("main.tex", 0, 10);
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
                asset_ref: "figures/missing.png".to_string(),
                asset_format: Some(GraphicAssetFormat::Png),
                page_selection: None,
                asset_hash: None,
                natural_width_pt: None,
                natural_height_pt: None,
                crop: None,
                scale: None,
                rotation: None,
                diagnostic: Some("missing graphic asset figures/missing.png".to_string()),
                source,
            })],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page.clone()]);
        let pdf_text = String::from_utf8_lossy(&pdf);
        let svg = render_display_list_svg(&page);

        assert!(pdf_text.contains("[missing image: figures/missing.png]"));
        assert!(svg.contains("data-image-placeholder-kind=\"missing\""));
        assert!(
            svg.contains("data-image-diagnostic=\"missing graphic asset figures/missing.png\"")
        );
        assert!(svg.contains("[missing image: figures/missing.png]"));
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
                target: r"https://example.com/a(1)\b?c=2&d=3".to_string(),
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
        assert!(pdf_text.contains(r"/URI (https://example.com/a\(1\)\\b?c=2&d=3)"));
        assert!(svg.contains(r#"<a href="https://example.com/a(1)\b?c=2&amp;d=3">"#));
        assert!(svg.contains(r#"data-link-target="https://example.com/a(1)\b?c=2&amp;d=3""#));
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
                name: r"sec:intro(1)\more&extra".to_string(),
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
        assert!(pdf_text.contains(r"(sec:intro\(1\)\\more&extra) [16 0 R /XYZ 72 720 null]"));
        assert!(svg.contains(r#"data-destination-name="sec:intro(1)\more&amp;extra""#));
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

    #[test]
    fn renders_display_list_pdf_destination_names_in_stable_order() {
        let source = SourceProvenance::file("main.tex", 0, 10);
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 612.0,
            height_pt: 792.0,
            ops: vec![
                DrawOp::NamedDestination(Destination {
                    name: "sec:zeta".to_string(),
                    point: Point { x: 72.0, y: 72.0 },
                    source: source.clone(),
                }),
                DrawOp::NamedDestination(Destination {
                    name: "sec:alpha".to_string(),
                    point: Point { x: 72.0, y: 96.0 },
                    source,
                }),
            ],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf(&[page]);
        let pdf_text = String::from_utf8_lossy(&pdf);

        let alpha_index = pdf_text
            .find("(sec:alpha) [16 0 R /XYZ 72 696 null]")
            .expect("alpha destination should be present");
        let zeta_index = pdf_text
            .find("(sec:zeta) [16 0 R /XYZ 72 720 null]")
            .expect("zeta destination should be present");
        assert!(alpha_index < zeta_index);
    }
}
