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
    let pdf_opacity_resource_key = |opacity: f32| -> Option<u16> {
        let opacity = opacity.clamp(0.0, 1.0);
        (opacity < 0.999_5).then(|| (opacity * 1000.0).round().clamp(0.0, 1000.0) as u16)
    };
    let pdf_opacity_resource_value = |key: u16| -> String { format!("{}", key as f32 / 1000.0) };
    let push_pdf_paint_opacity =
        |stream: &mut String, opacity_resource_keys: &mut Vec<u16>, opacity: f32| -> bool {
            let Some(key) = pdf_opacity_resource_key(opacity) else {
                return false;
            };
            if !opacity_resource_keys.contains(&key) {
                opacity_resource_keys.push(key);
            }
            stream.push_str(&format!("q /GS{key} gs "));
            true
        };
    let push_pdf_stroke_state = |stream: &mut String,
                                 dasharray: Option<SimpleSvgDashArray>,
                                 style: SimpleSvgStrokeStyle,
                                 scale: f32|
     -> bool {
        let mut operators = Vec::new();
        if let Some(dasharray) = dasharray {
            if !scale.is_finite() || scale <= 0.0 {
                return false;
            }
            let mut values = Vec::new();
            for value in dasharray.values.iter().take(dasharray.len) {
                let value = *value * scale;
                if !value.is_finite() || value < 0.0 {
                    return false;
                }
                values.push(format!("{value}"));
            }
            if values.is_empty() {
                return false;
            }
            let phase = dasharray.offset_ratio * scale;
            if !phase.is_finite() || phase < 0.0 {
                return false;
            }
            operators.push(format!("[{}] {phase} d", values.join(" ")));
        }
        let linecap = match style.linecap {
            SimpleSvgStrokeLineCap::Butt => 0,
            SimpleSvgStrokeLineCap::Round => 1,
            SimpleSvgStrokeLineCap::Square => 2,
        };
        if style.linecap != SimpleSvgStrokeLineCap::Butt {
            operators.push(format!("{linecap} J"));
        }
        let linejoin = match style.linejoin {
            SimpleSvgStrokeLineJoin::Miter => 0,
            SimpleSvgStrokeLineJoin::Round => 1,
            SimpleSvgStrokeLineJoin::Bevel => 2,
        };
        if style.linejoin != SimpleSvgStrokeLineJoin::Miter {
            operators.push(format!("{linejoin} j"));
        }
        if (style.miterlimit - 10.0).abs() > 0.000_5 {
            operators.push(format!("{} M", style.miterlimit));
        }
        if operators.is_empty() {
            return false;
        }
        stream.push_str(&format!("q {} ", operators.join(" ")));
        true
    };
    let pdf_fill_operator = |rule: SimpleSvgFillRule| -> &'static str {
        match rule {
            SimpleSvgFillRule::NonZero => "f",
            SimpleSvgFillRule::EvenOdd => "f*",
        }
    };
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
        let mut opacity_resource_keys = Vec::new();
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
                            && (!svg.rects.is_empty()
                                || !svg.lines.is_empty()
                                || !svg.ellipses.is_empty()
                                || !svg.polys.is_empty()
                                || !svg.paths.is_empty()
                                || !svg.texts.is_empty())
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
                            let svg_clip_to_viewport =
                                svg.preserve_aspect_ratio.scale == SimpleSvgAspectScale::Slice;
                            if draw.clip_to_dest || svg_clip_to_viewport {
                                let clip_rect = if draw.clip_to_dest {
                                    Rect {
                                        x: dest_x,
                                        y: dest_y,
                                        width: image.rect.width,
                                        height: image.rect.height,
                                    }
                                } else {
                                    draw.rect
                                };
                                stream.push_str(&format!(
                                    "q {} {} {} {} re W n q ",
                                    clip_rect.x, clip_rect.y, clip_rect.width, clip_rect.height
                                ));
                            } else {
                                stream.push_str("q ");
                            }
                            let mut svg_rect = draw.rect;
                            if svg.preserve_aspect_ratio.scale != SimpleSvgAspectScale::None
                                && svg.view_box_aspect_ratio.is_finite()
                                && svg.view_box_aspect_ratio > 0.0
                                && draw.rect.width.is_finite()
                                && draw.rect.height.is_finite()
                                && draw.rect.width > 0.0
                                && draw.rect.height > 0.0
                            {
                                let viewport_aspect = draw.rect.width / draw.rect.height;
                                let fit_width = match svg.preserve_aspect_ratio.scale {
                                    SimpleSvgAspectScale::None => draw.rect.width,
                                    SimpleSvgAspectScale::Meet => {
                                        if viewport_aspect > svg.view_box_aspect_ratio {
                                            draw.rect.height * svg.view_box_aspect_ratio
                                        } else {
                                            draw.rect.width
                                        }
                                    }
                                    SimpleSvgAspectScale::Slice => {
                                        if viewport_aspect > svg.view_box_aspect_ratio {
                                            draw.rect.width
                                        } else {
                                            draw.rect.height * svg.view_box_aspect_ratio
                                        }
                                    }
                                };
                                let fit_height = match svg.preserve_aspect_ratio.scale {
                                    SimpleSvgAspectScale::None => draw.rect.height,
                                    SimpleSvgAspectScale::Meet => {
                                        if viewport_aspect > svg.view_box_aspect_ratio {
                                            draw.rect.height
                                        } else {
                                            draw.rect.width / svg.view_box_aspect_ratio
                                        }
                                    }
                                    SimpleSvgAspectScale::Slice => {
                                        if viewport_aspect > svg.view_box_aspect_ratio {
                                            draw.rect.width / svg.view_box_aspect_ratio
                                        } else {
                                            draw.rect.height
                                        }
                                    }
                                };
                                if fit_width.is_finite()
                                    && fit_height.is_finite()
                                    && fit_width > 0.0
                                    && fit_height > 0.0
                                {
                                    let remaining_x = draw.rect.width - fit_width;
                                    let remaining_y = draw.rect.height - fit_height;
                                    let offset_x = match svg.preserve_aspect_ratio.x_align {
                                        SimpleSvgAspectAlign::Min => 0.0,
                                        SimpleSvgAspectAlign::Mid => remaining_x / 2.0,
                                        SimpleSvgAspectAlign::Max => remaining_x,
                                    };
                                    let offset_y = match svg.preserve_aspect_ratio.y_align {
                                        SimpleSvgAspectAlign::Min => remaining_y,
                                        SimpleSvgAspectAlign::Mid => remaining_y / 2.0,
                                        SimpleSvgAspectAlign::Max => 0.0,
                                    };
                                    svg_rect = Rect {
                                        x: draw.rect.x + offset_x,
                                        y: draw.rect.y + offset_y,
                                        width: fit_width,
                                        height: fit_height,
                                    };
                                }
                            }
                            let svg_width = svg_rect.width;
                            let svg_height = svg_rect.height;
                            if svg_width.is_finite()
                                && svg_height.is_finite()
                                && svg_width > 0.0
                                && svg_height > 0.0
                            {
                                for rect in svg.rects {
                                    let rect_x = svg_rect.x + rect.x_ratio * svg_width;
                                    let rect_y = svg_rect.y
                                        + (1.0 - rect.y_ratio - rect.height_ratio) * svg_height;
                                    let rect_width = rect.width_ratio * svg_width;
                                    let rect_height = rect.height_ratio * svg_height;
                                    if rect_width.is_finite()
                                        && rect_height.is_finite()
                                        && rect_width > 0.0
                                        && rect_height > 0.0
                                    {
                                        if let Some(fill) = rect.fill {
                                            let fill_operator = pdf_fill_operator(rect.fill_rule);
                                            let scoped_opacity = push_pdf_paint_opacity(
                                                &mut stream,
                                                &mut opacity_resource_keys,
                                                fill.opacity,
                                            );
                                            stream.push_str(&format!(
                                                "{} {} {} rg {} {} {} {} re {} ",
                                                fill.rgb.0,
                                                fill.rgb.1,
                                                fill.rgb.2,
                                                rect_x,
                                                rect_y,
                                                rect_width,
                                                rect_height,
                                                fill_operator
                                            ));
                                            if scoped_opacity {
                                                stream.push_str("Q ");
                                            }
                                        }
                                        if let Some(stroke) = rect.stroke {
                                            let stroke_width = rect.stroke_width_ratio * svg_width;
                                            if stroke_width.is_finite() && stroke_width > 0.0 {
                                                let scoped_stroke_state = push_pdf_stroke_state(
                                                    &mut stream,
                                                    rect.stroke_dasharray,
                                                    rect.stroke_style,
                                                    svg_width,
                                                );
                                                let scoped_opacity = push_pdf_paint_opacity(
                                                    &mut stream,
                                                    &mut opacity_resource_keys,
                                                    stroke.opacity,
                                                );
                                                stream.push_str(&format!(
                                                    "{} {} {} RG {} w {} {} {} {} re S ",
                                                    stroke.rgb.0,
                                                    stroke.rgb.1,
                                                    stroke.rgb.2,
                                                    stroke_width,
                                                    rect_x,
                                                    rect_y,
                                                    rect_width,
                                                    rect_height
                                                ));
                                                if scoped_opacity {
                                                    stream.push_str("Q ");
                                                }
                                                if scoped_stroke_state {
                                                    stream.push_str("Q ");
                                                }
                                            }
                                        }
                                    }
                                }
                                for line in svg.lines {
                                    let x1 = svg_rect.x + line.x1_ratio * svg_width;
                                    let y1 = svg_rect.y + (1.0 - line.y1_ratio) * svg_height;
                                    let x2 = svg_rect.x + line.x2_ratio * svg_width;
                                    let y2 = svg_rect.y + (1.0 - line.y2_ratio) * svg_height;
                                    let stroke_width = line.stroke_width_ratio * svg_width;
                                    if x1.is_finite()
                                        && y1.is_finite()
                                        && x2.is_finite()
                                        && y2.is_finite()
                                        && stroke_width.is_finite()
                                        && stroke_width > 0.0
                                    {
                                        let scoped_stroke_state = push_pdf_stroke_state(
                                            &mut stream,
                                            line.stroke_dasharray,
                                            line.stroke_style,
                                            svg_width,
                                        );
                                        let scoped_opacity = push_pdf_paint_opacity(
                                            &mut stream,
                                            &mut opacity_resource_keys,
                                            line.stroke.opacity,
                                        );
                                        stream.push_str(&format!(
                                            "{} {} {} RG {} w {} {} m {} {} l S ",
                                            line.stroke.rgb.0,
                                            line.stroke.rgb.1,
                                            line.stroke.rgb.2,
                                            stroke_width,
                                            x1,
                                            y1,
                                            x2,
                                            y2
                                        ));
                                        if scoped_opacity {
                                            stream.push_str("Q ");
                                        }
                                        if scoped_stroke_state {
                                            stream.push_str("Q ");
                                        }
                                    }
                                }
                                for ellipse in svg.ellipses {
                                    let center_x = svg_rect.x + ellipse.cx_ratio * svg_width;
                                    let center_y =
                                        svg_rect.y + (1.0 - ellipse.cy_ratio) * svg_height;
                                    let radius_x = ellipse.rx_ratio * svg_width;
                                    let radius_y = ellipse.ry_ratio * svg_height;
                                    if center_x.is_finite()
                                        && center_y.is_finite()
                                        && radius_x.is_finite()
                                        && radius_y.is_finite()
                                        && radius_x > 0.0
                                        && radius_y > 0.0
                                    {
                                        let kappa = 0.552_284_8_f32;
                                        let path = format!(
                                            "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h ",
                                            center_x + radius_x,
                                            center_y,
                                            center_x + radius_x,
                                            center_y + kappa * radius_y,
                                            center_x + kappa * radius_x,
                                            center_y + radius_y,
                                            center_x,
                                            center_y + radius_y,
                                            center_x - kappa * radius_x,
                                            center_y + radius_y,
                                            center_x - radius_x,
                                            center_y + kappa * radius_y,
                                            center_x - radius_x,
                                            center_y,
                                            center_x - radius_x,
                                            center_y - kappa * radius_y,
                                            center_x - kappa * radius_x,
                                            center_y - radius_y,
                                            center_x,
                                            center_y - radius_y,
                                            center_x + kappa * radius_x,
                                            center_y - radius_y,
                                            center_x + radius_x,
                                            center_y - kappa * radius_y,
                                            center_x + radius_x,
                                            center_y
                                        );
                                        if let Some(fill) = ellipse.fill {
                                            let fill_operator =
                                                pdf_fill_operator(ellipse.fill_rule);
                                            let scoped_opacity = push_pdf_paint_opacity(
                                                &mut stream,
                                                &mut opacity_resource_keys,
                                                fill.opacity,
                                            );
                                            stream.push_str(&format!(
                                                "{} {} {} rg {}{} ",
                                                fill.rgb.0,
                                                fill.rgb.1,
                                                fill.rgb.2,
                                                path,
                                                fill_operator
                                            ));
                                            if scoped_opacity {
                                                stream.push_str("Q ");
                                            }
                                        }
                                        if let Some(stroke) = ellipse.stroke {
                                            let stroke_width =
                                                ellipse.stroke_width_ratio * svg_width;
                                            if stroke_width.is_finite() && stroke_width > 0.0 {
                                                let scoped_stroke_state = push_pdf_stroke_state(
                                                    &mut stream,
                                                    ellipse.stroke_dasharray,
                                                    ellipse.stroke_style,
                                                    svg_width,
                                                );
                                                let scoped_opacity = push_pdf_paint_opacity(
                                                    &mut stream,
                                                    &mut opacity_resource_keys,
                                                    stroke.opacity,
                                                );
                                                stream.push_str(&format!(
                                                    "{} {} {} RG {} w {}S ",
                                                    stroke.rgb.0,
                                                    stroke.rgb.1,
                                                    stroke.rgb.2,
                                                    stroke_width,
                                                    path
                                                ));
                                                if scoped_opacity {
                                                    stream.push_str("Q ");
                                                }
                                                if scoped_stroke_state {
                                                    stream.push_str("Q ");
                                                }
                                            }
                                        }
                                    }
                                }
                                for poly in &svg.polys {
                                    if poly.points.len() < 2 {
                                        continue;
                                    }
                                    let mut path = String::new();
                                    let mut points = poly.points.iter();
                                    let Some(first) = points.next() else {
                                        continue;
                                    };
                                    let first_x = svg_rect.x + first.0 * svg_width;
                                    let first_y = svg_rect.y + (1.0 - first.1) * svg_height;
                                    if !first_x.is_finite() || !first_y.is_finite() {
                                        continue;
                                    }
                                    path.push_str(&format!("{first_x} {first_y} m "));
                                    let mut valid = true;
                                    for point in points {
                                        let x = svg_rect.x + point.0 * svg_width;
                                        let y = svg_rect.y + (1.0 - point.1) * svg_height;
                                        if !x.is_finite() || !y.is_finite() {
                                            valid = false;
                                            break;
                                        }
                                        path.push_str(&format!("{x} {y} l "));
                                    }
                                    if !valid {
                                        continue;
                                    }
                                    if poly.closed {
                                        path.push_str("h ");
                                    }
                                    if let Some(fill) = poly.fill {
                                        let fill_operator = pdf_fill_operator(poly.fill_rule);
                                        let scoped_opacity = push_pdf_paint_opacity(
                                            &mut stream,
                                            &mut opacity_resource_keys,
                                            fill.opacity,
                                        );
                                        stream.push_str(&format!(
                                            "{} {} {} rg {}{} ",
                                            fill.rgb.0, fill.rgb.1, fill.rgb.2, path, fill_operator
                                        ));
                                        if scoped_opacity {
                                            stream.push_str("Q ");
                                        }
                                    }
                                    if let Some(stroke) = poly.stroke {
                                        let stroke_width = poly.stroke_width_ratio * svg_width;
                                        if stroke_width.is_finite() && stroke_width > 0.0 {
                                            let scoped_stroke_state = push_pdf_stroke_state(
                                                &mut stream,
                                                poly.stroke_dasharray,
                                                poly.stroke_style,
                                                svg_width,
                                            );
                                            let scoped_opacity = push_pdf_paint_opacity(
                                                &mut stream,
                                                &mut opacity_resource_keys,
                                                stroke.opacity,
                                            );
                                            stream.push_str(&format!(
                                                "{} {} {} RG {} w {}S ",
                                                stroke.rgb.0,
                                                stroke.rgb.1,
                                                stroke.rgb.2,
                                                stroke_width,
                                                path
                                            ));
                                            if scoped_opacity {
                                                stream.push_str("Q ");
                                            }
                                            if scoped_stroke_state {
                                                stream.push_str("Q ");
                                            }
                                        }
                                    }
                                }
                                for svg_path in &svg.paths {
                                    if svg_path.ops.len() < 2 {
                                        continue;
                                    }
                                    let mut path = String::new();
                                    let mut valid = true;
                                    for op in &svg_path.ops {
                                        match op {
                                            SimpleSvgPathOp::MoveTo(point) => {
                                                let x = svg_rect.x + point.0 * svg_width;
                                                let y = svg_rect.y + (1.0 - point.1) * svg_height;
                                                if !x.is_finite() || !y.is_finite() {
                                                    valid = false;
                                                    break;
                                                }
                                                path.push_str(&format!("{x} {y} m "));
                                            }
                                            SimpleSvgPathOp::LineTo(point) => {
                                                let x = svg_rect.x + point.0 * svg_width;
                                                let y = svg_rect.y + (1.0 - point.1) * svg_height;
                                                if !x.is_finite() || !y.is_finite() {
                                                    valid = false;
                                                    break;
                                                }
                                                path.push_str(&format!("{x} {y} l "));
                                            }
                                            SimpleSvgPathOp::CubicTo { ctrl1, ctrl2, to } => {
                                                let ctrl1_x = svg_rect.x + ctrl1.0 * svg_width;
                                                let ctrl1_y =
                                                    svg_rect.y + (1.0 - ctrl1.1) * svg_height;
                                                let ctrl2_x = svg_rect.x + ctrl2.0 * svg_width;
                                                let ctrl2_y =
                                                    svg_rect.y + (1.0 - ctrl2.1) * svg_height;
                                                let to_x = svg_rect.x + to.0 * svg_width;
                                                let to_y = svg_rect.y + (1.0 - to.1) * svg_height;
                                                if !ctrl1_x.is_finite()
                                                    || !ctrl1_y.is_finite()
                                                    || !ctrl2_x.is_finite()
                                                    || !ctrl2_y.is_finite()
                                                    || !to_x.is_finite()
                                                    || !to_y.is_finite()
                                                {
                                                    valid = false;
                                                    break;
                                                }
                                                path.push_str(&format!(
                                                    "{ctrl1_x} {ctrl1_y} {ctrl2_x} {ctrl2_y} {to_x} {to_y} c "
                                                ));
                                            }
                                            SimpleSvgPathOp::Close => {
                                                path.push_str("h ");
                                            }
                                        }
                                    }
                                    if !valid || path.is_empty() {
                                        continue;
                                    }
                                    if let Some(fill) = svg_path.fill {
                                        let fill_operator = pdf_fill_operator(svg_path.fill_rule);
                                        let scoped_opacity = push_pdf_paint_opacity(
                                            &mut stream,
                                            &mut opacity_resource_keys,
                                            fill.opacity,
                                        );
                                        stream.push_str(&format!(
                                            "{} {} {} rg {}{} ",
                                            fill.rgb.0, fill.rgb.1, fill.rgb.2, path, fill_operator
                                        ));
                                        if scoped_opacity {
                                            stream.push_str("Q ");
                                        }
                                    }
                                    if let Some(stroke) = svg_path.stroke {
                                        let stroke_width = svg_path.stroke_width_ratio * svg_width;
                                        if stroke_width.is_finite() && stroke_width > 0.0 {
                                            let scoped_stroke_state = push_pdf_stroke_state(
                                                &mut stream,
                                                svg_path.stroke_dasharray,
                                                svg_path.stroke_style,
                                                svg_width,
                                            );
                                            let scoped_opacity = push_pdf_paint_opacity(
                                                &mut stream,
                                                &mut opacity_resource_keys,
                                                stroke.opacity,
                                            );
                                            stream.push_str(&format!(
                                                "{} {} {} RG {} w {}S ",
                                                stroke.rgb.0,
                                                stroke.rgb.1,
                                                stroke.rgb.2,
                                                stroke_width,
                                                path
                                            ));
                                            if scoped_opacity {
                                                stream.push_str("Q ");
                                            }
                                            if scoped_stroke_state {
                                                stream.push_str("Q ");
                                            }
                                        }
                                    }
                                }
                                for svg_text in &svg.texts {
                                    let Some(fill) = svg_text.fill else {
                                        continue;
                                    };
                                    let mut x = svg_rect.x + svg_text.x_ratio * svg_width;
                                    let y = svg_rect.y + (1.0 - svg_text.y_ratio) * svg_height;
                                    let font_size = svg_text.font_size_ratio * svg_height;
                                    if !x.is_finite()
                                        || !y.is_finite()
                                        || !font_size.is_finite()
                                        || font_size <= 0.0
                                        || svg_text.text.is_empty()
                                    {
                                        continue;
                                    }
                                    let estimated_advance = svg_text
                                        .text
                                        .chars()
                                        .map(|ch| {
                                            if ch.is_whitespace() {
                                                0.33
                                            } else if ch.is_ascii_punctuation() {
                                                0.33
                                            } else {
                                                0.5
                                            }
                                        })
                                        .sum::<f32>()
                                        * font_size;
                                    match svg_text.anchor {
                                        SimpleSvgTextAnchor::Start => {}
                                        SimpleSvgTextAnchor::Middle => {
                                            x -= estimated_advance / 2.0;
                                        }
                                        SimpleSvgTextAnchor::End => {
                                            x -= estimated_advance;
                                        }
                                    }
                                    if !x.is_finite() {
                                        continue;
                                    }
                                    let scoped_opacity = push_pdf_paint_opacity(
                                        &mut stream,
                                        &mut opacity_resource_keys,
                                        fill.opacity,
                                    );
                                    let font_resource = match (
                                        svg_text.font_family,
                                        svg_text.font_series,
                                        svg_text.font_shape,
                                    ) {
                                        (
                                            SimpleSvgFontFamily::Serif,
                                            FontSeries::Regular,
                                            FontShape::Upright,
                                        ) => "F1",
                                        (
                                            SimpleSvgFontFamily::Serif,
                                            FontSeries::Bold,
                                            FontShape::Upright,
                                        ) => "F2",
                                        (
                                            SimpleSvgFontFamily::Serif,
                                            FontSeries::Regular,
                                            FontShape::Italic,
                                        ) => "F3",
                                        (
                                            SimpleSvgFontFamily::Serif,
                                            FontSeries::Bold,
                                            FontShape::Italic,
                                        ) => "F4",
                                        (
                                            SimpleSvgFontFamily::Sans,
                                            FontSeries::Regular,
                                            FontShape::Upright,
                                        ) => "F5",
                                        (
                                            SimpleSvgFontFamily::Sans,
                                            FontSeries::Bold,
                                            FontShape::Upright,
                                        ) => "F6",
                                        (
                                            SimpleSvgFontFamily::Sans,
                                            FontSeries::Regular,
                                            FontShape::Italic,
                                        ) => "F7",
                                        (
                                            SimpleSvgFontFamily::Sans,
                                            FontSeries::Bold,
                                            FontShape::Italic,
                                        ) => "F8",
                                        (
                                            SimpleSvgFontFamily::Mono,
                                            FontSeries::Regular,
                                            FontShape::Upright,
                                        ) => "F9",
                                        (
                                            SimpleSvgFontFamily::Mono,
                                            FontSeries::Bold,
                                            FontShape::Upright,
                                        ) => "F10",
                                        (
                                            SimpleSvgFontFamily::Mono,
                                            FontSeries::Regular,
                                            FontShape::Italic,
                                        ) => "F11",
                                        (
                                            SimpleSvgFontFamily::Mono,
                                            FontSeries::Bold,
                                            FontShape::Italic,
                                        ) => "F12",
                                    };
                                    stream.push_str(&format!(
                                        "{} {} {} rg BT /{} {} Tf 1 0 0 1 {} {} Tm (",
                                        fill.rgb.0,
                                        fill.rgb.1,
                                        fill.rgb.2,
                                        font_resource,
                                        font_size,
                                        x,
                                        y
                                    ));
                                    stream.push_str(&escape_pdf_text(&svg_text.text));
                                    stream.push_str(") Tj ET ");
                                    if scoped_opacity {
                                        stream.push_str("Q ");
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
        opacity_resource_keys.sort_unstable();
        opacity_resource_keys.dedup();
        let ext_gstates = if opacity_resource_keys.is_empty() {
            String::new()
        } else {
            format!(
                " /ExtGState << {} >>",
                opacity_resource_keys
                    .iter()
                    .map(|key| {
                        let value = pdf_opacity_resource_value(*key);
                        format!("/GS{key} << /Type /ExtGState /ca {value} /CA {value} >>")
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        objects.push(format!(
            "{page_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {} >>{}{} >> /Contents {content_id} 0 R{} >> endobj\n",
            page.width_pt,
            page.height_pt,
            font_resources,
            xobjects,
            ext_gstates,
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
    view_box_aspect_ratio: f32,
    preserve_aspect_ratio: SimpleSvgPreserveAspectRatio,
    rects: Vec<SimpleSvgRect>,
    lines: Vec<SimpleSvgLine>,
    ellipses: Vec<SimpleSvgEllipse>,
    polys: Vec<SimpleSvgPoly>,
    paths: Vec<SimpleSvgPath>,
    texts: Vec<SimpleSvgText>,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgPreserveAspectRatio {
    x_align: SimpleSvgAspectAlign,
    y_align: SimpleSvgAspectAlign,
    scale: SimpleSvgAspectScale,
}

#[derive(Debug, Clone, Copy)]
enum SimpleSvgAspectAlign {
    Min,
    Mid,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgAspectScale {
    None,
    Meet,
    Slice,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgPaint {
    rgb: (f32, f32, f32),
    opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgDashArray {
    values: [f32; 8],
    len: usize,
    offset_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgStrokeLineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgStrokeLineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgStrokeStyle {
    linecap: SimpleSvgStrokeLineCap,
    linejoin: SimpleSvgStrokeLineJoin,
    miterlimit: f32,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgRect {
    x_ratio: f32,
    y_ratio: f32,
    width_ratio: f32,
    height_ratio: f32,
    fill: Option<SimpleSvgPaint>,
    fill_rule: SimpleSvgFillRule,
    stroke: Option<SimpleSvgPaint>,
    stroke_width_ratio: f32,
    stroke_dasharray: Option<SimpleSvgDashArray>,
    stroke_style: SimpleSvgStrokeStyle,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgLine {
    x1_ratio: f32,
    y1_ratio: f32,
    x2_ratio: f32,
    y2_ratio: f32,
    stroke: SimpleSvgPaint,
    stroke_width_ratio: f32,
    stroke_dasharray: Option<SimpleSvgDashArray>,
    stroke_style: SimpleSvgStrokeStyle,
}

#[derive(Debug, Clone, Copy)]
struct SimpleSvgEllipse {
    cx_ratio: f32,
    cy_ratio: f32,
    rx_ratio: f32,
    ry_ratio: f32,
    fill: Option<SimpleSvgPaint>,
    fill_rule: SimpleSvgFillRule,
    stroke: Option<SimpleSvgPaint>,
    stroke_width_ratio: f32,
    stroke_dasharray: Option<SimpleSvgDashArray>,
    stroke_style: SimpleSvgStrokeStyle,
}

#[derive(Debug, Clone)]
struct SimpleSvgPoly {
    points: Vec<(f32, f32)>,
    closed: bool,
    fill: Option<SimpleSvgPaint>,
    fill_rule: SimpleSvgFillRule,
    stroke: Option<SimpleSvgPaint>,
    stroke_width_ratio: f32,
    stroke_dasharray: Option<SimpleSvgDashArray>,
    stroke_style: SimpleSvgStrokeStyle,
}

#[derive(Debug, Clone)]
struct SimpleSvgPath {
    ops: Vec<SimpleSvgPathOp>,
    fill: Option<SimpleSvgPaint>,
    fill_rule: SimpleSvgFillRule,
    stroke: Option<SimpleSvgPaint>,
    stroke_width_ratio: f32,
    stroke_dasharray: Option<SimpleSvgDashArray>,
    stroke_style: SimpleSvgStrokeStyle,
}

#[derive(Debug, Clone)]
struct SimpleSvgText {
    x_ratio: f32,
    y_ratio: f32,
    font_size_ratio: f32,
    anchor: SimpleSvgTextAnchor,
    font_family: SimpleSvgFontFamily,
    font_series: FontSeries,
    font_shape: FontShape,
    fill: Option<SimpleSvgPaint>,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgTextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgFontFamily {
    Serif,
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleSvgTextBaseline {
    Alphabetic,
    Middle,
}

#[derive(Debug, Clone, Copy)]
enum SimpleSvgPathOp {
    MoveTo((f32, f32)),
    LineTo((f32, f32)),
    CubicTo {
        ctrl1: (f32, f32),
        ctrl2: (f32, f32),
        to: (f32, f32),
    },
    Close,
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
    let is_start_tag_named = |tag_tail: &str, element_name: &str| {
        let Some(after_lt) = tag_tail.strip_prefix('<') else {
            return false;
        };
        let Some(after_name) = after_lt.strip_prefix(element_name) else {
            return false;
        };
        after_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '>' | '/'))
    };
    let mut svg_search_offset = 0usize;
    let (tag_start, tag_end) = loop {
        let relative = text[svg_search_offset..].find("<svg")?;
        let tag_start = svg_search_offset + relative;
        let tag_tail = &text[tag_start..];
        if !is_start_tag_named(tag_tail, "svg") {
            svg_search_offset = tag_start + "<svg".len();
            continue;
        }
        let tag_end = tag_tail.find('>')?;
        break (tag_start, tag_end);
    };
    let tag_tail = &text[tag_start..];
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
            let Some(after) = after.strip_prefix('=') else {
                offset = index + name.len();
                continue;
            };
            let after = after.trim_start();
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
    let preserve_aspect_ratio = attr_value(svg_tag, "preserveAspectRatio")
        .as_deref()
        .and_then(|raw| {
            let parts = raw
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let align_index = usize::from(
                parts
                    .first()
                    .is_some_and(|part| part.eq_ignore_ascii_case("defer")),
            );
            let align = parts.get(align_index)?;
            if align.eq_ignore_ascii_case("none") {
                return Some(SimpleSvgPreserveAspectRatio {
                    x_align: SimpleSvgAspectAlign::Mid,
                    y_align: SimpleSvgAspectAlign::Mid,
                    scale: SimpleSvgAspectScale::None,
                });
            }
            let (x_align, y_align) = match *align {
                "xMinYMin" => (SimpleSvgAspectAlign::Min, SimpleSvgAspectAlign::Min),
                "xMidYMin" => (SimpleSvgAspectAlign::Mid, SimpleSvgAspectAlign::Min),
                "xMaxYMin" => (SimpleSvgAspectAlign::Max, SimpleSvgAspectAlign::Min),
                "xMinYMid" => (SimpleSvgAspectAlign::Min, SimpleSvgAspectAlign::Mid),
                "xMidYMid" => (SimpleSvgAspectAlign::Mid, SimpleSvgAspectAlign::Mid),
                "xMaxYMid" => (SimpleSvgAspectAlign::Max, SimpleSvgAspectAlign::Mid),
                "xMinYMax" => (SimpleSvgAspectAlign::Min, SimpleSvgAspectAlign::Max),
                "xMidYMax" => (SimpleSvgAspectAlign::Mid, SimpleSvgAspectAlign::Max),
                "xMaxYMax" => (SimpleSvgAspectAlign::Max, SimpleSvgAspectAlign::Max),
                _ => return None,
            };
            let scale = match parts.get(align_index + 1).copied().unwrap_or("meet") {
                "meet" => SimpleSvgAspectScale::Meet,
                "slice" => SimpleSvgAspectScale::Slice,
                _ => return None,
            };
            Some(SimpleSvgPreserveAspectRatio {
                x_align,
                y_align,
                scale,
            })
        })
        .unwrap_or(SimpleSvgPreserveAspectRatio {
            x_align: SimpleSvgAspectAlign::Mid,
            y_align: SimpleSvgAspectAlign::Mid,
            scale: SimpleSvgAspectScale::Meet,
        });
    let declaration_value = |declarations: &str, name: &str| -> Option<String> {
        for declaration in declarations.split(';') {
            let Some((key, value)) = declaration.split_once(':') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case(name) {
                return Some(value.trim().to_string());
            }
        }
        None
    };
    let style_value = |tag: &str, name: &str| -> Option<String> {
        let style = attr_value(tag, "style")?;
        declaration_value(&style, name)
    };
    let parse_color = |raw: &str| -> Option<SimpleSvgResolvedColor> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("transparent") {
            return None;
        }
        let color = |rgb: (f32, f32, f32)| SimpleSvgResolvedColor { rgb, alpha: 1.0 };
        let parse_rgb_component = |component: &str| -> Option<f32> {
            let component = component.trim();
            if let Some(percent) = component.strip_suffix('%') {
                return percent
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| (value / 100.0).clamp(0.0, 1.0));
            }
            component
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| (value / 255.0).clamp(0.0, 1.0))
        };
        let parse_alpha_component = |component: &str| -> Option<f32> {
            let component = component.trim();
            if let Some(percent) = component.strip_suffix('%') {
                return percent
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| (value / 100.0).clamp(0.0, 1.0));
            }
            component
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| value.clamp(0.0, 1.0))
        };
        if raw.ends_with(')') {
            let (body, is_rgba) = if raw.len() >= 5 && raw[..4].eq_ignore_ascii_case("rgb(") {
                (&raw[4..raw.len() - 1], false)
            } else if raw.len() >= 6 && raw[..5].eq_ignore_ascii_case("rgba(") {
                (&raw[5..raw.len() - 1], true)
            } else {
                ("", false)
            };
            let (body, slash_alpha) = body
                .split_once('/')
                .map(|(body, alpha)| (body, Some(alpha)))
                .unwrap_or((body, None));
            let components = body
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .filter(|component| !component.is_empty())
                .collect::<Vec<_>>();
            if components.len() >= 3 {
                let comma_alpha = if is_rgba && components.len() >= 4 {
                    Some(components[3])
                } else {
                    None
                };
                let alpha = if let Some(alpha) = slash_alpha.or(comma_alpha) {
                    parse_alpha_component(alpha)?
                } else {
                    1.0
                };
                return Some(SimpleSvgResolvedColor {
                    rgb: (
                        parse_rgb_component(components[0])?,
                        parse_rgb_component(components[1])?,
                        parse_rgb_component(components[2])?,
                    ),
                    alpha,
                });
            }
        }
        if let Some(hex) = raw.strip_prefix('#') {
            if hex.len() == 6 || hex.len() == 8 {
                return Some(SimpleSvgResolvedColor {
                    rgb: (
                        u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0,
                        u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0,
                        u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0,
                    ),
                    alpha: if hex.len() == 8 {
                        u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0
                    } else {
                        1.0
                    },
                });
            }
            if hex.len() == 3 || hex.len() == 4 {
                let expand = |component: &str| -> Option<f32> {
                    Some(u8::from_str_radix(&component.repeat(2), 16).ok()? as f32 / 255.0)
                };
                return Some(SimpleSvgResolvedColor {
                    rgb: (
                        expand(&hex[0..1])?,
                        expand(&hex[1..2])?,
                        expand(&hex[2..3])?,
                    ),
                    alpha: if hex.len() == 4 {
                        expand(&hex[3..4])?
                    } else {
                        1.0
                    },
                });
            }
        }
        match raw.to_ascii_lowercase().as_str() {
            "black" => Some(color((0.0, 0.0, 0.0))),
            "silver" => Some(color((0.75, 0.75, 0.75))),
            "white" => Some(color((1.0, 1.0, 1.0))),
            "gray" | "grey" => Some(color((0.5, 0.5, 0.5))),
            "maroon" => Some(color((0.5, 0.0, 0.0))),
            "red" => Some(color((1.0, 0.0, 0.0))),
            "purple" => Some(color((0.5, 0.0, 0.5))),
            "fuchsia" | "magenta" => Some(color((1.0, 0.0, 1.0))),
            "green" => Some(color((0.0, 0.5, 0.0))),
            "lime" => Some(color((0.0, 1.0, 0.0))),
            "olive" => Some(color((0.5, 0.5, 0.0))),
            "yellow" => Some(color((1.0, 1.0, 0.0))),
            "navy" => Some(color((0.0, 0.0, 0.5))),
            "blue" => Some(color((0.0, 0.0, 1.0))),
            "teal" => Some(color((0.0, 0.5, 0.5))),
            "aqua" | "cyan" => Some(color((0.0, 1.0, 1.0))),
            _ => Some(color((0.0, 0.0, 0.0))),
        }
    };
    let parse_paint = |raw: &str| -> Option<SimpleSvgColor> {
        let raw = raw.trim();
        if raw.len() >= 4 && raw[..4].eq_ignore_ascii_case("url(") {
            let Some(url_end) = raw.find(')') else {
                return Some(SimpleSvgColor::Resolved(SimpleSvgResolvedColor::opaque((
                    0.0, 0.0, 0.0,
                ))));
            };
            let fallback = raw[url_end + 1..].trim();
            if fallback.is_empty() {
                return Some(SimpleSvgColor::Resolved(SimpleSvgResolvedColor::opaque((
                    0.0, 0.0, 0.0,
                ))));
            }
            if fallback.eq_ignore_ascii_case("none") || fallback.eq_ignore_ascii_case("transparent")
            {
                return None;
            }
            if fallback.eq_ignore_ascii_case("currentColor") {
                return Some(SimpleSvgColor::CurrentColor);
            }
            return parse_color(fallback).map(SimpleSvgColor::Resolved);
        }
        if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("transparent") {
            return None;
        }
        if raw.eq_ignore_ascii_case("currentColor") {
            return Some(SimpleSvgColor::CurrentColor);
        }
        parse_color(raw).map(SimpleSvgColor::Resolved)
    };
    #[derive(Debug, Clone, Copy)]
    struct SimpleSvgResolvedColor {
        rgb: (f32, f32, f32),
        alpha: f32,
    }
    impl SimpleSvgResolvedColor {
        fn opaque(rgb: (f32, f32, f32)) -> Self {
            Self { rgb, alpha: 1.0 }
        }
    }
    #[derive(Debug, Clone, Copy)]
    enum SimpleSvgColor {
        Resolved(SimpleSvgResolvedColor),
        CurrentColor,
    }
    #[derive(Debug, Clone, Copy)]
    enum SimpleSvgFontSize {
        Absolute(f32),
        Percent(f32),
    }
    #[derive(Debug, Clone, Copy)]
    enum SimpleSvgBaselineShift {
        Offset(f32),
        Percent(f32),
        Super,
        Sub,
    }
    #[derive(Debug, Clone, Copy, Default)]
    struct SimpleSvgPresentation {
        // Outer Option means "specified"; inner Option preserves SVG paint "none".
        fill: Option<Option<SimpleSvgColor>>,
        fill_rule: Option<SimpleSvgFillRule>,
        stroke: Option<Option<SimpleSvgColor>>,
        stroke_width: Option<f32>,
        stroke_dasharray: Option<Option<SimpleSvgDashArray>>,
        stroke_dashoffset: Option<f32>,
        stroke_linecap: Option<SimpleSvgStrokeLineCap>,
        stroke_linejoin: Option<SimpleSvgStrokeLineJoin>,
        stroke_miterlimit: Option<f32>,
        color: Option<SimpleSvgResolvedColor>,
        opacity: Option<f32>,
        fill_opacity: Option<f32>,
        stroke_opacity: Option<f32>,
        text_anchor: Option<SimpleSvgTextAnchor>,
        font_size: Option<SimpleSvgFontSize>,
        font_family: Option<SimpleSvgFontFamily>,
        font_series: Option<FontSeries>,
        font_shape: Option<FontShape>,
        text_baseline: Option<SimpleSvgTextBaseline>,
        baseline_shift: Option<SimpleSvgBaselineShift>,
    }
    let parse_opacity = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if let Some(percent) = raw.strip_suffix('%') {
            return percent
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| (value / 100.0).clamp(0.0, 1.0));
        }
        raw.parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 1.0))
    };
    let parse_dasharray = |raw: &str| -> Option<Option<SimpleSvgDashArray>> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("none") {
            return Some(None);
        }
        let mut values = [0.0_f32; 8];
        let mut len = 0usize;
        let mut has_positive_value = false;
        for component in raw
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|component| !component.is_empty())
        {
            let value = parse_number_prefix(component)?;
            if !value.is_finite() || value < 0.0 {
                return None;
            }
            has_positive_value |= value > 0.0;
            if len < values.len() {
                values[len] = value;
                len += 1;
            }
        }
        (len > 0 && has_positive_value).then_some(Some(SimpleSvgDashArray {
            values,
            len,
            offset_ratio: 0.0,
        }))
    };
    let parse_stroke_linecap = |raw: &str| -> Option<SimpleSvgStrokeLineCap> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "butt" => Some(SimpleSvgStrokeLineCap::Butt),
            "round" => Some(SimpleSvgStrokeLineCap::Round),
            "square" => Some(SimpleSvgStrokeLineCap::Square),
            _ => None,
        }
    };
    let parse_stroke_linejoin = |raw: &str| -> Option<SimpleSvgStrokeLineJoin> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "miter" | "miter-clip" => Some(SimpleSvgStrokeLineJoin::Miter),
            "round" => Some(SimpleSvgStrokeLineJoin::Round),
            "bevel" => Some(SimpleSvgStrokeLineJoin::Bevel),
            _ => None,
        }
    };
    let parse_fill_rule = |raw: &str| -> Option<SimpleSvgFillRule> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "nonzero" => Some(SimpleSvgFillRule::NonZero),
            "evenodd" => Some(SimpleSvgFillRule::EvenOdd),
            _ => None,
        }
    };
    let parse_text_anchor = |raw: &str| -> Option<SimpleSvgTextAnchor> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "start" => Some(SimpleSvgTextAnchor::Start),
            "middle" => Some(SimpleSvgTextAnchor::Middle),
            "end" => Some(SimpleSvgTextAnchor::End),
            _ => None,
        }
    };
    let parse_text_baseline = |raw: &str| -> Option<SimpleSvgTextBaseline> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "alphabetic" | "baseline" => Some(SimpleSvgTextBaseline::Alphabetic),
            "middle" | "central" => Some(SimpleSvgTextBaseline::Middle),
            _ => None,
        }
    };
    let parse_baseline_shift = |raw: &str| -> Option<SimpleSvgBaselineShift> {
        let raw = raw.trim();
        match raw.to_ascii_lowercase().as_str() {
            "baseline" => Some(SimpleSvgBaselineShift::Offset(0.0)),
            "super" => Some(SimpleSvgBaselineShift::Super),
            "sub" => Some(SimpleSvgBaselineShift::Sub),
            _ => {
                if let Some(percent) = raw.strip_suffix('%') {
                    return percent
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(|value| SimpleSvgBaselineShift::Percent(value / 100.0));
                }
                parse_number_prefix(raw)
                    .filter(|offset| offset.is_finite())
                    .map(SimpleSvgBaselineShift::Offset)
            }
        }
    };
    let parse_font_series = |raw: &str| -> Option<FontSeries> {
        let raw = raw.trim().to_ascii_lowercase();
        match raw.as_str() {
            "normal" | "lighter" => Some(FontSeries::Regular),
            "bold" | "bolder" => Some(FontSeries::Bold),
            _ => raw.parse::<u16>().ok().and_then(|weight| match weight {
                1..=599 => Some(FontSeries::Regular),
                600..=1000 => Some(FontSeries::Bold),
                _ => None,
            }),
        }
    };
    let parse_font_shape = |raw: &str| -> Option<FontShape> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(FontShape::Upright),
            "italic" | "oblique" => Some(FontShape::Italic),
            _ => None,
        }
    };
    let parse_font_size = |raw: &str| -> Option<SimpleSvgFontSize> {
        let raw = raw.trim();
        if let Some(percent) = raw.strip_suffix('%') {
            return percent
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(|value| SimpleSvgFontSize::Percent(value / 100.0));
        }
        parse_number_prefix(raw)
            .filter(|font_size| *font_size > 0.0)
            .map(SimpleSvgFontSize::Absolute)
    };
    let parse_font_family = |raw: &str| -> Option<SimpleSvgFontFamily> {
        raw.split(',').find_map(|family| {
            let family = family
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"')
                .to_ascii_lowercase();
            match family.as_str() {
                "serif" | "times" | "times new roman" | "dejavu serif" | "liberation serif" => {
                    Some(SimpleSvgFontFamily::Serif)
                }
                "sans" | "sans-serif" | "helvetica" | "arial" | "dejavu sans"
                | "liberation sans" => Some(SimpleSvgFontFamily::Sans),
                "mono" | "monospace" | "courier" | "courier new" | "dejavu sans mono"
                | "liberation mono" => Some(SimpleSvgFontFamily::Mono),
                _ => None,
            }
        })
    };
    let first_some = |left: Option<String>, right: Option<String>| left.or(right);
    let parse_optional_paint = |value: Option<String>| -> Option<Option<SimpleSvgColor>> {
        let value = value?;
        if value.trim().eq_ignore_ascii_case("inherit") {
            return None;
        }
        Some(parse_paint(&value))
    };
    let parse_optional_color = |value: Option<String>| -> Option<SimpleSvgResolvedColor> {
        let value = value?;
        if value.trim().eq_ignore_ascii_case("inherit") {
            return None;
        }
        parse_color(&value)
    };
    let presentation_from_values = |fill: Option<String>,
                                    fill_rule: Option<String>,
                                    stroke: Option<String>,
                                    stroke_width: Option<String>,
                                    stroke_dasharray: Option<String>,
                                    stroke_dashoffset: Option<String>,
                                    stroke_linecap: Option<String>,
                                    stroke_linejoin: Option<String>,
                                    stroke_miterlimit: Option<String>,
                                    color: Option<String>,
                                    opacity: Option<String>,
                                    fill_opacity: Option<String>,
                                    stroke_opacity: Option<String>,
                                    text_anchor: Option<String>,
                                    font_size: Option<String>,
                                    font_family: Option<String>,
                                    font_weight: Option<String>,
                                    font_style: Option<String>,
                                    text_baseline: Option<String>,
                                    baseline_shift: Option<String>|
     -> SimpleSvgPresentation {
        SimpleSvgPresentation {
            fill: parse_optional_paint(fill),
            fill_rule: fill_rule.as_deref().and_then(parse_fill_rule),
            stroke: parse_optional_paint(stroke),
            stroke_width: stroke_width
                .as_deref()
                .and_then(parse_number_prefix)
                .filter(|width| *width > 0.0),
            stroke_dasharray: stroke_dasharray.as_deref().and_then(parse_dasharray),
            stroke_dashoffset: stroke_dashoffset
                .as_deref()
                .and_then(parse_number_prefix)
                .filter(|offset| *offset >= 0.0),
            stroke_linecap: stroke_linecap.as_deref().and_then(parse_stroke_linecap),
            stroke_linejoin: stroke_linejoin.as_deref().and_then(parse_stroke_linejoin),
            stroke_miterlimit: stroke_miterlimit
                .as_deref()
                .and_then(parse_number_prefix)
                .filter(|limit| *limit >= 1.0),
            color: parse_optional_color(color),
            opacity: opacity.as_deref().and_then(parse_opacity),
            fill_opacity: fill_opacity.as_deref().and_then(parse_opacity),
            stroke_opacity: stroke_opacity.as_deref().and_then(parse_opacity),
            text_anchor: text_anchor.as_deref().and_then(parse_text_anchor),
            font_size: font_size.as_deref().and_then(parse_font_size),
            font_family: font_family.as_deref().and_then(parse_font_family),
            font_series: font_weight.as_deref().and_then(parse_font_series),
            font_shape: font_style.as_deref().and_then(parse_font_shape),
            text_baseline: text_baseline.as_deref().and_then(parse_text_baseline),
            baseline_shift: baseline_shift.as_deref().and_then(parse_baseline_shift),
        }
    };
    let parse_attr_presentation = |tag: &str| -> SimpleSvgPresentation {
        presentation_from_values(
            attr_value(tag, "fill"),
            attr_value(tag, "fill-rule"),
            attr_value(tag, "stroke"),
            attr_value(tag, "stroke-width"),
            attr_value(tag, "stroke-dasharray"),
            attr_value(tag, "stroke-dashoffset"),
            attr_value(tag, "stroke-linecap"),
            attr_value(tag, "stroke-linejoin"),
            attr_value(tag, "stroke-miterlimit"),
            attr_value(tag, "color"),
            attr_value(tag, "opacity"),
            attr_value(tag, "fill-opacity"),
            attr_value(tag, "stroke-opacity"),
            attr_value(tag, "text-anchor"),
            attr_value(tag, "font-size"),
            attr_value(tag, "font-family"),
            attr_value(tag, "font-weight"),
            attr_value(tag, "font-style"),
            first_some(
                attr_value(tag, "dominant-baseline"),
                attr_value(tag, "alignment-baseline"),
            ),
            attr_value(tag, "baseline-shift"),
        )
    };
    let parse_inline_style_presentation = |tag: &str| -> SimpleSvgPresentation {
        presentation_from_values(
            style_value(tag, "fill"),
            style_value(tag, "fill-rule"),
            style_value(tag, "stroke"),
            style_value(tag, "stroke-width"),
            style_value(tag, "stroke-dasharray"),
            style_value(tag, "stroke-dashoffset"),
            style_value(tag, "stroke-linecap"),
            style_value(tag, "stroke-linejoin"),
            style_value(tag, "stroke-miterlimit"),
            style_value(tag, "color"),
            style_value(tag, "opacity"),
            style_value(tag, "fill-opacity"),
            style_value(tag, "stroke-opacity"),
            style_value(tag, "text-anchor"),
            style_value(tag, "font-size"),
            style_value(tag, "font-family"),
            style_value(tag, "font-weight"),
            style_value(tag, "font-style"),
            first_some(
                style_value(tag, "dominant-baseline"),
                style_value(tag, "alignment-baseline"),
            ),
            style_value(tag, "baseline-shift"),
        )
    };
    let parse_declaration_presentation = |declarations: &str| -> SimpleSvgPresentation {
        presentation_from_values(
            declaration_value(declarations, "fill"),
            declaration_value(declarations, "fill-rule"),
            declaration_value(declarations, "stroke"),
            declaration_value(declarations, "stroke-width"),
            declaration_value(declarations, "stroke-dasharray"),
            declaration_value(declarations, "stroke-dashoffset"),
            declaration_value(declarations, "stroke-linecap"),
            declaration_value(declarations, "stroke-linejoin"),
            declaration_value(declarations, "stroke-miterlimit"),
            declaration_value(declarations, "color"),
            declaration_value(declarations, "opacity"),
            declaration_value(declarations, "fill-opacity"),
            declaration_value(declarations, "stroke-opacity"),
            declaration_value(declarations, "text-anchor"),
            declaration_value(declarations, "font-size"),
            declaration_value(declarations, "font-family"),
            declaration_value(declarations, "font-weight"),
            declaration_value(declarations, "font-style"),
            first_some(
                declaration_value(declarations, "dominant-baseline"),
                declaration_value(declarations, "alignment-baseline"),
            ),
            declaration_value(declarations, "baseline-shift"),
        )
    };
    #[derive(Debug, Clone)]
    enum SimpleSvgStyleSelector {
        Type {
            element_name: String,
        },
        Class {
            element_name: Option<String>,
            class_name: String,
        },
        Id {
            element_name: Option<String>,
            id: String,
        },
    }
    #[derive(Debug, Clone)]
    struct SimpleSvgStyleRule {
        selector: SimpleSvgStyleSelector,
        specificity: u16,
        presentation: SimpleSvgPresentation,
    }
    #[derive(Debug, Clone, Copy)]
    struct SimpleSvgCascadeValue<T> {
        value: T,
        specificity: u16,
        order: usize,
    }
    let valid_svg_element_name = |element_name: &str| {
        !element_name.is_empty()
            && element_name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    };
    let parse_style_selector = |selector: &str| -> Option<SimpleSvgStyleSelector> {
        let selector = selector.trim();
        if selector.chars().any(char::is_whitespace) {
            return None;
        }
        if selector.contains('.') {
            let (element_name, class_selector) =
                if let Some(class_selector) = selector.strip_prefix('.') {
                    (None, class_selector)
                } else {
                    let dot_index = selector.find('.')?;
                    let element_name = selector[..dot_index].trim();
                    if !valid_svg_element_name(element_name) {
                        return None;
                    }
                    (
                        Some(element_name.to_ascii_lowercase()),
                        &selector[dot_index + 1..],
                    )
                };
            let class_name = class_selector
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
                .collect::<String>();
            (!class_name.is_empty()).then_some(SimpleSvgStyleSelector::Class {
                element_name,
                class_name,
            })
        } else if selector.contains('#') {
            let (element_name, id_selector) = if let Some(id_selector) = selector.strip_prefix('#')
            {
                (None, id_selector)
            } else {
                let hash_index = selector.find('#')?;
                let element_name = selector[..hash_index].trim();
                if !valid_svg_element_name(element_name) {
                    return None;
                }
                (
                    Some(element_name.to_ascii_lowercase()),
                    &selector[hash_index + 1..],
                )
            };
            let id = id_selector
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
                .collect::<String>();
            (!id.is_empty()).then_some(SimpleSvgStyleSelector::Id { element_name, id })
        } else if valid_svg_element_name(selector) {
            Some(SimpleSvgStyleSelector::Type {
                element_name: selector.to_ascii_lowercase(),
            })
        } else {
            None
        }
    };
    let selector_specificity = |selector: &SimpleSvgStyleSelector| -> u16 {
        match selector {
            SimpleSvgStyleSelector::Type { .. } => 1,
            SimpleSvgStyleSelector::Class { element_name, .. } => {
                10 + u16::from(element_name.is_some())
            }
            SimpleSvgStyleSelector::Id { element_name, .. } => {
                100 + u16::from(element_name.is_some())
            }
        }
    };
    let mut style_rules = Vec::new();
    let mut style_block_offset = 0usize;
    while let Some(style_start_relative) = text[style_block_offset..].find("<style") {
        let style_start = style_block_offset + style_start_relative;
        let style_tag_tail = &text[style_start..];
        if !is_start_tag_named(style_tag_tail, "style") {
            style_block_offset = style_start + "<style".len();
            continue;
        }
        let Some(style_tag_end) = style_tag_tail.find('>') else {
            break;
        };
        let content_start = style_start + style_tag_end + 1;
        let Some(content_end_relative) = text[content_start..].find("</style>") else {
            break;
        };
        let css = text[content_start..content_start + content_end_relative].trim();
        let css = css
            .strip_prefix("<![CDATA[")
            .and_then(|css| css.strip_suffix("]]>"))
            .unwrap_or(css);
        let mut css_without_comments = String::new();
        let mut css_comment_offset = 0usize;
        while let Some(comment_start_relative) = css[css_comment_offset..].find("/*") {
            let comment_start = css_comment_offset + comment_start_relative;
            css_without_comments.push_str(&css[css_comment_offset..comment_start]);
            let comment_body_start = comment_start + 2;
            let Some(comment_end_relative) = css[comment_body_start..].find("*/") else {
                css_comment_offset = css.len();
                break;
            };
            css_comment_offset = comment_body_start + comment_end_relative + 2;
        }
        css_without_comments.push_str(&css[css_comment_offset..]);
        let css = css_without_comments.as_str();
        let mut css_offset = 0usize;
        while let Some(selector_end_relative) = css[css_offset..].find('{') {
            let selector_end = css_offset + selector_end_relative;
            let body_start = selector_end + 1;
            let Some(body_end_relative) = css[body_start..].find('}') else {
                break;
            };
            let body_end = body_start + body_end_relative;
            let presentation = parse_declaration_presentation(&css[body_start..body_end]);
            for selector in css[css_offset..selector_end].split(',') {
                let Some(selector) = parse_style_selector(selector) else {
                    continue;
                };
                let specificity = selector_specificity(&selector);
                style_rules.push(SimpleSvgStyleRule {
                    selector,
                    specificity,
                    presentation,
                });
            }
            css_offset = body_end + 1;
        }
        style_block_offset = content_start + content_end_relative + "</style>".len();
    }
    let overlay_presentation =
        |base: SimpleSvgPresentation, local: SimpleSvgPresentation| -> SimpleSvgPresentation {
            SimpleSvgPresentation {
                fill: local.fill.or(base.fill),
                fill_rule: local.fill_rule.or(base.fill_rule),
                stroke: local.stroke.or(base.stroke),
                stroke_width: local.stroke_width.or(base.stroke_width),
                stroke_dasharray: local.stroke_dasharray.or(base.stroke_dasharray),
                stroke_dashoffset: local.stroke_dashoffset.or(base.stroke_dashoffset),
                stroke_linecap: local.stroke_linecap.or(base.stroke_linecap),
                stroke_linejoin: local.stroke_linejoin.or(base.stroke_linejoin),
                stroke_miterlimit: local.stroke_miterlimit.or(base.stroke_miterlimit),
                color: local.color.or(base.color),
                opacity: local.opacity.or(base.opacity),
                fill_opacity: local.fill_opacity.or(base.fill_opacity),
                stroke_opacity: local.stroke_opacity.or(base.stroke_opacity),
                text_anchor: local.text_anchor.or(base.text_anchor),
                font_size: local.font_size.or(base.font_size),
                font_family: local.font_family.or(base.font_family),
                font_series: local.font_series.or(base.font_series),
                font_shape: local.font_shape.or(base.font_shape),
                text_baseline: local.text_baseline.or(base.text_baseline),
                baseline_shift: local.baseline_shift.or(base.baseline_shift),
            }
        };
    let inherit_presentation = |parent: SimpleSvgPresentation,
                                local: SimpleSvgPresentation|
     -> SimpleSvgPresentation {
        let opacity = match (parent.opacity, local.opacity) {
            (Some(parent), Some(local)) => Some((parent * local).clamp(0.0, 1.0)),
            (Some(parent), None) => Some(parent),
            (None, Some(local)) => Some(local),
            (None, None) => None,
        };
        let font_size = match (parent.font_size, local.font_size) {
            (_, Some(SimpleSvgFontSize::Absolute(size))) => Some(SimpleSvgFontSize::Absolute(size)),
            (
                Some(SimpleSvgFontSize::Absolute(parent_size)),
                Some(SimpleSvgFontSize::Percent(scale)),
            ) => Some(SimpleSvgFontSize::Absolute(parent_size * scale)),
            (
                Some(SimpleSvgFontSize::Percent(parent_scale)),
                Some(SimpleSvgFontSize::Percent(scale)),
            ) => Some(SimpleSvgFontSize::Percent(parent_scale * scale)),
            (None, Some(SimpleSvgFontSize::Percent(scale))) => {
                Some(SimpleSvgFontSize::Percent(scale))
            }
            (Some(parent_size), None) => Some(parent_size),
            (None, None) => None,
        };
        SimpleSvgPresentation {
            fill: local.fill.or(parent.fill),
            fill_rule: local.fill_rule.or(parent.fill_rule),
            stroke: local.stroke.or(parent.stroke),
            stroke_width: local.stroke_width.or(parent.stroke_width),
            stroke_dasharray: local.stroke_dasharray.or(parent.stroke_dasharray),
            stroke_dashoffset: local.stroke_dashoffset.or(parent.stroke_dashoffset),
            stroke_linecap: local.stroke_linecap.or(parent.stroke_linecap),
            stroke_linejoin: local.stroke_linejoin.or(parent.stroke_linejoin),
            stroke_miterlimit: local.stroke_miterlimit.or(parent.stroke_miterlimit),
            color: local.color.or(parent.color),
            opacity,
            fill_opacity: local.fill_opacity.or(parent.fill_opacity),
            stroke_opacity: local.stroke_opacity.or(parent.stroke_opacity),
            text_anchor: local.text_anchor.or(parent.text_anchor),
            font_size,
            font_family: local.font_family.or(parent.font_family),
            font_series: local.font_series.or(parent.font_series),
            font_shape: local.font_shape.or(parent.font_shape),
            text_baseline: local.text_baseline.or(parent.text_baseline),
            baseline_shift: local.baseline_shift.or(parent.baseline_shift),
        }
    };
    let tag_element_name = |tag: &str| -> Option<String> {
        let tag = tag.trim_start();
        let tag = tag.strip_prefix('<').unwrap_or(tag).trim_start();
        let tag = tag.strip_prefix('/').unwrap_or(tag).trim_start();
        let name = tag
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
            .collect::<String>();
        (!name.is_empty()).then(|| name.to_ascii_lowercase())
    };
    let style_rule_presentation_for = |tag: &str| -> SimpleSvgPresentation {
        let tag_element_name = tag_element_name(tag);
        let class_attr = attr_value(tag, "class");
        let id_attr = attr_value(tag, "id");
        let should_replace_cascade_value =
            |current: Option<(u16, usize)>, specificity: u16, order: usize| {
                current
                    .map(|(current_specificity, current_order)| {
                        specificity > current_specificity
                            || (specificity == current_specificity && order >= current_order)
                    })
                    .unwrap_or(true)
            };
        let mut fill: Option<SimpleSvgCascadeValue<Option<SimpleSvgColor>>> = None;
        let mut fill_rule: Option<SimpleSvgCascadeValue<SimpleSvgFillRule>> = None;
        let mut stroke: Option<SimpleSvgCascadeValue<Option<SimpleSvgColor>>> = None;
        let mut stroke_width: Option<SimpleSvgCascadeValue<f32>> = None;
        let mut stroke_dasharray: Option<SimpleSvgCascadeValue<Option<SimpleSvgDashArray>>> = None;
        let mut stroke_dashoffset: Option<SimpleSvgCascadeValue<f32>> = None;
        let mut stroke_linecap: Option<SimpleSvgCascadeValue<SimpleSvgStrokeLineCap>> = None;
        let mut stroke_linejoin: Option<SimpleSvgCascadeValue<SimpleSvgStrokeLineJoin>> = None;
        let mut stroke_miterlimit: Option<SimpleSvgCascadeValue<f32>> = None;
        let mut color: Option<SimpleSvgCascadeValue<SimpleSvgResolvedColor>> = None;
        let mut opacity: Option<SimpleSvgCascadeValue<f32>> = None;
        let mut fill_opacity: Option<SimpleSvgCascadeValue<f32>> = None;
        let mut stroke_opacity: Option<SimpleSvgCascadeValue<f32>> = None;
        let mut text_anchor: Option<SimpleSvgCascadeValue<SimpleSvgTextAnchor>> = None;
        let mut font_size: Option<SimpleSvgCascadeValue<SimpleSvgFontSize>> = None;
        let mut font_family: Option<SimpleSvgCascadeValue<SimpleSvgFontFamily>> = None;
        let mut font_series: Option<SimpleSvgCascadeValue<FontSeries>> = None;
        let mut font_shape: Option<SimpleSvgCascadeValue<FontShape>> = None;
        let mut text_baseline: Option<SimpleSvgCascadeValue<SimpleSvgTextBaseline>> = None;
        let mut baseline_shift: Option<SimpleSvgCascadeValue<SimpleSvgBaselineShift>> = None;
        for (order, rule) in style_rules.iter().enumerate() {
            let matches = match &rule.selector {
                SimpleSvgStyleSelector::Type { element_name } => {
                    tag_element_name.as_deref() == Some(element_name.as_str())
                }
                SimpleSvgStyleSelector::Class {
                    element_name,
                    class_name,
                } => {
                    let element_matches = element_name
                        .as_ref()
                        .map(|element_name| {
                            tag_element_name.as_deref() == Some(element_name.as_str())
                        })
                        .unwrap_or(true);
                    element_matches
                        && class_attr
                            .as_ref()
                            .map(|class_attr| {
                                class_attr
                                    .split_whitespace()
                                    .any(|tag_class_name| tag_class_name == class_name)
                            })
                            .unwrap_or(false)
                }
                SimpleSvgStyleSelector::Id { element_name, id } => {
                    let element_matches = element_name
                        .as_ref()
                        .map(|element_name| {
                            tag_element_name.as_deref() == Some(element_name.as_str())
                        })
                        .unwrap_or(true);
                    element_matches && id_attr.as_deref() == Some(id.as_str())
                }
            };
            if matches {
                if let Some(value) = rule.presentation.fill {
                    let current = fill.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.fill_rule {
                    let current = fill_rule.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill_rule = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke {
                    let current = stroke.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_width {
                    let current = stroke_width.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_width = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_dasharray {
                    let current = stroke_dasharray.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_dasharray = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_dashoffset {
                    let current = stroke_dashoffset.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_dashoffset = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_linecap {
                    let current = stroke_linecap.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_linecap = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_linejoin {
                    let current = stroke_linejoin.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_linejoin = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_miterlimit {
                    let current = stroke_miterlimit.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_miterlimit = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.color {
                    let current = color.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        color = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.opacity {
                    let current = opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        opacity = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.fill_opacity {
                    let current = fill_opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill_opacity = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_opacity {
                    let current = stroke_opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_opacity = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.text_anchor {
                    let current = text_anchor.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_anchor = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.font_size {
                    let current = font_size.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_size = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.font_family {
                    let current = font_family.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_family = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.font_series {
                    let current = font_series.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_series = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.font_shape {
                    let current = font_shape.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_shape = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.text_baseline {
                    let current = text_baseline.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_baseline = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.baseline_shift {
                    let current = baseline_shift.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        baseline_shift = Some(SimpleSvgCascadeValue {
                            value,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
            }
        }
        SimpleSvgPresentation {
            fill: fill.map(|value| value.value),
            fill_rule: fill_rule.map(|value| value.value),
            stroke: stroke.map(|value| value.value),
            stroke_width: stroke_width.map(|value| value.value),
            stroke_dasharray: stroke_dasharray.map(|value| value.value),
            stroke_dashoffset: stroke_dashoffset.map(|value| value.value),
            stroke_linecap: stroke_linecap.map(|value| value.value),
            stroke_linejoin: stroke_linejoin.map(|value| value.value),
            stroke_miterlimit: stroke_miterlimit.map(|value| value.value),
            color: color.map(|value| value.value),
            opacity: opacity.map(|value| value.value),
            fill_opacity: fill_opacity.map(|value| value.value),
            stroke_opacity: stroke_opacity.map(|value| value.value),
            text_anchor: text_anchor.map(|value| value.value),
            font_size: font_size.map(|value| value.value),
            font_family: font_family.map(|value| value.value),
            font_series: font_series.map(|value| value.value),
            font_shape: font_shape.map(|value| value.value),
            text_baseline: text_baseline.map(|value| value.value),
            baseline_shift: baseline_shift.map(|value| value.value),
        }
    };
    let parse_presentation = |tag: &str| -> SimpleSvgPresentation {
        let attr_presentation = parse_attr_presentation(tag);
        let class_presentation = style_rule_presentation_for(tag);
        let inline_style_presentation = parse_inline_style_presentation(tag);
        overlay_presentation(
            overlay_presentation(attr_presentation, class_presentation),
            inline_style_presentation,
        )
    };
    let resolved_font_size = |presentation: SimpleSvgPresentation| -> f32 {
        match presentation.font_size {
            Some(SimpleSvgFontSize::Absolute(size)) => size,
            Some(SimpleSvgFontSize::Percent(scale)) => 12.0 * scale,
            None => 12.0,
        }
    };
    let baseline_y_offset = |presentation: SimpleSvgPresentation, font_size: f32| -> f32 {
        match presentation
            .text_baseline
            .unwrap_or(SimpleSvgTextBaseline::Alphabetic)
        {
            SimpleSvgTextBaseline::Alphabetic => 0.0,
            SimpleSvgTextBaseline::Middle => font_size * 0.5,
        }
    };
    let baseline_shift_y_offset = |presentation: SimpleSvgPresentation, font_size: f32| -> f32 {
        match presentation.baseline_shift {
            Some(SimpleSvgBaselineShift::Offset(offset)) => -offset,
            Some(SimpleSvgBaselineShift::Percent(scale)) => -font_size * scale,
            Some(SimpleSvgBaselineShift::Super) => -font_size * 0.6,
            Some(SimpleSvgBaselineShift::Sub) => font_size * 0.2,
            None => 0.0,
        }
    };
    let root_presentation = parse_presentation(svg_tag);
    let stroke_width_ratio = |presentation: SimpleSvgPresentation| -> f32 {
        presentation.stroke_width.unwrap_or(1.0) / view_box.2
    };
    let stroke_dasharray_ratio =
        |presentation: SimpleSvgPresentation| -> Option<SimpleSvgDashArray> {
            presentation
                .stroke_dasharray
                .unwrap_or(None)
                .map(|mut dasharray| {
                    for index in 0..dasharray.len {
                        dasharray.values[index] /= view_box.2;
                    }
                    dasharray.offset_ratio =
                        presentation.stroke_dashoffset.unwrap_or(0.0) / view_box.2;
                    dasharray
                })
        };
    let stroke_style = |presentation: SimpleSvgPresentation| -> SimpleSvgStrokeStyle {
        SimpleSvgStrokeStyle {
            linecap: presentation
                .stroke_linecap
                .unwrap_or(SimpleSvgStrokeLineCap::Butt),
            linejoin: presentation
                .stroke_linejoin
                .unwrap_or(SimpleSvgStrokeLineJoin::Miter),
            miterlimit: presentation.stroke_miterlimit.unwrap_or(4.0),
        }
    };
    let resolve_svg_color =
        |color: SimpleSvgColor, current_color: SimpleSvgResolvedColor| match color {
            SimpleSvgColor::Resolved(color) => color,
            SimpleSvgColor::CurrentColor => current_color,
        };
    let paint_from_color = |color: Option<SimpleSvgColor>,
                            opacity: f32,
                            current_color: SimpleSvgResolvedColor|
     -> Option<SimpleSvgPaint> {
        let color = resolve_svg_color(color?, current_color);
        let opacity = (opacity * color.alpha).clamp(0.0, 1.0);
        (opacity > 0.0).then_some(SimpleSvgPaint {
            rgb: color.rgb,
            opacity,
        })
    };
    let fill_paint = |presentation: SimpleSvgPresentation,
                      default_rgb: Option<(f32, f32, f32)>|
     -> Option<SimpleSvgPaint> {
        let current_color = presentation
            .color
            .unwrap_or_else(|| SimpleSvgResolvedColor::opaque((0.0, 0.0, 0.0)));
        paint_from_color(
            presentation.fill.unwrap_or_else(|| {
                default_rgb
                    .map(SimpleSvgResolvedColor::opaque)
                    .map(SimpleSvgColor::Resolved)
            }),
            presentation.opacity.unwrap_or(1.0) * presentation.fill_opacity.unwrap_or(1.0),
            current_color,
        )
    };
    let fill_rule = |presentation: SimpleSvgPresentation| -> SimpleSvgFillRule {
        presentation.fill_rule.unwrap_or(SimpleSvgFillRule::NonZero)
    };
    let stroke_paint = |presentation: SimpleSvgPresentation| -> Option<SimpleSvgPaint> {
        let current_color = presentation
            .color
            .unwrap_or_else(|| SimpleSvgResolvedColor::opaque((0.0, 0.0, 0.0)));
        paint_from_color(
            presentation.stroke.unwrap_or(None),
            presentation.opacity.unwrap_or(1.0) * presentation.stroke_opacity.unwrap_or(1.0),
            current_color,
        )
    };
    #[derive(Debug, Clone, Copy)]
    struct SimpleSvgTransform {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
        stroke_scale: f32,
        axis_aligned: bool,
    }
    let identity_transform = SimpleSvgTransform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
        stroke_scale: 1.0,
        axis_aligned: true,
    };
    let compose_transform = |inner: SimpleSvgTransform,
                             outer: SimpleSvgTransform,
                             outer_stroke_scale: f32|
     -> SimpleSvgTransform {
        SimpleSvgTransform {
            a: outer.a * inner.a + outer.c * inner.b,
            b: outer.b * inner.a + outer.d * inner.b,
            c: outer.a * inner.c + outer.c * inner.d,
            d: outer.b * inner.c + outer.d * inner.d,
            e: outer.a * inner.e + outer.c * inner.f + outer.e,
            f: outer.b * inner.e + outer.d * inner.f + outer.f,
            stroke_scale: inner.stroke_scale * outer_stroke_scale,
            axis_aligned: outer.axis_aligned && inner.axis_aligned,
        }
    };
    let parse_transform_numbers = |raw: &str| -> Option<Vec<f32>> {
        raw.split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter(|part| !part.is_empty())
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .filter(|values| values.iter().all(|value| value.is_finite()))
    };
    let parse_transform = |tag: &str| -> Option<SimpleSvgTransform> {
        let Some(raw) = attr_value(tag, "transform") else {
            return Some(identity_transform);
        };
        let mut transform = identity_transform;
        let mut index = 0usize;
        while index < raw.len() {
            while index < raw.len() {
                let ch = raw[index..].chars().next()?;
                if ch.is_whitespace() || ch == ',' {
                    index += ch.len_utf8();
                } else {
                    break;
                }
            }
            if index >= raw.len() {
                break;
            }
            let name_start = index;
            while index < raw.len() {
                let ch = raw[index..].chars().next()?;
                if ch.is_ascii_alphabetic() {
                    index += ch.len_utf8();
                } else {
                    break;
                }
            }
            let name = raw[name_start..index].trim();
            while index < raw.len() {
                let ch = raw[index..].chars().next()?;
                if ch.is_whitespace() {
                    index += ch.len_utf8();
                } else {
                    break;
                }
            }
            if raw[index..].chars().next()? != '(' {
                return None;
            }
            index += 1;
            let args_start = index;
            let args_len = raw[args_start..].find(')')?;
            index = args_start + args_len;
            let values = parse_transform_numbers(&raw[args_start..index])?;
            index += 1;
            let (next, next_stroke_scale) = match name {
                "translate" => {
                    if values.is_empty() || values.len() > 2 {
                        return None;
                    }
                    let tx = values[0];
                    let ty = values.get(1).copied().unwrap_or(0.0);
                    (
                        SimpleSvgTransform {
                            e: tx,
                            f: ty,
                            ..identity_transform
                        },
                        1.0,
                    )
                }
                "matrix" => {
                    if values.len() != 6 {
                        return None;
                    }
                    let scale_x = values[0].hypot(values[1]);
                    let scale_y = values[2].hypot(values[3]);
                    if scale_x == 0.0 || scale_y == 0.0 {
                        return None;
                    }
                    (
                        SimpleSvgTransform {
                            a: values[0],
                            b: values[1],
                            c: values[2],
                            d: values[3],
                            e: values[4],
                            f: values[5],
                            stroke_scale: 1.0,
                            axis_aligned: values[1].abs() <= f32::EPSILON
                                && values[2].abs() <= f32::EPSILON,
                        },
                        (scale_x + scale_y) / 2.0,
                    )
                }
                "rotate" => {
                    if values.is_empty() || values.len() > 3 {
                        return None;
                    }
                    let radians = values[0].to_radians();
                    let cos = radians.cos();
                    let sin = radians.sin();
                    let (cx, cy) = if values.len() == 3 {
                        (values[1], values[2])
                    } else {
                        (0.0, 0.0)
                    };
                    (
                        SimpleSvgTransform {
                            a: cos,
                            b: sin,
                            c: -sin,
                            d: cos,
                            e: cx - cos * cx + sin * cy,
                            f: cy - sin * cx - cos * cy,
                            stroke_scale: 1.0,
                            axis_aligned: sin.abs() <= f32::EPSILON,
                        },
                        1.0,
                    )
                }
                "scale" => {
                    if values.is_empty() || values.len() > 2 {
                        return None;
                    }
                    let sx = values[0];
                    let sy = values.get(1).copied().unwrap_or(sx);
                    if sx == 0.0 || sy == 0.0 {
                        return None;
                    }
                    (
                        SimpleSvgTransform {
                            a: sx,
                            d: sy,
                            ..identity_transform
                        },
                        (sx.abs() + sy.abs()) / 2.0,
                    )
                }
                "skewX" => {
                    if values.len() != 1 {
                        return None;
                    }
                    let skew = values[0].to_radians().tan();
                    if !skew.is_finite() {
                        return None;
                    }
                    (
                        SimpleSvgTransform {
                            c: skew,
                            axis_aligned: skew.abs() <= f32::EPSILON,
                            ..identity_transform
                        },
                        (1.0 + skew.hypot(1.0)) / 2.0,
                    )
                }
                "skewY" => {
                    if values.len() != 1 {
                        return None;
                    }
                    let skew = values[0].to_radians().tan();
                    if !skew.is_finite() {
                        return None;
                    }
                    (
                        SimpleSvgTransform {
                            b: skew,
                            axis_aligned: skew.abs() <= f32::EPSILON,
                            ..identity_transform
                        },
                        (skew.hypot(1.0) + 1.0) / 2.0,
                    )
                }
                _ => return None,
            };
            transform = compose_transform(transform, next, next_stroke_scale);
            if !transform.a.is_finite()
                || !transform.b.is_finite()
                || !transform.c.is_finite()
                || !transform.d.is_finite()
                || !transform.e.is_finite()
                || !transform.f.is_finite()
                || !transform.stroke_scale.is_finite()
            {
                return None;
            }
        }
        Some(transform)
    };
    let snap_transform_number = |value: f32| -> f32 {
        let rounded = value.round();
        if (value - rounded).abs() <= 0.000_1 {
            rounded
        } else {
            value
        }
    };
    let apply_transform = |transform: SimpleSvgTransform, x: f32, y: f32| -> Option<(f32, f32)> {
        let transformed_x = transform.a * x + transform.c * y + transform.e;
        let transformed_y = transform.b * x + transform.d * y + transform.f;
        (transformed_x.is_finite() && transformed_y.is_finite()).then_some((
            snap_transform_number(transformed_x),
            snap_transform_number(transformed_y),
        ))
    };
    let normalize_point = |point: (f32, f32)| -> (f32, f32) {
        (
            (point.0 - view_box.0) / view_box.2,
            (point.1 - view_box.1) / view_box.3,
        )
    };
    let transformed_stroke_width_ratio =
        |presentation: SimpleSvgPresentation, transform: SimpleSvgTransform| -> f32 {
            stroke_width_ratio(presentation) * transform.stroke_scale
        };
    let ellipse_path_ops = |cx: f32,
                            cy: f32,
                            rx: f32,
                            ry: f32,
                            transform: SimpleSvgTransform|
     -> Option<Vec<SimpleSvgPathOp>> {
        let kappa = 0.552_284_8_f32;
        let transform_point = |x: f32, y: f32| -> Option<(f32, f32)> {
            Some(normalize_point(apply_transform(transform, x, y)?))
        };
        Some(vec![
            SimpleSvgPathOp::MoveTo(transform_point(cx + rx, cy)?),
            SimpleSvgPathOp::CubicTo {
                ctrl1: transform_point(cx + rx, cy + kappa * ry)?,
                ctrl2: transform_point(cx + kappa * rx, cy + ry)?,
                to: transform_point(cx, cy + ry)?,
            },
            SimpleSvgPathOp::CubicTo {
                ctrl1: transform_point(cx - kappa * rx, cy + ry)?,
                ctrl2: transform_point(cx - rx, cy + kappa * ry)?,
                to: transform_point(cx - rx, cy)?,
            },
            SimpleSvgPathOp::CubicTo {
                ctrl1: transform_point(cx - rx, cy - kappa * ry)?,
                ctrl2: transform_point(cx - kappa * rx, cy - ry)?,
                to: transform_point(cx, cy - ry)?,
            },
            SimpleSvgPathOp::CubicTo {
                ctrl1: transform_point(cx + kappa * rx, cy - ry)?,
                ctrl2: transform_point(cx + rx, cy - kappa * ry)?,
                to: transform_point(cx + rx, cy)?,
            },
            SimpleSvgPathOp::Close,
        ])
    };
    let parse_raw_points = |raw: &str| -> Option<Vec<(f32, f32)>> {
        let values = raw
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter(|part| !part.is_empty())
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if values.len() < 4 || values.len() % 2 != 0 {
            return None;
        }
        let mut points = Vec::new();
        for pair in values.chunks_exact(2) {
            if !pair[0].is_finite() || !pair[1].is_finite() {
                return None;
            }
            points.push((pair[0], pair[1]));
        }
        Some(points)
    };
    let parse_points = |raw: &str, transform: SimpleSvgTransform| -> Option<Vec<(f32, f32)>> {
        let raw_points = parse_raw_points(raw)?;
        let mut points = Vec::new();
        for (x, y) in raw_points {
            points.push(normalize_point(apply_transform(transform, x, y)?));
        }
        Some(points)
    };
    let path_data_from_points = |points: &[(f32, f32)], closed: bool| -> Option<String> {
        let first = points.first()?;
        if points.len() < 2 {
            return None;
        }
        let mut path_data = format!("M {} {}", first.0, first.1);
        for point in &points[1..] {
            path_data.push_str(&format!(" L {} {}", point.0, point.1));
        }
        if closed {
            path_data.push_str(" Z");
        }
        Some(path_data)
    };
    let rect_path_data = |x: f32, y: f32, width: f32, height: f32| -> Option<String> {
        (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0).then(|| {
            format!(
                "M {} {} L {} {} L {} {} L {} {} Z",
                x,
                y,
                x + width,
                y,
                x + width,
                y + height,
                x,
                y + height
            )
        })
    };
    let ellipse_path_data = |cx: f32, cy: f32, rx: f32, ry: f32| -> Option<String> {
        (rx.is_finite() && ry.is_finite() && rx > 0.0 && ry > 0.0).then(|| {
            format!(
                "M {} {} A {} {} 0 1 0 {} {} A {} {} 0 1 0 {} {} Z",
                cx + rx,
                cy,
                rx,
                ry,
                cx - rx,
                cy,
                rx,
                ry,
                cx + rx,
                cy
            )
        })
    };
    #[derive(Debug, Clone, Copy)]
    enum SimplePathToken {
        Command(char),
        Number(f32),
    }
    #[derive(Debug, Clone, Copy)]
    struct SimpleSvgArcCommand {
        current: (f32, f32),
        rx: f32,
        ry: f32,
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        to: (f32, f32),
        transform: SimpleSvgTransform,
    }
    type SimplePathParse = Option<(Vec<SimpleSvgPathOp>, bool)>;
    let arc_to_cubics = |arc: SimpleSvgArcCommand| -> Option<Vec<SimpleSvgPathOp>> {
        let SimpleSvgArcCommand {
            current,
            mut rx,
            mut ry,
            x_axis_rotation,
            large_arc,
            sweep,
            to,
            transform,
        } = arc;
        if !rx.is_finite()
            || !ry.is_finite()
            || !x_axis_rotation.is_finite()
            || !to.0.is_finite()
            || !to.1.is_finite()
        {
            return None;
        }
        rx = rx.abs();
        ry = ry.abs();
        if rx == 0.0 || ry == 0.0 {
            return Some(vec![SimpleSvgPathOp::LineTo(normalize_point(
                apply_transform(transform, to.0, to.1)?,
            ))]);
        }
        if (current.0 - to.0).abs() <= f32::EPSILON && (current.1 - to.1).abs() <= f32::EPSILON {
            return Some(Vec::new());
        }

        let radians = x_axis_rotation.to_radians();
        let cos_phi = radians.cos();
        let sin_phi = radians.sin();
        let dx = (current.0 - to.0) / 2.0;
        let dy = (current.1 - to.1) / 2.0;
        let x1_prime = cos_phi * dx + sin_phi * dy;
        let y1_prime = -sin_phi * dx + cos_phi * dy;
        let lambda = x1_prime.powi(2) / rx.powi(2) + y1_prime.powi(2) / ry.powi(2);
        if lambda > 1.0 {
            let scale = lambda.sqrt();
            rx *= scale;
            ry *= scale;
        }

        let rx_sq = rx.powi(2);
        let ry_sq = ry.powi(2);
        let x1_prime_sq = x1_prime.powi(2);
        let y1_prime_sq = y1_prime.powi(2);
        let center_denom = rx_sq * y1_prime_sq + ry_sq * x1_prime_sq;
        if center_denom == 0.0 {
            return None;
        }
        let center_sign = if large_arc == sweep { -1.0 } else { 1.0 };
        let center_scale = ((rx_sq * ry_sq - rx_sq * y1_prime_sq - ry_sq * x1_prime_sq)
            / center_denom)
            .max(0.0)
            .sqrt()
            * center_sign;
        let cx_prime = center_scale * rx * y1_prime / ry;
        let cy_prime = center_scale * -ry * x1_prime / rx;
        let center = (
            cos_phi * cx_prime - sin_phi * cy_prime + (current.0 + to.0) / 2.0,
            sin_phi * cx_prime + cos_phi * cy_prime + (current.1 + to.1) / 2.0,
        );

        let start_vector = ((x1_prime - cx_prime) / rx, (y1_prime - cy_prime) / ry);
        let end_vector = ((-x1_prime - cx_prime) / rx, (-y1_prime - cy_prime) / ry);
        let start_angle = start_vector.1.atan2(start_vector.0);
        let mut sweep_angle = (start_vector.0 * end_vector.1 - start_vector.1 * end_vector.0)
            .atan2(start_vector.0 * end_vector.0 + start_vector.1 * end_vector.1);
        if !sweep && sweep_angle > 0.0 {
            sweep_angle -= 2.0 * std::f32::consts::PI;
        } else if sweep && sweep_angle < 0.0 {
            sweep_angle += 2.0 * std::f32::consts::PI;
        }
        let segment_count = (sweep_angle.abs() / (std::f32::consts::PI / 2.0)).ceil() as usize;
        if segment_count == 0 {
            return Some(Vec::new());
        }
        let segment_sweep = sweep_angle / segment_count as f32;
        let mut ops = Vec::new();
        for segment_index in 0..segment_count {
            let theta1 = start_angle + segment_sweep * segment_index as f32;
            let theta2 = theta1 + segment_sweep;
            let alpha = 4.0 / 3.0 * ((theta2 - theta1) / 4.0).tan();
            let ctrl1_unit = (
                theta1.cos() - alpha * theta1.sin(),
                theta1.sin() + alpha * theta1.cos(),
            );
            let ctrl2_unit = (
                theta2.cos() + alpha * theta2.sin(),
                theta2.sin() - alpha * theta2.cos(),
            );
            let end_unit = (theta2.cos(), theta2.sin());
            let ellipse_point = |point: (f32, f32)| -> (f32, f32) {
                (
                    center.0 + cos_phi * rx * point.0 - sin_phi * ry * point.1,
                    center.1 + sin_phi * rx * point.0 + cos_phi * ry * point.1,
                )
            };
            let ctrl1 = ellipse_point(ctrl1_unit);
            let ctrl2 = ellipse_point(ctrl2_unit);
            let segment_to = if segment_index + 1 == segment_count {
                to
            } else {
                ellipse_point(end_unit)
            };
            ops.push(SimpleSvgPathOp::CubicTo {
                ctrl1: normalize_point(apply_transform(transform, ctrl1.0, ctrl1.1)?),
                ctrl2: normalize_point(apply_transform(transform, ctrl2.0, ctrl2.1)?),
                to: normalize_point(apply_transform(transform, segment_to.0, segment_to.1)?),
            });
        }
        Some(ops)
    };
    let parse_path = |raw: &str, transform: SimpleSvgTransform| -> SimplePathParse {
        let mut tokens = Vec::new();
        let mut index = 0usize;
        while index < raw.len() {
            let ch = raw[index..].chars().next()?;
            if ch.is_whitespace() || ch == ',' {
                index += ch.len_utf8();
                continue;
            }
            if matches!(
                ch,
                'M' | 'm'
                    | 'L'
                    | 'l'
                    | 'H'
                    | 'h'
                    | 'V'
                    | 'v'
                    | 'C'
                    | 'c'
                    | 'S'
                    | 's'
                    | 'Q'
                    | 'q'
                    | 'T'
                    | 't'
                    | 'A'
                    | 'a'
                    | 'Z'
                    | 'z'
            ) {
                tokens.push(SimplePathToken::Command(ch));
                index += ch.len_utf8();
                continue;
            }
            let start = index;
            let mut allow_sign = true;
            let mut saw_digit = false;
            let mut saw_dot = false;
            let mut saw_exp = false;
            while index < raw.len() {
                let ch = raw[index..].chars().next()?;
                if ch.is_ascii_digit() {
                    saw_digit = true;
                    allow_sign = false;
                    index += ch.len_utf8();
                } else if matches!(ch, '+' | '-') && allow_sign {
                    allow_sign = false;
                    index += ch.len_utf8();
                } else if ch == '.' && !saw_dot && !saw_exp {
                    saw_dot = true;
                    allow_sign = false;
                    index += ch.len_utf8();
                } else if matches!(ch, 'e' | 'E') && saw_digit && !saw_exp {
                    saw_exp = true;
                    allow_sign = true;
                    index += ch.len_utf8();
                } else {
                    break;
                }
            }
            if index == start {
                return None;
            }
            tokens.push(SimplePathToken::Number(raw[start..index].parse().ok()?));
        }

        let mut token_index = 0usize;
        let mut command = None;
        let mut current = (0.0_f32, 0.0_f32);
        let mut subpath_start = None;
        let mut ops = Vec::new();
        let mut closed = false;
        let mut has_move = false;
        let mut last_cubic_ctrl2 = None;
        let mut last_quadratic_ctrl = None;
        while token_index < tokens.len() {
            if let SimplePathToken::Command(path_command) = tokens[token_index] {
                command = Some(path_command);
                token_index += 1;
                if matches!(path_command, 'Z' | 'z') {
                    let start = subpath_start?;
                    closed = true;
                    ops.push(SimpleSvgPathOp::Close);
                    current = start;
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = None;
                    continue;
                }
            }
            let current_command = command?;
            let mut next_number = || -> Option<f32> {
                let Some(SimplePathToken::Number(value)) = tokens.get(token_index).copied() else {
                    return None;
                };
                token_index += 1;
                Some(value)
            };
            match current_command {
                'M' | 'm' => {
                    let x = next_number()?;
                    let y = next_number()?;
                    if current_command == 'm' {
                        current.0 += x;
                        current.1 += y;
                    } else {
                        current = (x, y);
                    }
                    ops.push(SimpleSvgPathOp::MoveTo(normalize_point(apply_transform(
                        transform, current.0, current.1,
                    )?)));
                    subpath_start = Some(current);
                    has_move = true;
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = None;
                    command = Some(if current_command == 'm' { 'l' } else { 'L' });
                }
                'L' | 'l' => {
                    if !has_move {
                        return None;
                    }
                    let x = next_number()?;
                    let y = next_number()?;
                    if current_command == 'l' {
                        current.0 += x;
                        current.1 += y;
                    } else {
                        current = (x, y);
                    }
                    ops.push(SimpleSvgPathOp::LineTo(normalize_point(apply_transform(
                        transform, current.0, current.1,
                    )?)));
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = None;
                }
                'H' | 'h' => {
                    if !has_move {
                        return None;
                    }
                    let x = next_number()?;
                    if current_command == 'h' {
                        current.0 += x;
                    } else {
                        current.0 = x;
                    }
                    ops.push(SimpleSvgPathOp::LineTo(normalize_point(apply_transform(
                        transform, current.0, current.1,
                    )?)));
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = None;
                }
                'V' | 'v' => {
                    if !has_move {
                        return None;
                    }
                    let y = next_number()?;
                    if current_command == 'v' {
                        current.1 += y;
                    } else {
                        current.1 = y;
                    }
                    ops.push(SimpleSvgPathOp::LineTo(normalize_point(apply_transform(
                        transform, current.0, current.1,
                    )?)));
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = None;
                }
                'C' | 'c' => {
                    if !has_move {
                        return None;
                    }
                    let ctrl1_x = next_number()?;
                    let ctrl1_y = next_number()?;
                    let ctrl2_x = next_number()?;
                    let ctrl2_y = next_number()?;
                    let to_x = next_number()?;
                    let to_y = next_number()?;
                    let (ctrl1, ctrl2, to) = if current_command == 'c' {
                        (
                            (current.0 + ctrl1_x, current.1 + ctrl1_y),
                            (current.0 + ctrl2_x, current.1 + ctrl2_y),
                            (current.0 + to_x, current.1 + to_y),
                        )
                    } else {
                        ((ctrl1_x, ctrl1_y), (ctrl2_x, ctrl2_y), (to_x, to_y))
                    };
                    current = to;
                    ops.push(SimpleSvgPathOp::CubicTo {
                        ctrl1: normalize_point(apply_transform(transform, ctrl1.0, ctrl1.1)?),
                        ctrl2: normalize_point(apply_transform(transform, ctrl2.0, ctrl2.1)?),
                        to: normalize_point(apply_transform(transform, to.0, to.1)?),
                    });
                    last_cubic_ctrl2 = Some(ctrl2);
                    last_quadratic_ctrl = None;
                }
                'S' | 's' => {
                    if !has_move {
                        return None;
                    }
                    let ctrl2_x = next_number()?;
                    let ctrl2_y = next_number()?;
                    let to_x = next_number()?;
                    let to_y = next_number()?;
                    let ctrl1 = last_cubic_ctrl2
                        .map(|ctrl2: (f32, f32)| {
                            (2.0 * current.0 - ctrl2.0, 2.0 * current.1 - ctrl2.1)
                        })
                        .unwrap_or(current);
                    let (ctrl2, to) = if current_command == 's' {
                        (
                            (current.0 + ctrl2_x, current.1 + ctrl2_y),
                            (current.0 + to_x, current.1 + to_y),
                        )
                    } else {
                        ((ctrl2_x, ctrl2_y), (to_x, to_y))
                    };
                    current = to;
                    ops.push(SimpleSvgPathOp::CubicTo {
                        ctrl1: normalize_point(apply_transform(transform, ctrl1.0, ctrl1.1)?),
                        ctrl2: normalize_point(apply_transform(transform, ctrl2.0, ctrl2.1)?),
                        to: normalize_point(apply_transform(transform, to.0, to.1)?),
                    });
                    last_cubic_ctrl2 = Some(ctrl2);
                    last_quadratic_ctrl = None;
                }
                'Q' | 'q' => {
                    if !has_move {
                        return None;
                    }
                    let ctrl_x = next_number()?;
                    let ctrl_y = next_number()?;
                    let to_x = next_number()?;
                    let to_y = next_number()?;
                    let (ctrl, to) = if current_command == 'q' {
                        (
                            (current.0 + ctrl_x, current.1 + ctrl_y),
                            (current.0 + to_x, current.1 + to_y),
                        )
                    } else {
                        ((ctrl_x, ctrl_y), (to_x, to_y))
                    };
                    let ctrl1 = (
                        current.0 + (2.0 / 3.0) * (ctrl.0 - current.0),
                        current.1 + (2.0 / 3.0) * (ctrl.1 - current.1),
                    );
                    let ctrl2 = (
                        to.0 + (2.0 / 3.0) * (ctrl.0 - to.0),
                        to.1 + (2.0 / 3.0) * (ctrl.1 - to.1),
                    );
                    current = to;
                    ops.push(SimpleSvgPathOp::CubicTo {
                        ctrl1: normalize_point(apply_transform(transform, ctrl1.0, ctrl1.1)?),
                        ctrl2: normalize_point(apply_transform(transform, ctrl2.0, ctrl2.1)?),
                        to: normalize_point(apply_transform(transform, to.0, to.1)?),
                    });
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = Some(ctrl);
                }
                'T' | 't' => {
                    if !has_move {
                        return None;
                    }
                    let to_x = next_number()?;
                    let to_y = next_number()?;
                    let ctrl = last_quadratic_ctrl
                        .map(|ctrl: (f32, f32)| {
                            (2.0 * current.0 - ctrl.0, 2.0 * current.1 - ctrl.1)
                        })
                        .unwrap_or(current);
                    let to = if current_command == 't' {
                        (current.0 + to_x, current.1 + to_y)
                    } else {
                        (to_x, to_y)
                    };
                    let ctrl1 = (
                        current.0 + (2.0 / 3.0) * (ctrl.0 - current.0),
                        current.1 + (2.0 / 3.0) * (ctrl.1 - current.1),
                    );
                    let ctrl2 = (
                        to.0 + (2.0 / 3.0) * (ctrl.0 - to.0),
                        to.1 + (2.0 / 3.0) * (ctrl.1 - to.1),
                    );
                    current = to;
                    ops.push(SimpleSvgPathOp::CubicTo {
                        ctrl1: normalize_point(apply_transform(transform, ctrl1.0, ctrl1.1)?),
                        ctrl2: normalize_point(apply_transform(transform, ctrl2.0, ctrl2.1)?),
                        to: normalize_point(apply_transform(transform, to.0, to.1)?),
                    });
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = Some(ctrl);
                }
                'A' | 'a' => {
                    if !has_move {
                        return None;
                    }
                    let rx = next_number()?;
                    let ry = next_number()?;
                    let x_axis_rotation = next_number()?;
                    let large_arc = next_number()?.abs() > f32::EPSILON;
                    let sweep = next_number()?.abs() > f32::EPSILON;
                    let x = next_number()?;
                    let y = next_number()?;
                    let to = if current_command == 'a' {
                        (current.0 + x, current.1 + y)
                    } else {
                        (x, y)
                    };
                    ops.extend(arc_to_cubics(SimpleSvgArcCommand {
                        current,
                        rx,
                        ry,
                        x_axis_rotation,
                        large_arc,
                        sweep,
                        to,
                        transform,
                    })?);
                    current = to;
                    last_cubic_ctrl2 = None;
                    last_quadratic_ctrl = None;
                }
                _ => return None,
            }
        }
        if ops.len() < 2 {
            return None;
        }
        Some((ops, closed))
    };
    let svg_content_start = tag_start + tag_end + 1;
    let svg_content_end = text[svg_content_start..]
        .find("</svg>")
        .map(|relative| svg_content_start + relative)
        .unwrap_or(text.len());
    let svg_content = &text[svg_content_start..svg_content_end];
    let mut defs_ranges = Vec::new();
    let mut defs_stack = Vec::new();
    let mut defs_search_index = 0usize;
    while let Some(relative) = svg_content[defs_search_index..].find('<') {
        let defs_tag_start = defs_search_index + relative;
        let defs_tag_tail = &svg_content[defs_tag_start..];
        let Some(defs_tag_end) = defs_tag_tail.find('>') else {
            break;
        };
        let is_defs_close = defs_tag_tail.starts_with("</defs")
            && defs_tag_tail[6..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ch == '>');
        let is_defs_open = defs_tag_tail.starts_with("<defs")
            && defs_tag_tail[5..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '>' | '/'));
        if is_defs_close {
            if let Some(start) = defs_stack.pop() {
                defs_ranges.push((start, defs_tag_start + defs_tag_end + 1));
            }
        } else if is_defs_open && !defs_tag_tail[..defs_tag_end].trim_end().ends_with('/') {
            defs_stack.push(defs_tag_start);
        }
        defs_search_index = defs_tag_start + defs_tag_end + 1;
    }
    while let Some(start) = defs_stack.pop() {
        defs_ranges.push((start, svg_content.len()));
    }
    let in_defs = |element_start: usize| {
        defs_ranges
            .iter()
            .any(|(start, end)| *start <= element_start && element_start < *end)
    };
    #[derive(Debug, Clone, Copy)]
    struct SimpleSvgGroupTransform {
        content_start: usize,
        content_end: usize,
        transform: Option<SimpleSvgTransform>,
        presentation: SimpleSvgPresentation,
    }
    let mut group_transforms = Vec::new();
    let mut group_stack: Vec<(usize, Option<SimpleSvgTransform>, SimpleSvgPresentation)> =
        Vec::new();
    let mut group_search_index = 0usize;
    while let Some(relative) = svg_content[group_search_index..].find('<') {
        let group_tag_start = group_search_index + relative;
        let group_tag_tail = &svg_content[group_tag_start..];
        let Some(group_tag_end) = group_tag_tail.find('>') else {
            break;
        };
        let is_group_close = group_tag_tail.starts_with("</g")
            && group_tag_tail[3..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ch == '>');
        let is_group_open = group_tag_tail.starts_with("<g")
            && group_tag_tail[2..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '>' | '/'));
        if is_group_close {
            if let Some((content_start, transform, presentation)) = group_stack.pop()
                && content_start <= group_tag_start
            {
                group_transforms.push(SimpleSvgGroupTransform {
                    content_start,
                    content_end: group_tag_start,
                    transform,
                    presentation,
                });
            }
        } else if is_group_open {
            let group_tag = &group_tag_tail[..group_tag_end];
            let local_transform = parse_transform(group_tag);
            let local_presentation = parse_presentation(group_tag);
            let presentation = inherit_presentation(
                group_stack
                    .last()
                    .map(|(_, _, parent_presentation)| *parent_presentation)
                    .unwrap_or(root_presentation),
                local_presentation,
            );
            let transform = if let Some((_, Some(parent_transform), _)) = group_stack.last() {
                local_transform.map(|local| {
                    compose_transform(local, *parent_transform, parent_transform.stroke_scale)
                })
            } else if group_stack.last().is_some() {
                None
            } else {
                local_transform
            };
            if !group_tag.trim_end().ends_with('/') {
                group_stack.push((group_tag_start + group_tag_end + 1, transform, presentation));
            }
        }
        group_search_index = group_tag_start + group_tag_end + 1;
    }
    while let Some((content_start, transform, presentation)) = group_stack.pop() {
        group_transforms.push(SimpleSvgGroupTransform {
            content_start,
            content_end: svg_content.len(),
            transform,
            presentation,
        });
    }
    let group_state_for =
        |element_start: usize| -> (Option<SimpleSvgTransform>, SimpleSvgPresentation) {
            let mut selected_group: Option<&SimpleSvgGroupTransform> = None;
            for group in &group_transforms {
                if group.content_start <= element_start && element_start < group.content_end {
                    selected_group = match selected_group {
                        Some(selected) if selected.content_start > group.content_start => {
                            Some(selected)
                        }
                        _ => Some(group),
                    };
                }
            }
            selected_group
                .map(|group| (group.transform, group.presentation))
                .unwrap_or((Some(identity_transform), root_presentation))
        };
    let parse_element_state =
        |tag: &str, element_start: usize| -> Option<(SimpleSvgTransform, SimpleSvgPresentation)> {
            let (group_transform, group_presentation) = group_state_for(element_start);
            let group_transform = group_transform?;
            let element_transform = parse_transform(tag)?;
            let presentation = inherit_presentation(group_presentation, parse_presentation(tag));
            Some((
                compose_transform(
                    element_transform,
                    group_transform,
                    group_transform.stroke_scale,
                ),
                presentation,
            ))
        };
    let mut rects = Vec::new();
    let mut rect_polys = Vec::new();
    let mut shape_paths = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<rect") {
        let rect_start = search_index + relative;
        let rect_tail = &svg_content[rect_start..];
        if !is_start_tag_named(rect_tail, "rect") {
            search_index = rect_start + "<rect".len();
            continue;
        }
        let Some(rect_end) = rect_tail.find('>') else {
            break;
        };
        if in_defs(rect_start) {
            search_index = rect_start + rect_end + 1;
            continue;
        }
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
            let Some((transform, presentation)) = parse_element_state(rect_tag, rect_start) else {
                search_index = rect_start + rect_end + 1;
                continue;
            };
            let fill = fill_paint(presentation, Some((0.0, 0.0, 0.0)));
            let stroke = stroke_paint(presentation);
            let rx_raw = attr_value(rect_tag, "rx")
                .as_deref()
                .and_then(parse_number_prefix);
            let ry_raw = attr_value(rect_tag, "ry")
                .as_deref()
                .and_then(parse_number_prefix);
            let rounded_radii = match (rx_raw, ry_raw) {
                (Some(rx), Some(ry)) if rx > 0.0 && ry > 0.0 => Some((rx, ry)),
                (Some(radius), None) | (None, Some(radius)) if radius > 0.0 => {
                    Some((radius, radius))
                }
                _ => None,
            };
            if let Some((rx, ry)) = rounded_radii {
                let rx = rx.min(width / 2.0);
                let ry = ry.min(height / 2.0);
                if rx > 0.0 && ry > 0.0 && (fill.is_some() || stroke.is_some()) {
                    let kappa = 0.552_284_8_f32;
                    let transform_point = |x: f32, y: f32| -> Option<(f32, f32)> {
                        Some(normalize_point(apply_transform(transform, x, y)?))
                    };
                    let Some(start) = transform_point(x + rx, y) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(top_end) = transform_point(x + width - rx, y) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(top_right_ctrl1) = transform_point(x + width - rx + kappa * rx, y)
                    else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(top_right_ctrl2) = transform_point(x + width, y + ry - kappa * ry)
                    else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(right_start) = transform_point(x + width, y + ry) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(right_end) = transform_point(x + width, y + height - ry) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(bottom_right_ctrl1) =
                        transform_point(x + width, y + height - ry + kappa * ry)
                    else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(bottom_right_ctrl2) =
                        transform_point(x + width - rx + kappa * rx, y + height)
                    else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(bottom_start) = transform_point(x + width - rx, y + height) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(bottom_end) = transform_point(x + rx, y + height) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(bottom_left_ctrl1) = transform_point(x + rx - kappa * rx, y + height)
                    else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(bottom_left_ctrl2) = transform_point(x, y + height - ry + kappa * ry)
                    else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(left_start) = transform_point(x, y + height - ry) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(left_end) = transform_point(x, y + ry) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(top_left_ctrl1) = transform_point(x, y + ry - kappa * ry) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    let Some(top_left_ctrl2) = transform_point(x + rx - kappa * rx, y) else {
                        search_index = rect_start + rect_end + 1;
                        continue;
                    };
                    shape_paths.push(SimpleSvgPath {
                        ops: vec![
                            SimpleSvgPathOp::MoveTo(start),
                            SimpleSvgPathOp::LineTo(top_end),
                            SimpleSvgPathOp::CubicTo {
                                ctrl1: top_right_ctrl1,
                                ctrl2: top_right_ctrl2,
                                to: right_start,
                            },
                            SimpleSvgPathOp::LineTo(right_end),
                            SimpleSvgPathOp::CubicTo {
                                ctrl1: bottom_right_ctrl1,
                                ctrl2: bottom_right_ctrl2,
                                to: bottom_start,
                            },
                            SimpleSvgPathOp::LineTo(bottom_end),
                            SimpleSvgPathOp::CubicTo {
                                ctrl1: bottom_left_ctrl1,
                                ctrl2: bottom_left_ctrl2,
                                to: left_start,
                            },
                            SimpleSvgPathOp::LineTo(left_end),
                            SimpleSvgPathOp::CubicTo {
                                ctrl1: top_left_ctrl1,
                                ctrl2: top_left_ctrl2,
                                to: start,
                            },
                            SimpleSvgPathOp::Close,
                        ],
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: stroke_dasharray_ratio(presentation),
                        stroke_style: stroke_style(presentation),
                    });
                }
                search_index = rect_start + rect_end + 1;
                continue;
            }
            if !transform.axis_aligned {
                let Some(corner_a) = apply_transform(transform, x, y) else {
                    search_index = rect_start + rect_end + 1;
                    continue;
                };
                let Some(corner_b) = apply_transform(transform, x + width, y) else {
                    search_index = rect_start + rect_end + 1;
                    continue;
                };
                let Some(corner_c) = apply_transform(transform, x + width, y + height) else {
                    search_index = rect_start + rect_end + 1;
                    continue;
                };
                let Some(corner_d) = apply_transform(transform, x, y + height) else {
                    search_index = rect_start + rect_end + 1;
                    continue;
                };
                if fill.is_some() || stroke.is_some() {
                    rect_polys.push(SimpleSvgPoly {
                        points: vec![
                            normalize_point(corner_a),
                            normalize_point(corner_b),
                            normalize_point(corner_c),
                            normalize_point(corner_d),
                        ],
                        closed: true,
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: stroke_dasharray_ratio(presentation),
                        stroke_style: stroke_style(presentation),
                    });
                }
                search_index = rect_start + rect_end + 1;
                continue;
            }
            let Some(corner_a) = apply_transform(transform, x, y) else {
                search_index = rect_start + rect_end + 1;
                continue;
            };
            let Some(corner_b) = apply_transform(transform, x + width, y + height) else {
                search_index = rect_start + rect_end + 1;
                continue;
            };
            let x = corner_a.0.min(corner_b.0);
            let y = corner_a.1.min(corner_b.1);
            let width = (corner_b.0 - corner_a.0).abs();
            let height = (corner_b.1 - corner_a.1).abs();
            if width > 0.0 && height > 0.0 && (fill.is_some() || stroke.is_some()) {
                rects.push(SimpleSvgRect {
                    x_ratio: (x - view_box.0) / view_box.2,
                    y_ratio: (y - view_box.1) / view_box.3,
                    width_ratio: width / view_box.2,
                    height_ratio: height / view_box.3,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: stroke_dasharray_ratio(presentation),
                    stroke_style: stroke_style(presentation),
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
        if !is_start_tag_named(line_tail, "line") {
            search_index = line_start + "<line".len();
            continue;
        }
        let Some(line_end) = line_tail.find('>') else {
            break;
        };
        if in_defs(line_start) {
            search_index = line_start + line_end + 1;
            continue;
        }
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
        let Some((transform, presentation)) = parse_element_state(line_tag, line_start) else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some((x1, y1)) = apply_transform(transform, x1, y1) else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some((x2, y2)) = apply_transform(transform, x2, y2) else {
            search_index = line_start + line_end + 1;
            continue;
        };
        if (x1 != x2 || y1 != y2)
            && let Some(stroke) = stroke_paint(presentation)
        {
            lines.push(SimpleSvgLine {
                x1_ratio: (x1 - view_box.0) / view_box.2,
                y1_ratio: (y1 - view_box.1) / view_box.3,
                x2_ratio: (x2 - view_box.0) / view_box.2,
                y2_ratio: (y2 - view_box.1) / view_box.3,
                stroke,
                stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                stroke_dasharray: stroke_dasharray_ratio(presentation),
                stroke_style: stroke_style(presentation),
            });
        }
        search_index = line_start + line_end + 1;
    }
    let mut ellipses = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<circle") {
        let circle_start = search_index + relative;
        let circle_tail = &svg_content[circle_start..];
        if !is_start_tag_named(circle_tail, "circle") {
            search_index = circle_start + "<circle".len();
            continue;
        }
        let Some(circle_end) = circle_tail.find('>') else {
            break;
        };
        if in_defs(circle_start) {
            search_index = circle_start + circle_end + 1;
            continue;
        }
        let circle_tag = &circle_tail[..circle_end];
        let cx = attr_value(circle_tag, "cx")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let cy = attr_value(circle_tag, "cy")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let Some(radius) = attr_value(circle_tag, "r")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = circle_start + circle_end + 1;
            continue;
        };
        if radius > 0.0 {
            let Some((transform, presentation)) = parse_element_state(circle_tag, circle_start)
            else {
                search_index = circle_start + circle_end + 1;
                continue;
            };
            let fill = fill_paint(presentation, Some((0.0, 0.0, 0.0)));
            let stroke = stroke_paint(presentation);
            if !transform.axis_aligned {
                if (fill.is_some() || stroke.is_some())
                    && let Some(ops) = ellipse_path_ops(cx, cy, radius, radius, transform)
                {
                    shape_paths.push(SimpleSvgPath {
                        ops,
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: stroke_dasharray_ratio(presentation),
                        stroke_style: stroke_style(presentation),
                    });
                }
                search_index = circle_start + circle_end + 1;
                continue;
            }
            let Some(center) = apply_transform(transform, cx, cy) else {
                search_index = circle_start + circle_end + 1;
                continue;
            };
            let Some(radius_x_point) = apply_transform(transform, cx + radius, cy) else {
                search_index = circle_start + circle_end + 1;
                continue;
            };
            let Some(radius_y_point) = apply_transform(transform, cx, cy + radius) else {
                search_index = circle_start + circle_end + 1;
                continue;
            };
            let rx = (radius_x_point.0 - center.0).abs();
            let ry = (radius_y_point.1 - center.1).abs();
            if rx > 0.0 && ry > 0.0 && (fill.is_some() || stroke.is_some()) {
                ellipses.push(SimpleSvgEllipse {
                    cx_ratio: (center.0 - view_box.0) / view_box.2,
                    cy_ratio: (center.1 - view_box.1) / view_box.3,
                    rx_ratio: rx / view_box.2,
                    ry_ratio: ry / view_box.3,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: stroke_dasharray_ratio(presentation),
                    stroke_style: stroke_style(presentation),
                });
            }
        }
        search_index = circle_start + circle_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<ellipse") {
        let ellipse_start = search_index + relative;
        let ellipse_tail = &svg_content[ellipse_start..];
        if !is_start_tag_named(ellipse_tail, "ellipse") {
            search_index = ellipse_start + "<ellipse".len();
            continue;
        }
        let Some(ellipse_end) = ellipse_tail.find('>') else {
            break;
        };
        if in_defs(ellipse_start) {
            search_index = ellipse_start + ellipse_end + 1;
            continue;
        }
        let ellipse_tag = &ellipse_tail[..ellipse_end];
        let cx = attr_value(ellipse_tag, "cx")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let cy = attr_value(ellipse_tag, "cy")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let Some(rx) = attr_value(ellipse_tag, "rx")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = ellipse_start + ellipse_end + 1;
            continue;
        };
        let Some(ry) = attr_value(ellipse_tag, "ry")
            .as_deref()
            .and_then(parse_number_prefix)
        else {
            search_index = ellipse_start + ellipse_end + 1;
            continue;
        };
        if rx > 0.0 && ry > 0.0 {
            let Some((transform, presentation)) = parse_element_state(ellipse_tag, ellipse_start)
            else {
                search_index = ellipse_start + ellipse_end + 1;
                continue;
            };
            let fill = fill_paint(presentation, Some((0.0, 0.0, 0.0)));
            let stroke = stroke_paint(presentation);
            if !transform.axis_aligned {
                if (fill.is_some() || stroke.is_some())
                    && let Some(ops) = ellipse_path_ops(cx, cy, rx, ry, transform)
                {
                    shape_paths.push(SimpleSvgPath {
                        ops,
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: stroke_dasharray_ratio(presentation),
                        stroke_style: stroke_style(presentation),
                    });
                }
                search_index = ellipse_start + ellipse_end + 1;
                continue;
            }
            let Some(center) = apply_transform(transform, cx, cy) else {
                search_index = ellipse_start + ellipse_end + 1;
                continue;
            };
            let Some(radius_x_point) = apply_transform(transform, cx + rx, cy) else {
                search_index = ellipse_start + ellipse_end + 1;
                continue;
            };
            let Some(radius_y_point) = apply_transform(transform, cx, cy + ry) else {
                search_index = ellipse_start + ellipse_end + 1;
                continue;
            };
            let rx = (radius_x_point.0 - center.0).abs();
            let ry = (radius_y_point.1 - center.1).abs();
            if rx > 0.0 && ry > 0.0 && (fill.is_some() || stroke.is_some()) {
                ellipses.push(SimpleSvgEllipse {
                    cx_ratio: (center.0 - view_box.0) / view_box.2,
                    cy_ratio: (center.1 - view_box.1) / view_box.3,
                    rx_ratio: rx / view_box.2,
                    ry_ratio: ry / view_box.3,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: stroke_dasharray_ratio(presentation),
                    stroke_style: stroke_style(presentation),
                });
            }
        }
        search_index = ellipse_start + ellipse_end + 1;
    }
    let mut polys = rect_polys;
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<polyline") {
        let poly_start = search_index + relative;
        let poly_tail = &svg_content[poly_start..];
        if !is_start_tag_named(poly_tail, "polyline") {
            search_index = poly_start + "<polyline".len();
            continue;
        }
        let Some(poly_end) = poly_tail.find('>') else {
            break;
        };
        if in_defs(poly_start) {
            search_index = poly_start + poly_end + 1;
            continue;
        }
        let poly_tag = &poly_tail[..poly_end];
        if let Some((transform, presentation)) = parse_element_state(poly_tag, poly_start)
            && let Some(points) = attr_value(poly_tag, "points")
                .as_deref()
                .and_then(|points| parse_points(points, transform))
        {
            let fill = fill_paint(presentation, None);
            let stroke = stroke_paint(presentation);
            if fill.is_some() || stroke.is_some() {
                polys.push(SimpleSvgPoly {
                    points,
                    closed: false,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: stroke_dasharray_ratio(presentation),
                    stroke_style: stroke_style(presentation),
                });
            }
        }
        search_index = poly_start + poly_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<polygon") {
        let poly_start = search_index + relative;
        let poly_tail = &svg_content[poly_start..];
        if !is_start_tag_named(poly_tail, "polygon") {
            search_index = poly_start + "<polygon".len();
            continue;
        }
        let Some(poly_end) = poly_tail.find('>') else {
            break;
        };
        if in_defs(poly_start) {
            search_index = poly_start + poly_end + 1;
            continue;
        }
        let poly_tag = &poly_tail[..poly_end];
        if let Some((transform, presentation)) = parse_element_state(poly_tag, poly_start)
            && let Some(points) = attr_value(poly_tag, "points")
                .as_deref()
                .and_then(|points| parse_points(points, transform))
        {
            let fill = fill_paint(presentation, Some((0.0, 0.0, 0.0)));
            let stroke = stroke_paint(presentation);
            if fill.is_some() || stroke.is_some() {
                polys.push(SimpleSvgPoly {
                    points,
                    closed: true,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: stroke_dasharray_ratio(presentation),
                    stroke_style: stroke_style(presentation),
                });
            }
        }
        search_index = poly_start + poly_end + 1;
    }
    let mut paths = shape_paths;
    let push_simple_svg_path = |paths: &mut Vec<SimpleSvgPath>,
                                transform: SimpleSvgTransform,
                                presentation: SimpleSvgPresentation,
                                path_data: &str|
     -> bool {
        let Some((ops, closed)) = parse_path(path_data, transform) else {
            return false;
        };
        let fill = fill_paint(
            presentation,
            if closed { Some((0.0, 0.0, 0.0)) } else { None },
        );
        let stroke = stroke_paint(presentation);
        if fill.is_none() && stroke.is_none() {
            return false;
        }
        paths.push(SimpleSvgPath {
            ops,
            fill,
            fill_rule: fill_rule(presentation),
            stroke,
            stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
            stroke_dasharray: stroke_dasharray_ratio(presentation),
            stroke_style: stroke_style(presentation),
        });
        true
    };
    #[derive(Debug, Clone)]
    struct SimpleSvgPathLikeDefinition<'a> {
        id: String,
        tag: &'a str,
        start: usize,
        path_data: String,
    }
    let mut path_like_definitions = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<path") {
        let path_start = search_index + relative;
        let path_tail = &svg_content[path_start..];
        if !is_start_tag_named(path_tail, "path") {
            search_index = path_start + "<path".len();
            continue;
        }
        let Some(path_end) = path_tail.find('>') else {
            break;
        };
        if !in_defs(path_start) {
            search_index = path_start + path_end + 1;
            continue;
        }
        let path_tag = &path_tail[..path_end];
        if let (Some(id), Some(path_data)) = (attr_value(path_tag, "id"), attr_value(path_tag, "d"))
            && !id.trim().is_empty()
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: path_tag,
                start: path_start,
                path_data,
            });
        }
        search_index = path_start + path_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<rect") {
        let rect_start = search_index + relative;
        let rect_tail = &svg_content[rect_start..];
        if !is_start_tag_named(rect_tail, "rect") {
            search_index = rect_start + "<rect".len();
            continue;
        }
        let Some(rect_end) = rect_tail.find('>') else {
            break;
        };
        if !in_defs(rect_start) {
            search_index = rect_start + rect_end + 1;
            continue;
        }
        let rect_tag = &rect_tail[..rect_end];
        let has_rounded_corner = attr_value(rect_tag, "rx")
            .as_deref()
            .and_then(parse_number_prefix)
            .is_some_and(|radius| radius > 0.0)
            || attr_value(rect_tag, "ry")
                .as_deref()
                .and_then(parse_number_prefix)
                .is_some_and(|radius| radius > 0.0);
        if has_rounded_corner {
            search_index = rect_start + rect_end + 1;
            continue;
        }
        let x = attr_value(rect_tag, "x")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let y = attr_value(rect_tag, "y")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        if let (Some(id), Some(width), Some(height)) = (
            attr_value(rect_tag, "id"),
            attr_value(rect_tag, "width")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(rect_tag, "height")
                .as_deref()
                .and_then(parse_number_prefix),
        ) && !id.trim().is_empty()
            && let Some(path_data) = rect_path_data(x, y, width, height)
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: rect_tag,
                start: rect_start,
                path_data,
            });
        }
        search_index = rect_start + rect_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<circle") {
        let circle_start = search_index + relative;
        let circle_tail = &svg_content[circle_start..];
        if !is_start_tag_named(circle_tail, "circle") {
            search_index = circle_start + "<circle".len();
            continue;
        }
        let Some(circle_end) = circle_tail.find('>') else {
            break;
        };
        if !in_defs(circle_start) {
            search_index = circle_start + circle_end + 1;
            continue;
        }
        let circle_tag = &circle_tail[..circle_end];
        let cx = attr_value(circle_tag, "cx")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let cy = attr_value(circle_tag, "cy")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        if let (Some(id), Some(radius)) = (
            attr_value(circle_tag, "id"),
            attr_value(circle_tag, "r")
                .as_deref()
                .and_then(parse_number_prefix),
        ) && !id.trim().is_empty()
            && let Some(path_data) = ellipse_path_data(cx, cy, radius, radius)
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: circle_tag,
                start: circle_start,
                path_data,
            });
        }
        search_index = circle_start + circle_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<ellipse") {
        let ellipse_start = search_index + relative;
        let ellipse_tail = &svg_content[ellipse_start..];
        if !is_start_tag_named(ellipse_tail, "ellipse") {
            search_index = ellipse_start + "<ellipse".len();
            continue;
        }
        let Some(ellipse_end) = ellipse_tail.find('>') else {
            break;
        };
        if !in_defs(ellipse_start) {
            search_index = ellipse_start + ellipse_end + 1;
            continue;
        }
        let ellipse_tag = &ellipse_tail[..ellipse_end];
        let cx = attr_value(ellipse_tag, "cx")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let cy = attr_value(ellipse_tag, "cy")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        if let (Some(id), Some(rx), Some(ry)) = (
            attr_value(ellipse_tag, "id"),
            attr_value(ellipse_tag, "rx")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(ellipse_tag, "ry")
                .as_deref()
                .and_then(parse_number_prefix),
        ) && !id.trim().is_empty()
            && let Some(path_data) = ellipse_path_data(cx, cy, rx, ry)
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: ellipse_tag,
                start: ellipse_start,
                path_data,
            });
        }
        search_index = ellipse_start + ellipse_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<line") {
        let line_start = search_index + relative;
        let line_tail = &svg_content[line_start..];
        if !is_start_tag_named(line_tail, "line") {
            search_index = line_start + "<line".len();
            continue;
        }
        let Some(line_end) = line_tail.find('>') else {
            break;
        };
        if !in_defs(line_start) {
            search_index = line_start + line_end + 1;
            continue;
        }
        let line_tag = &line_tail[..line_end];
        if let (Some(id), Some(x1), Some(y1), Some(x2), Some(y2)) = (
            attr_value(line_tag, "id"),
            attr_value(line_tag, "x1")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(line_tag, "y1")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(line_tag, "x2")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(line_tag, "y2")
                .as_deref()
                .and_then(parse_number_prefix),
        ) && !id.trim().is_empty()
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: line_tag,
                start: line_start,
                path_data: format!("M {} {} L {} {}", x1, y1, x2, y2),
            });
        }
        search_index = line_start + line_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<polyline") {
        let poly_start = search_index + relative;
        let poly_tail = &svg_content[poly_start..];
        if !is_start_tag_named(poly_tail, "polyline") {
            search_index = poly_start + "<polyline".len();
            continue;
        }
        let Some(poly_end) = poly_tail.find('>') else {
            break;
        };
        if !in_defs(poly_start) {
            search_index = poly_start + poly_end + 1;
            continue;
        }
        let poly_tag = &poly_tail[..poly_end];
        if let (Some(id), Some(points_raw)) =
            (attr_value(poly_tag, "id"), attr_value(poly_tag, "points"))
            && !id.trim().is_empty()
            && let Some(points) = parse_raw_points(&points_raw)
            && let Some(path_data) = path_data_from_points(&points, false)
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: poly_tag,
                start: poly_start,
                path_data,
            });
        }
        search_index = poly_start + poly_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<polygon") {
        let poly_start = search_index + relative;
        let poly_tail = &svg_content[poly_start..];
        if !is_start_tag_named(poly_tail, "polygon") {
            search_index = poly_start + "<polygon".len();
            continue;
        }
        let Some(poly_end) = poly_tail.find('>') else {
            break;
        };
        if !in_defs(poly_start) {
            search_index = poly_start + poly_end + 1;
            continue;
        }
        let poly_tag = &poly_tail[..poly_end];
        if let (Some(id), Some(points_raw)) =
            (attr_value(poly_tag, "id"), attr_value(poly_tag, "points"))
            && !id.trim().is_empty()
            && let Some(points) = parse_raw_points(&points_raw)
            && let Some(path_data) = path_data_from_points(&points, true)
        {
            path_like_definitions.push(SimpleSvgPathLikeDefinition {
                id,
                tag: poly_tag,
                start: poly_start,
                path_data,
            });
        }
        search_index = poly_start + poly_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<path") {
        let path_start = search_index + relative;
        let path_tail = &svg_content[path_start..];
        if !is_start_tag_named(path_tail, "path") {
            search_index = path_start + "<path".len();
            continue;
        }
        let Some(path_end) = path_tail.find('>') else {
            break;
        };
        if in_defs(path_start) {
            search_index = path_start + path_end + 1;
            continue;
        }
        let path_tag = &path_tail[..path_end];
        if let Some((transform, presentation)) = parse_element_state(path_tag, path_start)
            && let Some(path_data) = attr_value(path_tag, "d")
        {
            push_simple_svg_path(&mut paths, transform, presentation, &path_data);
        }
        search_index = path_start + path_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<use") {
        let use_start = search_index + relative;
        let use_tail = &svg_content[use_start..];
        if !is_start_tag_named(use_tail, "use") {
            search_index = use_start + "<use".len();
            continue;
        }
        let Some(use_end) = use_tail.find('>') else {
            break;
        };
        if in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some(definition) = path_like_definitions
            .iter()
            .find(|definition| definition.id == reference_id)
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some((definition_transform, definition_presentation)) =
            parse_element_state(definition.tag, definition.start)
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let x = attr_value(use_tag, "x")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let y = attr_value(use_tag, "y")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let translated_use_transform = compose_transform(
            SimpleSvgTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        let transform = compose_transform(
            definition_transform,
            translated_use_transform,
            translated_use_transform.stroke_scale,
        );
        let presentation = inherit_presentation(definition_presentation, use_presentation);
        push_simple_svg_path(&mut paths, transform, presentation, &definition.path_data);
        search_index = use_start + use_end + 1;
    }
    let decode_xml_text = |raw: &str| {
        raw.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
    };
    let mut texts = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<text") {
        let text_start = search_index + relative;
        let text_tail = &svg_content[text_start..];
        if !is_start_tag_named(text_tail, "text") {
            search_index = text_start + "<text".len();
            continue;
        }
        let Some(text_tag_end) = text_tail.find('>') else {
            break;
        };
        if in_defs(text_start) {
            search_index = text_start + text_tag_end + 1;
            continue;
        }
        let text_tag = &text_tail[..text_tag_end];
        let text_body_start = text_start + text_tag_end + 1;
        let Some(text_body_end_relative) = svg_content[text_body_start..].find("</text>") else {
            search_index = text_body_start;
            continue;
        };
        let text_body_end = text_body_start + text_body_end_relative;
        let text_body = svg_content[text_body_start..text_body_end].trim();
        if text_body.is_empty() {
            search_index = text_body_end + "</text>".len();
            continue;
        }
        let Some((transform, presentation)) = parse_element_state(text_tag, text_start) else {
            search_index = text_body_end + "</text>".len();
            continue;
        };
        let x = attr_value(text_tag, "x")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let y = attr_value(text_tag, "y")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let dx = attr_value(text_tag, "dx")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let dy = attr_value(text_tag, "dy")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let local_font_size = resolved_font_size(presentation);
        let text_x = x + dx;
        let text_raw_y = y + dy;
        let text_y = text_raw_y
            + baseline_y_offset(presentation, local_font_size)
            + baseline_shift_y_offset(presentation, local_font_size);
        let font_size = local_font_size * transform.stroke_scale;
        let Some(point) = apply_transform(transform, text_x, text_y).map(normalize_point) else {
            search_index = text_body_end + "</text>".len();
            continue;
        };
        let resolved_texts = if !text_body.contains('<') {
            vec![(point, font_size, presentation, decode_xml_text(text_body))]
        } else {
            let mut remaining = text_body;
            let mut current_x = text_x;
            let mut current_y = text_raw_y;
            let mut tspan_texts = Vec::new();
            let mut valid_tspans = true;
            while !remaining.trim().is_empty() {
                let Some(tspan_start) = remaining.find("<tspan") else {
                    valid_tspans = false;
                    break;
                };
                if !remaining[..tspan_start].trim().is_empty() {
                    valid_tspans = false;
                    break;
                }
                let tspan_tail = &remaining[tspan_start..];
                if !is_start_tag_named(tspan_tail, "tspan") {
                    valid_tspans = false;
                    break;
                }
                let Some(tspan_tag_end) = tspan_tail.find('>') else {
                    valid_tspans = false;
                    break;
                };
                let tspan_tag = &tspan_tail[..tspan_tag_end];
                let tspan_body_start = tspan_tag_end + 1;
                let Some(tspan_body_end_relative) = tspan_tail[tspan_body_start..].find("</tspan>")
                else {
                    valid_tspans = false;
                    break;
                };
                let tspan_body_end = tspan_body_start + tspan_body_end_relative;
                let tspan_body = tspan_tail[tspan_body_start..tspan_body_end].trim();
                if tspan_body.is_empty() || tspan_body.contains('<') {
                    valid_tspans = false;
                    break;
                }
                let tspan_x = attr_value(tspan_tag, "x")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(current_x);
                let tspan_y = attr_value(tspan_tag, "y")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(current_y);
                let tspan_dx = attr_value(tspan_tag, "dx")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let tspan_dy = attr_value(tspan_tag, "dy")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let tspan_presentation =
                    inherit_presentation(presentation, parse_presentation(tspan_tag));
                let tspan_local_font_size = resolved_font_size(tspan_presentation);
                let tspan_font_size = tspan_local_font_size * transform.stroke_scale;
                let tspan_x = tspan_x + tspan_dx;
                let tspan_y = tspan_y + tspan_dy;
                let tspan_baseline_y = tspan_y
                    + baseline_y_offset(tspan_presentation, tspan_local_font_size)
                    + baseline_shift_y_offset(tspan_presentation, tspan_local_font_size);
                let Some(point) =
                    apply_transform(transform, tspan_x, tspan_baseline_y).map(normalize_point)
                else {
                    valid_tspans = false;
                    break;
                };
                tspan_texts.push((
                    point,
                    tspan_font_size,
                    tspan_presentation,
                    decode_xml_text(tspan_body),
                ));
                current_x = tspan_x;
                current_y = tspan_y;
                let tspan_close_end = tspan_body_end + "</tspan>".len();
                remaining = &tspan_tail[tspan_close_end..];
            }
            if valid_tspans {
                tspan_texts
            } else {
                Vec::new()
            }
        };
        for (point, font_size, presentation, text) in resolved_texts {
            if font_size.is_finite() && font_size > 0.0 {
                texts.push(SimpleSvgText {
                    x_ratio: point.0,
                    y_ratio: point.1,
                    font_size_ratio: font_size / view_box.3,
                    anchor: presentation
                        .text_anchor
                        .unwrap_or(SimpleSvgTextAnchor::Start),
                    font_family: presentation
                        .font_family
                        .unwrap_or(SimpleSvgFontFamily::Serif),
                    font_series: presentation.font_series.unwrap_or(FontSeries::Regular),
                    font_shape: presentation.font_shape.unwrap_or(FontShape::Upright),
                    fill: fill_paint(presentation, Some((0.0, 0.0, 0.0))),
                    text,
                });
            }
        }
        search_index = text_body_end + "</text>".len();
    }
    Some(SimpleSvgAsset {
        natural_width_pt: natural_size.0,
        natural_height_pt: natural_size.1,
        view_box_aspect_ratio: view_box.2 / view_box.3,
        preserve_aspect_ratio,
        rects,
        lines,
        ellipses,
        polys,
        paths,
        texts,
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
    fn renders_simple_svg_rounded_rects_as_pdf_vector_paths() {
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
                asset_ref: "figures/rounded.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:rounded".to_string()),
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
            (asset_ref == "figures/rounded.svg").then(|| {
                br##"<svg width="20" height="10">
  <rect x="2" y="1" width="16" height="8" rx="4" ry="2" fill="#ff0000" stroke="#0000ff" stroke-width="0.5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 115.2 706.8 m 172.79999 706.8 l "));
        assert!(pdf_text.contains(" c "));
        assert!(pdf_text.contains(" h f"));
        assert!(pdf_text.contains("0 0 1 RG"));
        assert!(!pdf_text.contains("72 642 144 72 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/rounded.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_as_pdf_text() {
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
                asset_ref: "figures/text.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:text".to_string()),
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
            (asset_ref == "figures/text.svg").then(|| {
                br##"<svg width="20" height="10">
  <text x="2" y="6" font-size="2" fill="#00ff00">A &amp; B</text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(
            pdf_text.contains("0 1 0 rg BT /F1 14.400001 Tf 1 0 0 1 86.4 670.8 Tm (A & B) Tj ET")
        );
        assert!(!pdf_text.contains("[unsupported image: figures/text.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_anchor_as_pdf_position_adjustment() {
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
                asset_ref: "figures/text-anchor.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:text-anchor".to_string()),
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
            (asset_ref == "figures/text-anchor.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    text.centered { text-anchor: middle; fill: #0000ff; }
  </style>
  <text class="centered" x="10" y="6" font-size="2">aa</text>
  <text x="10" y="8" font-size="2" fill="#ff0000" text-anchor="end">bb</text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(
            pdf_text.contains("0 0 1 rg BT /F1 14.400001 Tf 1 0 0 1 136.8 670.8 Tm (aa) Tj ET")
        );
        assert!(
            pdf_text.contains("1 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 129.6 656.4 Tm (bb) Tj ET")
        );
        assert!(!pdf_text.contains("[unsupported image: figures/text-anchor.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_font_style_as_pdf_font_resources() {
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
                asset_ref: "figures/font-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:font-style".to_string()),
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
            (asset_ref == "figures/font-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    text.bold { fill: #000000; font-size: 2; font-weight: 700; }
    tspan.italic { fill: #000000; font-size: 2; font-style: oblique; }
  </style>
  <text class="bold" x="10" y="4">B</text>
  <text x="10" y="6"><tspan class="italic">I</tspan></text>
  <text x="10" y="8" font-weight="bold" font-style="italic" font-size="2">BI</text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 0 rg BT /F2 14.400001 Tf 1 0 0 1 144 685.2 Tm (B) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F3 14.400001 Tf 1 0 0 1 144 670.8 Tm (I) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F4 14.400001 Tf 1 0 0 1 144 656.4 Tm (BI) Tj ET"));
        assert!(!pdf_text.contains("[unsupported image: figures/font-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_font_family_as_pdf_font_resources() {
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
                asset_ref: "figures/font-family.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:font-family".to_string()),
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
            (asset_ref == "figures/font-family.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    text.sans { fill: #000000; font-family: Arial, sans-serif; font-size: 2; font-weight: bold; }
  </style>
  <text class="sans" x="10" y="4">S</text>
  <text x="10" y="6" font-family="'Courier New', monospace" font-style="italic" font-size="2">M</text>
  <text x="10" y="8" font-family="Helvetica" font-size="2"><tspan font-weight="bold" font-style="italic">BI</tspan></text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 0 rg BT /F6 14.400001 Tf 1 0 0 1 144 685.2 Tm (S) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F11 14.400001 Tf 1 0 0 1 144 670.8 Tm (M) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F8 14.400001 Tf 1 0 0 1 144 656.4 Tm (BI) Tj ET"));
        assert!(!pdf_text.contains("[unsupported image: figures/font-family.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_percentage_font_size_relative_to_parent() {
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
                asset_ref: "figures/font-size-percent.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:font-size-percent".to_string()),
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
            (asset_ref == "figures/font-size-percent.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    text.root-percent { fill: #000000; font-size: 50%; }
  </style>
  <text class="root-percent" x="10" y="4">R</text>
  <text x="10" y="6" font-size="4" fill="#000000"><tspan font-size="50%">P</tspan></text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 0 rg BT /F1 43.2 Tf 1 0 0 1 144 685.2 Tm (R) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 670.8 Tm (P) Tj ET"));
        assert!(!pdf_text.contains("/F1 360 Tf"));
        assert!(!pdf_text.contains("[unsupported image: figures/font-size-percent.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_single_tspan_as_pdf_text() {
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
                asset_ref: "figures/tspan.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:tspan".to_string()),
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
            (asset_ref == "figures/tspan.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    text.label { text-anchor: middle; fill: #0000ff; }
    tspan.hot { fill: #ff0000; font-size: 2; }
  </style>
  <text class="label" x="0" y="0"><tspan class="hot" x="10" y="6">aa</tspan></text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(
            pdf_text.contains("1 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 136.8 670.8 Tm (aa) Tj ET")
        );
        assert!(!pdf_text.contains("0 0 1 rg BT"));
        assert!(!pdf_text.contains("[unsupported image: figures/tspan.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_multiple_tspans_as_pdf_text_runs() {
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
                asset_ref: "figures/multiple-tspans.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:multiple-tspans".to_string()),
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
            (asset_ref == "figures/multiple-tspans.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    tspan.first { fill: #0000ff; font-size: 2; }
    tspan.second { fill: #ff0000; font-size: 2; }
  </style>
  <text x="0" y="0">
    <tspan class="first" x="10" y="6">A</tspan>
    <tspan class="second" x="10" y="7">B</tspan>
  </text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 rg BT /F1 14.400001 Tf 1 0 0 1 144 670.8 Tm (A) Tj ET"));
        assert!(pdf_text.contains("1 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 663.6 Tm (B) Tj ET"));
        assert!(!pdf_text.contains("[unsupported image: figures/multiple-tspans.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_baseline_shift_as_pdf_position_adjustments() {
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
                asset_ref: "figures/baseline-shift.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:baseline-shift".to_string()),
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
            (asset_ref == "figures/baseline-shift.svg").then(|| {
                br##"<svg width="20" height="10">
  <text x="10" y="0" font-size="2" fill="#000000">
    <tspan x="10" y="6" baseline-shift="1">U</tspan>
    <tspan x="10" y="7" baseline-shift="-1">D</tspan>
    <tspan x="10" y="8" baseline-shift="50%">P</tspan>
    <tspan x="10" y="9" baseline-shift="super">S</tspan>
  </text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 678 Tm (U) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 656.4 Tm (D) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 663.6 Tm (P) Tj ET"));
        assert!(pdf_text.contains("0 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 657.84 Tm (S) Tj ET"));
        assert!(!pdf_text.contains("[unsupported image: figures/baseline-shift.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_and_tspan_offsets_as_pdf_position_adjustments() {
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
                asset_ref: "figures/text-offset.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:text-offset".to_string()),
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
            (asset_ref == "figures/text-offset.svg").then(|| {
                br##"<svg width="20" height="10">
  <text x="4" y="4" dx="2" dy="1" font-size="2" fill="#00ff00">A</text>
  <text x="0" y="0"><tspan x="9" y="5" dx="1" dy="2" font-size="2" fill="#ff0000">B</tspan></text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 rg BT /F1 14.400001 Tf 1 0 0 1 115.2 678 Tm (A) Tj ET"));
        assert!(pdf_text.contains("1 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 663.6 Tm (B) Tj ET"));
        assert!(!pdf_text.contains("[unsupported image: figures/text-offset.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_text_baseline_alignment_as_pdf_position_adjustments() {
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
                asset_ref: "figures/text-baseline.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:text-baseline".to_string()),
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
            (asset_ref == "figures/text-baseline.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    text.mid { dominant-baseline: middle; font-size: 2; fill: #0000ff; }
    tspan.center { alignment-baseline: central; font-size: 2; fill: #ff0000; }
  </style>
  <text class="mid" x="10" y="5">M</text>
  <text x="0" y="0"><tspan class="center" x="10" y="6">T</tspan></text>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 rg BT /F1 14.400001 Tf 1 0 0 1 144 670.8 Tm (M) Tj ET"));
        assert!(pdf_text.contains("1 0 0 rg BT /F1 14.400001 Tf 1 0 0 1 144 663.6 Tm (T) Tj ET"));
        assert!(!pdf_text.contains("[unsupported image: figures/text-baseline.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_preserve_aspect_ratio_as_pdf_vector_content() {
        let page = PageDisplayList {
            page_id: "page-1".to_string(),
            width_pt: 300.0,
            height_pt: 300.0,
            ops: vec![
                DrawOp::Image(PositionedImage {
                    rect: Rect {
                        x: 10.0,
                        y: 20.0,
                        width: 200.0,
                        height: 100.0,
                    },
                    asset_ref: "figures/aspect-meet.svg".to_string(),
                    asset_format: Some(GraphicAssetFormat::Svg),
                    page_selection: None,
                    asset_hash: Some("blake3:aspect-meet".to_string()),
                    natural_width_pt: None,
                    natural_height_pt: None,
                    crop: None,
                    scale: None,
                    rotation: None,
                    diagnostic: None,
                    source: SourceProvenance::file("main.tex", 0, 10),
                }),
                DrawOp::Image(PositionedImage {
                    rect: Rect {
                        x: 10.0,
                        y: 140.0,
                        width: 200.0,
                        height: 100.0,
                    },
                    asset_ref: "figures/aspect-none.svg".to_string(),
                    asset_format: Some(GraphicAssetFormat::Svg),
                    page_selection: None,
                    asset_hash: Some("blake3:aspect-none".to_string()),
                    natural_width_pt: None,
                    natural_height_pt: None,
                    crop: None,
                    scale: None,
                    rotation: None,
                    diagnostic: None,
                    source: SourceProvenance::file("main.tex", 0, 10),
                }),
            ],
            source_spans: Vec::new(),
            content_hash: "hash".to_string(),
        };
        let pdf = render_display_list_pdf_with_assets(&[page], |asset_ref| match asset_ref {
            "figures/aspect-meet.svg" => Some(
                br##"<svg width="20" height="10" viewBox="0 0 20 20">
  <rect width="20" height="20" fill="#ff0000"/>
</svg>"##
                    .to_vec(),
            ),
            "figures/aspect-none.svg" => Some(
                br##"<svg width="20" height="10" viewBox="0 0 20 20" preserveAspectRatio="none">
  <rect width="20" height="20" fill="#0000ff"/>
</svg>"##
                    .to_vec(),
            ),
            _ => None,
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 60 180 100 100 re f"));
        assert!(pdf_text.contains("0 0 1 rg 10 60 200 100 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/aspect-meet.svg]"));
        assert!(!pdf_text.contains("[unsupported image: figures/aspect-none.svg]"));
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
  <line x1="0" y1="10" x2="20" y2="0" stroke-width="1" stroke="#00ff00"/>
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
    fn renders_simple_svg_circle_and_ellipse_as_pdf_vector_content() {
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
                asset_ref: "figures/markers.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:markers".to_string()),
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
            (asset_ref == "figures/markers.svg").then(|| {
                br##"<svg width="20" height="10">
  <circle cx="5" cy="5" r="2" fill="#ff0000"/>
  <ellipse cx="15" cy="5" rx="3" ry="2" fill="none" stroke-width="0.5" stroke="#0000ff"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 80 230 m"));
        assert!(pdf_text.contains("0 0 1 RG 5 w 190 230 m"));
        assert!(!pdf_text.contains("[unsupported image: figures/markers.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_transformed_ellipses_as_pdf_vector_content() {
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
                asset_ref: "figures/transformed-ellipse.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:transformed-ellipse".to_string()),
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
            (asset_ref == "figures/transformed-ellipse.svg").then(|| {
                br##"<svg width="20" height="10">
  <ellipse cx="5" cy="2" rx="4" ry="1" transform="matrix(0 1 -1 0 10 0)" fill="#ff0000" stroke="#0000ff" stroke-width="1"/>
  <circle cx="5" cy="6" r="1" transform="matrix(0 1 -1 0 10 0)" fill="none" stroke="#00ff00" stroke-width="1"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 90 190 m"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 90 190 m"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 50 220 m"));
        assert!(!pdf_text.contains("[unsupported image: figures/transformed-ellipse.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_polylines_and_polygons_as_pdf_vector_content() {
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
                asset_ref: "figures/poly.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:poly".to_string()),
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
            (asset_ref == "figures/poly.svg").then(|| {
                br##"<svg width="20" height="10">
  <polyline points="0,10 10,0 20,10" fill="none" stroke-width="1" stroke="#00ff00"/>
  <polygon points="2,8 10,2 18,8" style="fill:#ff0000;stroke:#0000ff;stroke-width:0.5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 RG 10 w 10 180 m 110 280 l 210 180 l S"));
        assert!(pdf_text.contains("1 0 0 rg 30 200 m 110 260 l 190 200 l h f"));
        assert!(pdf_text.contains("0 0 1 RG 5 w 30 200 m 110 260 l 190 200 l h S"));
        assert!(!pdf_text.contains("[unsupported image: figures/poly.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_line_paths_as_pdf_vector_content() {
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
                asset_ref: "figures/path.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:path".to_string()),
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
            (asset_ref == "figures/path.svg").then(|| {
                br##"<svg width="20" height="10">
  <path d="M 0 5 L 20 5" fill="none" stroke-width="1" stroke="#00ff00"/>
  <path d="M 0 10 L 10 0 H 20 V 10 Z" style="fill:#ff0000;stroke:#0000ff;stroke-width:0.5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 210 230 l S"));
        assert!(pdf_text.contains("1 0 0 rg 10 180 m 110 280 l 210 280 l 210 180 l h f"));
        assert!(pdf_text.contains("0 0 1 RG 5 w 10 180 m 110 280 l 210 280 l 210 180 l h S"));
        assert!(!pdf_text.contains("[unsupported image: figures/path.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_defs_pathlike_use_as_pdf_vector_content() {
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
                asset_ref: "figures/defs-use.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:defs-use".to_string()),
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
            (asset_ref == "figures/defs-use.svg").then(|| {
                br##"<svg width="20" height="10">
  <defs>
    <path id="tri" d="M 0 0 L 4 0 L 4 4 Z" fill="#ff0000"/>
    <path id="rule" d="M 0 0 L 4 0" fill="none" stroke="#ff0000" stroke-width="1"/>
    <rect id="panel" x="0" y="0" width="4" height="2" fill="#ff0000"/>
    <circle id="dot" cx="2" cy="2" r="2" fill="#ff0000"/>
    <ellipse id="oval" cx="2" cy="1" rx="2" ry="1" fill="#ff0000"/>
    <line id="line-rule" x1="0" y1="0" x2="4" y2="0" stroke="#ff0000" stroke-width="1"/>
    <polyline id="zig" points="0 0 2 2 4 0" fill="none" stroke="#ff0000" stroke-width="1"/>
    <polygon id="box" points="0 0 4 0 4 4 0 4" fill="#ff0000"/>
  </defs>
  <use href="#tri" x="5" y="0" fill="#0000ff"/>
  <use xlink:href="#rule" x="5" y="4" stroke="#00ff00"/>
  <use href="#panel" x="0" y="5" fill="#0000ff"/>
  <use href="#dot" x="5" y="5" fill="#ff00ff"/>
  <use href="#oval" x="10" y="6" fill="#00ffff"/>
  <use href="#line-rule" x="10" y="0" stroke="#00ff00"/>
  <use href="#zig" x="10" y="2" stroke="#0000ff"/>
  <use href="#box" x="10" y="5" fill="#00ff00"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 rg 60 280 m 100 280 l 100 240 l h f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 60 240 m 100 240 l S"));
        assert!(pdf_text.contains("0 0 1 rg 10 230 m 50 230 l 50 210 l 10 210 l h f"));
        assert!(pdf_text.contains("1 0 1 rg 100 210 m"));
        assert!(pdf_text.contains("0 1 1 rg 150 210 m"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 110 280 m 150 280 l S"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 110 260 m 130 240 l 150 260 l S"));
        assert!(pdf_text.contains("0 1 0 rg 110 230 m 150 230 l 150 190 l 110 190 l h f"));
        assert!(!pdf_text.contains("1 0 0 rg 10 280 m 50 280 l 50 240 l h f"));
        assert!(!pdf_text.contains("1 0 0 RG 10 w 10 280 m 50 280 l S"));
        assert!(!pdf_text.contains("1 0 0 rg 10 280 m 50 280 l 50 260 l 10 260 l h f"));
        assert!(!pdf_text.contains("1 0 0 rg 50 260 m"));
        assert!(!pdf_text.contains("1 0 0 rg 10 280 m 50 280 l 50 240 l 10 240 l h f"));
        assert!(!pdf_text.contains("[unsupported image: figures/defs-use.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_compound_paths_as_pdf_vector_content() {
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
                asset_ref: "figures/compound-path.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:compound-path".to_string()),
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
            (asset_ref == "figures/compound-path.svg").then(|| {
                br##"<svg width="20" height="10">
  <path fill="#ff0000" fill-rule="evenodd" d="M 0 0 L 20 0 L 20 10 L 0 10 Z M 5 2 L 15 2 L 15 8 L 5 8 Z"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains(
            "1 0 0 rg 10 280 m 210 280 l 210 180 l 10 180 l h 60 260 m 160 260 l 160 200 l 60 200 l h f*"
        ));
        assert!(!pdf_text.contains("[unsupported image: figures/compound-path.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_cubic_paths_as_pdf_vector_content() {
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
                asset_ref: "figures/cubic.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:cubic".to_string()),
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
            (asset_ref == "figures/cubic.svg").then(|| {
                br##"<svg width="20" height="10">
  <path d="M 0 5 C 5 0 15 0 20 5" fill="none" stroke-width="1" stroke="#00ff00"/>
  <path d="M 2 8 c 4 -6 12 -6 16 0 Z" fill="#ff0000"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 60 280 160 280 210 230 c S"));
        assert!(pdf_text.contains("1 0 0 rg 30 200 m 70 260 150 260 190 200 c h f"));
        assert!(!pdf_text.contains("[unsupported image: figures/cubic.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_smooth_and_quadratic_paths_as_pdf_vector_content() {
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
                asset_ref: "figures/smooth.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:smooth".to_string()),
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
            (asset_ref == "figures/smooth.svg").then(|| {
                br##"<svg width="20" height="10">
  <path d="M 0 5 C 5 0 10 0 10 5 S 15 10 20 5" fill="none" stroke-width="1" stroke="#00ff00"/>
  <path d="M 0 6 Q 6 0 12 6 T 24 6" fill="none" stroke-width="1" stroke="#0000ff"/>
  <path d="M 2 5 q 6 -6 12 0 t 12 0" fill="none" stroke-width="1" stroke="#ff0000"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains(
            "0 1 0 RG 10 w 10 230 m 60 280 110 280 110 230 c 110 180 160 180 210 230 c S"
        ));
        assert!(pdf_text.contains(
            "0 0 1 RG 10 w 10 220 m 50 260 90 260 130 220 c 170 180 210 180 250.00002 220 c S"
        ));
        assert!(pdf_text.contains(
            "1 0 0 RG 10 w 30 230 m 70 270 110 270 150 230 c 190 190 230 190 270 230 c S"
        ));
        assert!(!pdf_text.contains("[unsupported image: figures/smooth.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_arc_paths_as_pdf_vector_content() {
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
                asset_ref: "figures/arc.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:arc".to_string()),
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
            (asset_ref == "figures/arc.svg").then(|| {
                br##"<svg width="20" height="10">
  <path d="M 5 5 A 5 5 0 0 1 10 0" fill="none" stroke-width="1" stroke="#00ff00"/>
  <path d="M 5 5 a 5 5 0 0 0 5 5" fill="none" stroke-width="1" stroke="#ff0000"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 RG 10 w 60 230 m "));
        assert!(pdf_text.contains("110 280 c S"));
        assert!(pdf_text.contains("1 0 0 RG 10 w 60 230 m "));
        assert!(pdf_text.contains("110 180 c S"));
        assert!(!pdf_text.contains("[unsupported image: figures/arc.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_translate_and_scale_transforms_as_pdf_vector_content() {
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
                asset_ref: "figures/transformed.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:transformed".to_string()),
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
            (asset_ref == "figures/transformed.svg").then(|| {
                br##"<svg width="20" height="10">
  <rect x="1" y="1" width="4" height="2" transform="translate(2 1)" fill="#ff0000"/>
  <rect x="1" y="1" width="4" height="2" transform="scale(2)" fill="#0000ff"/>
  <path d="M 0 0 L 5 0" transform="translate(2,1)" fill="none" stroke-width="1" stroke="#00ff00"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 40 240 40 20 re f"));
        assert!(pdf_text.contains("0 0 1 rg 30 220 80 40 re f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 30 270 m 80 270 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/transformed.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_matrix_and_rotate_transforms_as_pdf_vector_content() {
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
                asset_ref: "figures/affine.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:affine".to_string()),
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
            (asset_ref == "figures/affine.svg").then(|| {
                br##"<svg width="20" height="10">
  <line x1="5" y1="5" x2="10" y2="5" transform="rotate(90 5 5)" stroke-width="1" stroke="#00ff00"/>
  <path d="M 0 0 L 5 0" transform="matrix(1 0 0 1 2 1)" fill="none" stroke-width="1" stroke="#ff0000"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 RG 10 w 60 230 m 60 180 l S"));
        assert!(pdf_text.contains("1 0 0 RG 10 w 30 270 m 80 270 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/affine.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_skew_transforms_as_pdf_vector_content() {
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
                asset_ref: "figures/skew.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:skew".to_string()),
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
            (asset_ref == "figures/skew.svg").then(|| {
                br##"<svg width="20" height="10">
  <path d="M 0 2 L 5 2" transform="skewX(45)" fill="none" stroke-width="1" stroke="#ff0000"/>
  <path d="M 2 0 L 2 5" transform="skewY(45)" fill="none" stroke-width="1" stroke="#0000ff"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("30 260 m 80 260 l S"));
        assert!(pdf_text.contains("30 260 m 30 210 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/skew.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_transformed_rectangles_as_pdf_vector_content() {
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
                asset_ref: "figures/rotated-rect.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:rotated-rect".to_string()),
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
            (asset_ref == "figures/rotated-rect.svg").then(|| {
                br##"<svg width="20" height="10">
  <rect x="0" y="0" width="5" height="2" transform="matrix(0 1 -1 0 10 0)" fill="#ff0000" stroke="#0000ff" stroke-width="1"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 110 280 m 110 230 l 90 230 l 90 280 l h f"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 110 280 m 110 230 l 90 230 l 90 280 l h S"));
        assert!(!pdf_text.contains("[unsupported image: figures/rotated-rect.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_group_transforms_as_pdf_vector_content() {
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
                asset_ref: "figures/grouped.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:grouped".to_string()),
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
            (asset_ref == "figures/grouped.svg").then(|| {
                br##"<svg width="20" height="10">
  <g transform="translate(2 1)">
    <path d="M 0 0 L 5 0" fill="none" stroke-width="1" stroke="#ff0000"/>
  </g>
  <g transform="scale(2)">
    <line x1="1" y1="1" x2="4" y2="1" stroke-width="1" stroke="#0000ff"/>
  </g>
  <g transform="translate(2 1)">
    <g transform="scale(2)">
      <path d="M 0 2 L 5 2" fill="none" stroke-width="1" stroke="#00ff00"/>
    </g>
  </g>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 RG 10 w 30 270 m 80 270 l S"));
        assert!(pdf_text.contains("0 0 1 RG 20 w 30 260 m 90 260 l S"));
        assert!(pdf_text.contains("0 1 0 RG 20 w 30 230 m 130 230 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/grouped.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_group_presentation_as_pdf_vector_content() {
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
                asset_ref: "figures/group-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:group-style".to_string()),
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
            (asset_ref == "figures/group-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <g stroke="#ff0000" stroke-width="2">
    <line x1="0" y1="0" x2="5" y2="0"/>
  </g>
  <g stroke="#ff0000">
    <line x1="0" y1="2" x2="5" y2="2" stroke="#0000ff"/>
  </g>
  <g style="fill:#0000ff">
    <rect x="1" y="1" width="2" height="2"/>
  </g>
  <g fill="none" stroke="#00ff00">
    <path d="M 0 5 L 5 5"/>
  </g>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 10 260 m 60 260 l S"));
        assert!(pdf_text.contains("0 0 1 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 60 230 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/group-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_root_presentation_as_pdf_vector_content() {
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
                asset_ref: "figures/root-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:root-style".to_string()),
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
            (asset_ref == "figures/root-style.svg").then(|| {
                br##"<svg width="20" height="10" style="fill:#ff0000;stroke:#0000ff;stroke-width:2">
  <line x1="0" y1="0" x2="5" y2="0"/>
  <rect x="1" y="1" width="2" height="2"/>
  <g stroke="#00ff00">
    <path d="M 0 5 L 5 5" fill="none"/>
  </g>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 0 1 RG 20 w 20 250 20 20 re S"));
        assert!(pdf_text.contains("0 1 0 RG 20 w 10 230 m 60 230 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/root-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_class_style_as_pdf_vector_content() {
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
                asset_ref: "figures/class-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:class-style".to_string()),
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
            (asset_ref == "figures/class-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style>
    .blue { stroke: #0000ff; stroke-width: 2; fill: none; }
    .red-fill { fill: #ff0000; }
  </style>
  <line class="blue" x1="0" y1="0" x2="5" y2="0"/>
  <rect class="red-fill blue" x="1" y="1" width="2" height="2"/>
  <g class="blue">
    <path d="M 0 5 L 5 5"/>
  </g>
  <line class="blue" style="stroke:#00ff00" x1="0" y1="7" x2="5" y2="7"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 0 1 RG 20 w 20 250 20 20 re S"));
        assert!(pdf_text.contains("0 0 1 RG 20 w 10 230 m 60 230 l S"));
        assert!(pdf_text.contains("0 1 0 RG 20 w 10 210 m 60 210 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/class-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_cdata_class_style_as_pdf_vector_content() {
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
                asset_ref: "figures/cdata-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:cdata-style".to_string()),
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
            (asset_ref == "figures/cdata-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css"><![CDATA[
    .blue { stroke: #0000ff; stroke-width: 2; fill: none; }
  ]]></style>
  <line class="blue" x1="0" y1="0" x2="5" y2="0"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/cdata-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_commented_class_style_as_pdf_vector_content() {
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
                asset_ref: "figures/commented-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:commented-style".to_string()),
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
            (asset_ref == "figures/commented-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    /* generated palette */
    .blue { stroke: #0000ff; stroke-width: 2; fill: none; }
  </style>
  <line class="blue" x1="0" y1="0" x2="5" y2="0"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/commented-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_element_qualified_class_style_as_pdf_vector_content() {
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
                asset_ref: "figures/qualified-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:qualified-style".to_string()),
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
            (asset_ref == "figures/qualified-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    line.blue { stroke: #0000ff; stroke-width: 2; fill: none; }
    rect.blue { fill: #ff0000; stroke: #00ff00; stroke-width: 1; }
  </style>
  <line class="blue" x1="0" y1="0" x2="5" y2="0"/>
  <rect class="blue" x="1" y="1" width="2" height="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 20 250 20 20 re S"));
        assert!(!pdf_text.contains("[unsupported image: figures/qualified-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_type_selector_style_as_pdf_vector_content() {
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
                asset_ref: "figures/type-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:type-style".to_string()),
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
            (asset_ref == "figures/type-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    line { stroke: #0000ff; stroke-width: 2; fill: none; }
    rect { fill: #ff0000; stroke: #00ff00; stroke-width: 1; }
  </style>
  <line x1="0" y1="0" x2="5" y2="0"/>
  <rect x="1" y="1" width="2" height="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 20 250 20 20 re S"));
        assert!(!pdf_text.contains("[unsupported image: figures/type-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_id_selector_style_as_pdf_vector_content() {
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
                asset_ref: "figures/id-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:id-style".to_string()),
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
            (asset_ref == "figures/id-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    line#blue-line { stroke: #0000ff; stroke-width: 2; fill: none; }
    #red-box { fill: #ff0000; stroke: #00ff00; stroke-width: 1; }
  </style>
  <line id="blue-line" x1="0" y1="0" x2="5" y2="0"/>
  <rect id="red-box" x="1" y="1" width="2" height="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 20 250 20 20 re S"));
        assert!(!pdf_text.contains("[unsupported image: figures/id-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_style_specificity_as_pdf_vector_content() {
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
                asset_ref: "figures/specificity-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:specificity-style".to_string()),
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
            (asset_ref == "figures/specificity-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    #focus-line { stroke: #0000ff; stroke-width: 2; fill: none; }
    .line-accent { stroke: #00ff00; stroke-width: 4; }
    .rect-accent { fill: #00ff00; stroke: #0000ff; stroke-width: 1; }
    line { stroke: #ff0000; stroke-width: 6; fill: none; }
    rect { fill: #ff0000; stroke: #ff0000; stroke-width: 3; }
    .ordered { stroke: #ff0000; stroke-width: 6; fill: none; }
    .ordered { stroke: #0000ff; stroke-width: 2; }
  </style>
  <line id="focus-line" class="line-accent" x1="0" y1="0" x2="5" y2="0"/>
  <rect class="rect-accent" x="1" y="1" width="2" height="2"/>
  <line class="ordered" x1="0" y1="2" x2="5" y2="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 RG 20 w 10 280 m 60 280 l S"));
        assert!(pdf_text.contains("0 1 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 20 250 20 20 re S"));
        assert!(pdf_text.contains("0 0 1 RG 20 w 10 260 m 60 260 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/specificity-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_rgb_color_functions_as_pdf_vector_content() {
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
                asset_ref: "figures/rgb-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:rgb-style".to_string()),
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
            (asset_ref == "figures/rgb-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    rect { fill: rgb(255, 0, 0); stroke: rgb(0 0 255); stroke-width: 1; }
    line { stroke: rgb(0%, 100%, 0%); stroke-width: 2; fill: none; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 20 250 20 20 re S"));
        assert!(pdf_text.contains("0 1 0 RG 20 w 10 260 m 60 260 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/rgb-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_rgba_color_functions_as_pdf_opacity() {
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
                asset_ref: "figures/rgba-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:rgba-style".to_string()),
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
            (asset_ref == "figures/rgba-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <rect x="1" y="1" width="2" height="2" fill="rgba(255, 0, 0, 0.25)" stroke="rgb(0 0 255 / 50%)" stroke-width="1"/>
  <line x1="0" y1="2" x2="5" y2="2" stroke="rgba(0%, 100%, 0%, 0.4)" stroke-width="2" fill="none"/>
  <rect x="4" y="1" width="2" height="2" fill="rgb(0 255 255 / 25%)" stroke="none"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("/GS250 << /Type /ExtGState /ca 0.25 /CA 0.25 >>"));
        assert!(pdf_text.contains("/GS500 << /Type /ExtGState /ca 0.5 /CA 0.5 >>"));
        assert!(pdf_text.contains("/GS400 << /Type /ExtGState /ca 0.4 /CA 0.4 >>"));
        assert!(pdf_text.contains("q /GS250 gs 1 0 0 rg 20 250 20 20 re f Q"));
        assert!(pdf_text.contains("q /GS500 gs 0 0 1 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q /GS400 gs 0 1 0 RG 20 w 10 260 m 60 260 l S Q"));
        assert!(pdf_text.contains("q /GS250 gs 0 1 1 rg 50 250 20 20 re f Q"));
        assert!(!pdf_text.contains("[unsupported image: figures/rgba-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_hex_alpha_colors_as_pdf_opacity() {
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
                asset_ref: "figures/hex-alpha-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:hex-alpha-style".to_string()),
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
            (asset_ref == "figures/hex-alpha-style.svg").then(|| {
                br##"<svg width="20" height="10" color="#0000ff33">
  <rect x="1" y="1" width="2" height="2" fill="#0f03" fill-opacity="0.5" stroke="#ff000033" stroke-width="1"/>
  <line x1="0" y1="2" x2="5" y2="2" stroke="currentColor" stroke-width="1" fill="none"/>
  <rect x="4" y="1" width="2" height="2" fill="#0000" stroke="#00ff0033" stroke-opacity="0.5" stroke-width="1"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("/GS100 << /Type /ExtGState /ca 0.1 /CA 0.1 >>"));
        assert!(pdf_text.contains("/GS200 << /Type /ExtGState /ca 0.2 /CA 0.2 >>"));
        assert!(pdf_text.contains("q /GS100 gs 0 1 0 rg 20 250 20 20 re f Q"));
        assert!(pdf_text.contains("q /GS200 gs 1 0 0 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q /GS200 gs 0 0 1 RG 10 w 10 260 m 60 260 l S Q"));
        assert!(pdf_text.contains("q /GS100 gs 0 1 0 RG 10 w 50 250 20 20 re S Q"));
        assert!(!pdf_text.contains("0 0 0 rg 50 250 20 20 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/hex-alpha-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_common_named_colors_as_pdf_vector_content() {
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
                asset_ref: "figures/named-color-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:named-color-style".to_string()),
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
            (asset_ref == "figures/named-color-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    rect { fill: yellow; stroke: cyan; stroke-width: 1; }
    line { stroke: magenta; stroke-width: 2; fill: none; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 1 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 1 RG 10 w 20 250 20 20 re S"));
        assert!(pdf_text.contains("1 0 1 RG 20 w 10 260 m 60 260 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/named-color-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_current_color_paint_as_inherited_color() {
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
                asset_ref: "figures/current-color.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:current-color".to_string()),
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
            (asset_ref == "figures/current-color.svg").then(|| {
                br##"<svg width="20" height="10" color="#00ff00">
  <style>
    .accent { color: #0000ff; fill: currentColor; }
  </style>
  <rect x="1" y="1" width="2" height="2" fill="currentColor"/>
  <rect class="accent" x="4" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2" color="#ff0000" stroke="currentColor" stroke-width="1" fill="none"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 0 1 rg 50 250 20 20 re f"));
        assert!(pdf_text.contains("1 0 0 RG 10 w 10 260 m 60 260 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/current-color.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_url_paint_fallback_colors() {
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
                asset_ref: "figures/url-paint-fallback.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:url-paint-fallback".to_string()),
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
            (asset_ref == "figures/url-paint-fallback.svg").then(|| {
                br##"<svg width="20" height="10" color="#0000ff">
  <rect x="1" y="1" width="2" height="2" fill="url(#missing) #00ff00"/>
  <line x1="0" y1="2" x2="5" y2="2" stroke="url(#missing) currentColor" stroke-width="1" fill="none"/>
  <rect x="4" y="1" width="2" height="2" fill="url(#missing) none" stroke="#ff0000" stroke-width="1"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 0 1 RG 10 w 10 260 m 60 260 l S"));
        assert!(pdf_text.contains("1 0 0 RG 10 w 50 250 20 20 re S"));
        assert!(!pdf_text.contains("0 0 0 rg 20 250 20 20 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/url-paint-fallback.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn treats_simple_svg_inherit_paint_as_parent_presentation() {
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
                asset_ref: "figures/inherit-paint.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:inherit-paint".to_string()),
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
            (asset_ref == "figures/inherit-paint.svg").then(|| {
                br##"<svg width="20" height="10" fill="#ff0000" stroke="#00ff00" stroke-width="1" color="#00ff00">
  <rect x="1" y="1" width="2" height="2" fill="inherit"/>
  <line x1="0" y1="2" x2="5" y2="2" stroke="inherit" fill="none"/>
  <g color="#0000ff">
    <rect x="4" y="1" width="2" height="2" color="inherit" fill="currentColor"/>
  </g>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 260 m 60 260 l S"));
        assert!(pdf_text.contains("0 0 1 rg 50 250 20 20 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/inherit-paint.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn ignores_simple_svg_element_name_prefix_false_positives() {
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
                asset_ref: "figures/prefix-elements.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:prefix-elements".to_string()),
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
            (asset_ref == "figures/prefix-elements.svg").then(|| {
                br##"<svg width="20" height="10">
  <linearGradient id="g" x1="0" y1="0" x2="5" y2="0" stroke="#ff0000" stroke-width="1">
    <stop offset="0" stop-color="#ffffff"/>
  </linearGradient>
  <rect x="1" y="1" width="2" height="2" fill="#00ff00"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 1 0 rg 20 250 20 20 re f"));
        assert!(!pdf_text.contains("1 0 0 RG 10 w 10 280 m 60 280 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/prefix-elements.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn ignores_simple_svg_root_and_style_prefix_false_positives() {
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
                asset_ref: "figures/root-style-prefix.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:root-style-prefix".to_string()),
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
            (asset_ref == "figures/root-style-prefix.svg").then(|| {
                br##"<svgz width="20" height="10">
  <line x1="0" y1="0" x2="5" y2="0" stroke="#ff0000" stroke-width="1"/>
</svgz>
<svg width="20" height="10">
  <stylesheet>rect { fill: #ff0000; }</stylesheet>
  <style>rect { fill: #0000ff; }</style>
  <rect x="1" y="1" width="2" height="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("0 0 1 rg 20 250 20 20 re f"));
        assert!(!pdf_text.contains("1 0 0 RG 10 w 10 280 m 60 280 l S"));
        assert!(!pdf_text.contains("1 0 0 rg 20 250 20 20 re f"));
        assert!(!pdf_text.contains("[unsupported image: figures/root-style-prefix.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_transparent_paint_as_no_paint() {
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
                asset_ref: "figures/transparent-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:transparent-style".to_string()),
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
            (asset_ref == "figures/transparent-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    rect { fill: transparent; stroke: cyan; stroke-width: 1; }
    line { stroke: transparent; stroke-width: 2; fill: none; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(!pdf_text.contains("0 0 0 rg 20 250 20 20 re f"));
        assert!(pdf_text.contains("0 1 1 RG 10 w 20 250 20 20 re S"));
        assert!(!pdf_text.contains("10 260 m 60 260 l S"));
        assert!(!pdf_text.contains("[unsupported image: figures/transparent-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_paint_opacity_as_pdf_ext_gstate() {
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
                asset_ref: "figures/opacity-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:opacity-style".to_string()),
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
            (asset_ref == "figures/opacity-style.svg").then(|| {
                br##"<svg width="20" height="10">
  <style type="text/css">
    rect { fill: #ff0000; fill-opacity: 0.5; stroke: #0000ff; stroke-opacity: 25%; stroke-width: 1; }
    line { stroke: #00ff00; opacity: 0.4; stroke-width: 2; fill: none; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <rect x="4" y="1" width="2" height="2" style="fill: #000000; fill-opacity: 0.5; stroke: none"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("/ExtGState <<"));
        assert!(pdf_text.contains("/GS500 << /Type /ExtGState /ca 0.5 /CA 0.5 >>"));
        assert!(pdf_text.contains("/GS250 << /Type /ExtGState /ca 0.25 /CA 0.25 >>"));
        assert!(pdf_text.contains("/GS400 << /Type /ExtGState /ca 0.4 /CA 0.4 >>"));
        assert!(pdf_text.contains("q /GS500 gs 1 0 0 rg 20 250 20 20 re f Q"));
        assert!(pdf_text.contains("q /GS250 gs 0 0 1 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q /GS500 gs 0 0 0 rg 50 250 20 20 re f Q"));
        assert!(pdf_text.contains("q /GS400 gs 0 1 0 RG 20 w 10 260 m 60 260 l S Q"));
        assert!(!pdf_text.contains("[unsupported image: figures/opacity-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_stroke_dasharray_as_pdf_dash_pattern() {
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
                asset_ref: "figures/dashed-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:dashed-style".to_string()),
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
            (asset_ref == "figures/dashed-style.svg").then(|| {
                br##"<svg width="20" height="10" stroke-dasharray="2 1">
  <style type="text/css">
    rect { fill: none; stroke: #ff0000; stroke-width: 1; }
    line { stroke: #0000ff; stroke-width: 2; fill: none; stroke-dasharray: 1, 0.5; }
    path { stroke: #00ff00; stroke-width: 1; fill: none; stroke-dasharray: none; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
  <path d="M 0 5 L 5 5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("q [20 10] 0 d 4 M 1 0 0 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q [10 5] 0 d 4 M 0 0 1 RG 20 w 10 260 m 60 260 l S Q"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 60 230 l S"));
        assert!(!pdf_text.contains("q [20 10] 0 d 4 M 0 1 0 RG 10 w 10 230 m 60 230 l S Q"));
        assert!(!pdf_text.contains("[unsupported image: figures/dashed-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_stroke_dashoffset_as_pdf_dash_phase() {
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
                asset_ref: "figures/dashoffset-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:dashoffset-style".to_string()),
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
            (asset_ref == "figures/dashoffset-style.svg").then(|| {
                br##"<svg width="20" height="10" stroke-dasharray="2 1" stroke-dashoffset="0.5">
  <style type="text/css">
    rect { fill: none; stroke: #ff0000; stroke-width: 1; }
    line { stroke: #0000ff; stroke-width: 2; fill: none; stroke-dasharray: 1, 0.5; stroke-dashoffset: 0.25; }
    path { stroke: #00ff00; stroke-width: 1; fill: none; stroke-dasharray: none; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
  <path d="M 0 5 L 5 5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("q [20 10] 5 d 4 M 1 0 0 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q [10 5] 2.5 d 4 M 0 0 1 RG 20 w 10 260 m 60 260 l S Q"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 60 230 l S"));
        assert!(!pdf_text.contains("q [20 10] 5 d 4 M 0 1 0 RG 10 w 10 230 m 60 230 l S Q"));
        assert!(!pdf_text.contains("[unsupported image: figures/dashoffset-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_stroke_line_styles_as_pdf_graphics_state() {
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
                asset_ref: "figures/stroke-line-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:stroke-line-style".to_string()),
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
            (asset_ref == "figures/stroke-line-style.svg").then(|| {
                br##"<svg width="20" height="10" stroke-linecap="round" stroke-linejoin="bevel">
  <style type="text/css">
    rect { fill: none; stroke: #ff0000; stroke-width: 1; }
    line { stroke: #0000ff; stroke-width: 2; fill: none; stroke-linecap: square; }
    path { stroke: #00ff00; stroke-width: 1; fill: none; stroke-linecap: butt; stroke-linejoin: miter; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
  <path d="M 0 5 L 5 5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("q 1 J 2 j 4 M 1 0 0 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q 2 J 2 j 4 M 0 0 1 RG 20 w 10 260 m 60 260 l S Q"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 60 230 l S"));
        assert!(!pdf_text.contains("q 0 J 0 j 0 1 0 RG 10 w 10 230 m 60 230 l S Q"));
        assert!(!pdf_text.contains("[unsupported image: figures/stroke-line-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_stroke_miterlimit_as_pdf_graphics_state() {
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
                asset_ref: "figures/miterlimit-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:miterlimit-style".to_string()),
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
            (asset_ref == "figures/miterlimit-style.svg").then(|| {
                br##"<svg width="20" height="10" stroke-miterlimit="5">
  <style type="text/css">
    rect { fill: none; stroke: #ff0000; stroke-width: 1; }
    line { stroke: #0000ff; stroke-width: 2; fill: none; stroke-linejoin: bevel; stroke-miterlimit: 2; }
    path { stroke: #00ff00; stroke-width: 1; fill: none; stroke-miterlimit: 10; }
  </style>
  <rect x="1" y="1" width="2" height="2"/>
  <line x1="0" y1="2" x2="5" y2="2"/>
  <path d="M 0 5 L 5 5"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains("q 5 M 1 0 0 RG 10 w 20 250 20 20 re S Q"));
        assert!(pdf_text.contains("q 2 j 2 M 0 0 1 RG 20 w 10 260 m 60 260 l S Q"));
        assert!(pdf_text.contains("0 1 0 RG 10 w 10 230 m 60 230 l S"));
        assert!(!pdf_text.contains("q 5 M 0 1 0 RG 10 w 10 230 m 60 230 l S Q"));
        assert!(!pdf_text.contains("[unsupported image: figures/miterlimit-style.svg]"));
        assert!(!pdf_text.contains("/Subtype /Image"));
    }

    #[test]
    fn renders_simple_svg_fill_rule_as_pdf_fill_operator() {
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
                asset_ref: "figures/fill-rule-style.svg".to_string(),
                asset_format: Some(GraphicAssetFormat::Svg),
                page_selection: None,
                asset_hash: Some("blake3:fill-rule-style".to_string()),
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
            (asset_ref == "figures/fill-rule-style.svg").then(|| {
                br##"<svg width="20" height="10" fill-rule="evenodd">
  <style type="text/css">
    path { fill: #ff0000; stroke: none; }
    polygon { fill: #0000ff; stroke: none; fill-rule: nonzero; }
  </style>
  <path d="M 0 0 H 10 V 10 H 0 Z M 2 2 H 8 V 8 H 2 Z"/>
  <polygon points="12,1 18,1 18,8 12,8"/>
</svg>"##
                    .to_vec()
            })
        });
        let pdf_text = String::from_utf8_lossy(&pdf);

        assert!(pdf_text.contains(
            "1 0 0 rg 10 280 m 110 280 l 110 180 l 10 180 l h 30 260 m 90 260 l 90 200 l 30 200 l h f*"
        ));
        assert!(pdf_text.contains("0 0 1 rg 130 270 m 190 270 l 190 200 l 130 200 l h f "));
        assert!(!pdf_text.contains("0 0 1 rg 130 270 m 190 270 l 190 200 l 130 200 l h f*"));
        assert!(!pdf_text.contains("[unsupported image: figures/fill-rule-style.svg]"));
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
