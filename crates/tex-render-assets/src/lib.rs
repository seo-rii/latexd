mod pdf;

pub use pdf::{PreparePdfError, pdf_page_count, prepare_pdf_form};

use tex_render_model::{
    EmbeddedRasterImage, FontSeries, FontShape, GraphicAssetFormat, GraphicAssetRequest,
    MaterializedGraphicAsset, VectorAspectAlign, VectorAspectScale, VectorClipRect,
    VectorDashArray, VectorEllipse, VectorEmbeddedImage, VectorFillRule, VectorFontFamily,
    VectorLine, VectorPaint, VectorPaintOrder, VectorPath, VectorPathOp, VectorPoly,
    VectorPreserveAspectRatio, VectorRect, VectorScene, VectorStrokeLineCap, VectorStrokeLineJoin,
    VectorStrokeStyle, VectorText, VectorTextAnchor, VectorTextBaseline, VectorTextDecoration,
    VectorTextDecorationStyle,
};

fn decode_raster_image(bytes: &[u8]) -> Option<EmbeddedRasterImage> {
    let image = image::load_from_memory(bytes).ok()?.to_rgb8();
    let (width, height) = image.dimensions();
    Some(EmbeddedRasterImage {
        width,
        height,
        rgb: image.into_raw(),
    })
}

pub fn parse_svg(text: &str) -> Option<VectorScene> {
    parse_svg_with_embedded_assets(text, &mut |_| None)
}

pub fn resolve_svg_embedded_asset_ref(svg_asset_ref: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty()
        || href.starts_with('#')
        || href.starts_with('/')
        || href.contains('\\')
        || href.chars().any(|ch| ch.is_ascii_control())
    {
        return None;
    }
    let percent_decode_path_component = |raw: &str| {
        let bytes = raw.as_bytes();
        let mut decoded = String::with_capacity(raw.len());
        let mut index = 0usize;
        while index < bytes.len() {
            if bytes[index] == b'%' {
                let hex = |byte: u8| -> Option<u8> {
                    match byte {
                        b'0'..=b'9' => Some(byte - b'0'),
                        b'a'..=b'f' => Some(byte - b'a' + 10),
                        b'A'..=b'F' => Some(byte - b'A' + 10),
                        _ => None,
                    }
                };
                let mut run_index = index;
                let mut run_bytes = Vec::new();
                let mut run_raw = Vec::new();
                while let (Some(high), Some(low)) = (
                    bytes.get(run_index + 1).copied().and_then(hex),
                    bytes.get(run_index + 2).copied().and_then(hex),
                ) {
                    let byte = (high << 4) | low;
                    if byte.is_ascii_control() || matches!(byte, b'/' | b'\\') {
                        break;
                    }
                    run_bytes.push(byte);
                    run_raw.push(&raw[run_index..run_index + 3]);
                    run_index += 3;
                    if bytes.get(run_index).copied() != Some(b'%') {
                        break;
                    }
                }
                if !run_bytes.is_empty() {
                    if let Ok(text) = std::str::from_utf8(&run_bytes) {
                        decoded.push_str(text);
                    } else {
                        for (byte, raw_escape) in run_bytes.into_iter().zip(run_raw) {
                            if byte.is_ascii() {
                                decoded.push(char::from(byte));
                            } else {
                                decoded.push_str(raw_escape);
                            }
                        }
                    }
                    index = run_index;
                    continue;
                }
            }
            let ch = raw[index..].chars().next().unwrap();
            decoded.push(ch);
            index += ch.len_utf8();
        }
        decoded
    };
    let first_component = href
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    let decoded_first_component = percent_decode_path_component(first_component);
    if first_component.contains(':') || decoded_first_component.contains(':') {
        return None;
    }
    let href = href.split(['?', '#']).next().unwrap_or_default().trim();
    if href.is_empty() {
        return None;
    }
    let mut parts = svg_asset_ref
        .rsplit_once('/')
        .map(|(parent, _)| parent.split('/').map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    for component in href.split('/') {
        let component = percent_decode_path_component(component);
        match component.as_str() {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(component),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn find_simple_xml_tag_end(tag_tail: &str) -> Option<usize> {
    let mut active_quote = None;
    for (index, ch) in tag_tail.char_indices() {
        match active_quote {
            Some(quote) if ch == quote => active_quote = None,
            None if ch == '"' || ch == '\'' => active_quote = Some(ch),
            None if ch == '>' => return Some(index),
            _ => {}
        }
    }
    None
}

fn decode_simple_xml_attribute_value(raw_value: &str) -> String {
    let mut decoded = String::new();
    let mut remaining = raw_value;
    while let Some(entity_start) = remaining.find('&') {
        decoded.push_str(&remaining[..entity_start]);
        let entity_tail = &remaining[entity_start + 1..];
        let Some(entity_end) = entity_tail.find(';') else {
            decoded.push_str(&remaining[entity_start..]);
            return decoded;
        };
        let entity = &entity_tail[..entity_end];
        let numeric_char = if let Some(hex) = entity
            .strip_prefix("#x")
            .or_else(|| entity.strip_prefix("#X"))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(decimal) = entity.strip_prefix('#') {
            decimal.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            None
        };
        match (entity, numeric_char) {
            ("lt", _) => decoded.push('<'),
            ("gt", _) => decoded.push('>'),
            ("quot", _) => decoded.push('"'),
            ("apos", _) => decoded.push('\''),
            ("amp", _) => decoded.push('&'),
            (_, Some(ch)) => decoded.push(ch),
            _ => {
                decoded.push('&');
                decoded.push_str(entity);
                decoded.push(';');
            }
        }
        remaining = &entity_tail[entity_end + 1..];
    }
    decoded.push_str(remaining);
    decoded
}

/// Rewrites external raster references so the SVG can be embedded without loading remote data.
///
/// Resolved PNG and JPEG assets become data URIs. Unresolved or unsupported external references
/// are replaced with an empty data URI, while fragment and existing data URI references are left
/// unchanged.
pub fn rewrite_svg_for_embedding(
    text: &str,
    svg_asset_ref: &str,
    mut resolve_embedded_asset: impl FnMut(&str) -> Option<Vec<u8>>,
) -> String {
    let mut rewritten = String::new();
    let mut cursor = 0usize;
    while let Some(relative) = text[cursor..].find("<image") {
        let tag_start = cursor + relative;
        let tag_tail = &text[tag_start..];
        let tag_name_boundary = tag_tail
            .strip_prefix("<image")
            .and_then(|tail| tail.chars().next())
            .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '>' | '/'));
        if !tag_name_boundary {
            let advance = "<image".len();
            rewritten.push_str(&text[cursor..tag_start + advance]);
            cursor = tag_start + advance;
            continue;
        }
        let Some(tag_end_relative) = find_simple_xml_tag_end(tag_tail) else {
            break;
        };
        let tag_end = tag_start + tag_end_relative + 1;
        let tag = &text[tag_start..tag_end];
        let mut href_attrs = Vec::<(usize, usize, usize)>::new();
        for attr_name in ["href", "xlink:href"] {
            let mut attr_search = 0usize;
            while let Some(relative) = tag[attr_search..].find(attr_name) {
                let attr_start = attr_search + relative;
                let mut active_quote = None;
                for ch in tag[..attr_start].chars() {
                    match active_quote {
                        Some(quote) if ch == quote => active_quote = None,
                        None if ch == '"' || ch == '\'' => active_quote = Some(ch),
                        _ => {}
                    }
                }
                if active_quote.is_some() {
                    attr_search = attr_start + attr_name.len();
                    continue;
                }
                let before = tag[..attr_start].chars().next_back();
                if before.is_some_and(|ch| !(ch.is_whitespace() || matches!(ch, '<' | '/'))) {
                    attr_search = attr_start + attr_name.len();
                    continue;
                }
                let after_attr_start = attr_start + attr_name.len();
                let after_attr = &tag[after_attr_start..];
                let after_whitespace = after_attr.trim_start();
                let whitespace_len = after_attr.len() - after_whitespace.len();
                let Some(after_equals) = after_whitespace.strip_prefix('=') else {
                    attr_search = after_attr_start;
                    continue;
                };
                let after_equals_whitespace = after_equals.trim_start();
                let equals_whitespace_len = after_equals.len() - after_equals_whitespace.len();
                let Some(quote) = after_equals_whitespace.chars().next() else {
                    attr_search = after_attr_start;
                    continue;
                };
                if quote != '"' && quote != '\'' {
                    attr_search = after_attr_start;
                    continue;
                }
                let value_start = after_attr_start
                    + whitespace_len
                    + '='.len_utf8()
                    + equals_whitespace_len
                    + quote.len_utf8();
                let Some(value_end_relative) = tag[value_start..].find(quote) else {
                    attr_search = after_attr_start;
                    continue;
                };
                let value_end = value_start + value_end_relative;
                href_attrs.push((attr_start, value_start, value_end));
                attr_search = value_end + quote.len_utf8();
            }
        }
        if href_attrs.is_empty() {
            rewritten.push_str(&text[cursor..tag_end]);
            cursor = tag_end;
            continue;
        }
        href_attrs.sort_by_key(|(attr_start, _, _)| *attr_start);
        let mut replacements = Vec::<(usize, usize, String)>::new();
        for (_, href_value_start_in_tag, href_value_end_in_tag) in href_attrs {
            let href_value_start = tag_start + href_value_start_in_tag;
            let href_value_end = tag_start + href_value_end_in_tag;
            let decoded_href =
                decode_simple_xml_attribute_value(&text[href_value_start..href_value_end]);
            let replacement = resolve_svg_embedded_asset_ref(svg_asset_ref, &decoded_href)
                .and_then(|asset_ref| {
                    let asset_bytes = resolve_embedded_asset(&asset_ref)?;
                    let lower = asset_ref.to_ascii_lowercase();
                    let media_type = if lower.ends_with(".png")
                        || asset_bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    {
                        Some("image/png")
                    } else if lower.ends_with(".jpg")
                        || lower.ends_with(".jpeg")
                        || asset_bytes.starts_with(&[0xff, 0xd8, 0xff])
                    {
                        Some("image/jpeg")
                    } else {
                        None
                    }?;
                    let mut data_uri = format!("data:{media_type},");
                    for byte in asset_bytes {
                        match byte {
                            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                                data_uri.push(byte as char)
                            }
                            _ => data_uri.push_str(&format!("%{byte:02X}")),
                        }
                    }
                    Some(data_uri)
                })
                .or_else(|| {
                    let trimmed_href = decoded_href.trim();
                    let lower_href = trimmed_href.to_ascii_lowercase();
                    if trimmed_href.is_empty()
                        || trimmed_href.starts_with('#')
                        || lower_href.starts_with("data:")
                    {
                        None
                    } else {
                        Some("data:,".to_string())
                    }
                });
            if let Some(replacement) = replacement {
                replacements.push((href_value_start, href_value_end, replacement));
            }
        }
        if replacements.is_empty() {
            rewritten.push_str(&text[cursor..tag_end]);
        } else {
            let mut rewrite_cursor = cursor;
            for (href_value_start, href_value_end, replacement) in replacements {
                rewritten.push_str(&text[rewrite_cursor..href_value_start]);
                rewritten.push_str(&replacement);
                rewrite_cursor = href_value_end;
            }
            rewritten.push_str(&text[rewrite_cursor..tag_end]);
        }
        cursor = tag_end;
    }
    rewritten.push_str(&text[cursor..]);
    rewritten
}

pub fn prepare_svg_materialization(
    request: &GraphicAssetRequest,
    materialized: MaterializedGraphicAsset,
    mut resolve_asset: impl FnMut(&str) -> Option<Vec<u8>>,
) -> MaterializedGraphicAsset {
    if materialized.format != GraphicAssetFormat::Svg {
        return materialized;
    }
    let Ok(svg_text) = std::str::from_utf8(&materialized.bytes) else {
        return materialized;
    };
    let Some(scene) = parse_svg_with_embedded_assets(svg_text, &mut |href| {
        resolve_svg_embedded_asset_ref(&request.asset_ref, href)
            .and_then(|asset_ref| resolve_asset(&asset_ref))
    }) else {
        return materialized;
    };
    let embeddable_svg = rewrite_svg_for_embedding(svg_text, &request.asset_ref, resolve_asset);
    materialized.with_vector_scene(scene, embeddable_svg)
}

pub fn svg_embedded_asset_refs(text: &str, svg_asset_ref: &str) -> Vec<String> {
    let mut asset_refs = std::collections::BTreeSet::new();
    let _ = rewrite_svg_for_embedding(text, svg_asset_ref, |asset_ref| {
        asset_refs.insert(asset_ref.to_string());
        None
    });
    asset_refs.into_iter().collect()
}

pub fn parse_svg_with_embedded_assets(
    text: &str,
    resolve_embedded_asset: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Option<VectorScene> {
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
        let tag_end = find_simple_xml_tag_end(tag_tail)?;
        break (tag_start, tag_end);
    };
    let tag_tail = &text[tag_start..];
    let svg_tag = &tag_tail[..tag_end];
    let attr_value = |tag: &str, name: &str| -> Option<String> {
        let mut offset = 0usize;
        while let Some(relative) = tag[offset..].find(name) {
            let index = offset + relative;
            let mut active_quote = None;
            for ch in tag[..index].chars() {
                match active_quote {
                    Some(quote) if ch == quote => active_quote = None,
                    None if ch == '"' || ch == '\'' => active_quote = Some(ch),
                    _ => {}
                }
            }
            if active_quote.is_some() {
                offset = index + name.len();
                continue;
            }
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
            let raw_value = &after[value_start..value_end];
            return Some(decode_simple_xml_attribute_value(raw_value));
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
    let parse_view_box = |tag: &str| -> Option<(f32, f32, f32, f32)> {
        attr_value(tag, "viewBox")
            .or_else(|| attr_value(tag, "viewbox"))
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
            })
    };
    let view_box = parse_view_box(svg_tag);
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
    let normalized_viewport_diagonal =
        ((view_box.2 * view_box.2 + view_box.3 * view_box.3) / 2.0).sqrt();
    let parse_percentage_ratio = |raw: &str| -> Option<f32> {
        raw.trim()
            .strip_suffix('%')?
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| value / 100.0)
    };
    let parse_x_length = |raw: &str| -> Option<f32> {
        parse_percentage_ratio(raw)
            .map(|ratio| view_box.2 * ratio)
            .or_else(|| parse_number_prefix(raw))
    };
    let parse_y_length = |raw: &str| -> Option<f32> {
        parse_percentage_ratio(raw)
            .map(|ratio| view_box.3 * ratio)
            .or_else(|| parse_number_prefix(raw))
    };
    let parse_diagonal_length = |raw: &str| -> Option<f32> {
        parse_percentage_ratio(raw)
            .map(|ratio| normalized_viewport_diagonal * ratio)
            .or_else(|| parse_number_prefix(raw))
    };
    let default_preserve_aspect_ratio = VectorPreserveAspectRatio {
        x_align: VectorAspectAlign::Mid,
        y_align: VectorAspectAlign::Mid,
        scale: VectorAspectScale::Meet,
    };
    let parse_preserve_aspect_ratio = |raw: &str| -> Option<VectorPreserveAspectRatio> {
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
            return Some(VectorPreserveAspectRatio {
                x_align: VectorAspectAlign::Mid,
                y_align: VectorAspectAlign::Mid,
                scale: VectorAspectScale::None,
            });
        }
        let (x_align, y_align) = match *align {
            "xMinYMin" => (VectorAspectAlign::Min, VectorAspectAlign::Min),
            "xMidYMin" => (VectorAspectAlign::Mid, VectorAspectAlign::Min),
            "xMaxYMin" => (VectorAspectAlign::Max, VectorAspectAlign::Min),
            "xMinYMid" => (VectorAspectAlign::Min, VectorAspectAlign::Mid),
            "xMidYMid" => (VectorAspectAlign::Mid, VectorAspectAlign::Mid),
            "xMaxYMid" => (VectorAspectAlign::Max, VectorAspectAlign::Mid),
            "xMinYMax" => (VectorAspectAlign::Min, VectorAspectAlign::Max),
            "xMidYMax" => (VectorAspectAlign::Mid, VectorAspectAlign::Max),
            "xMaxYMax" => (VectorAspectAlign::Max, VectorAspectAlign::Max),
            _ => return None,
        };
        let scale = match parts.get(align_index + 1).copied().unwrap_or("meet") {
            "meet" => VectorAspectScale::Meet,
            "slice" => VectorAspectScale::Slice,
            _ => return None,
        };
        Some(VectorPreserveAspectRatio {
            x_align,
            y_align,
            scale,
        })
    };
    let preserve_aspect_ratio = attr_value(svg_tag, "preserveAspectRatio")
        .as_deref()
        .and_then(|raw| parse_preserve_aspect_ratio(raw))
        .unwrap_or(default_preserve_aspect_ratio);
    let svg_content_start = tag_start + tag_end + 1;
    let svg_content_end = text[svg_content_start..]
        .find("</svg>")
        .map(|relative| svg_content_start + relative)
        .unwrap_or(text.len());
    let svg_content = &text[svg_content_start..svg_content_end];
    let strip_important_marker = |value: &str| {
        let value = value.trim();
        if let Some(marker_index) = value.rfind('!') {
            let marker = value[marker_index + 1..].trim();
            if marker.eq_ignore_ascii_case("important") {
                return value[..marker_index].trim().to_string();
            }
        }
        value.to_string()
    };
    let declaration_value = |declarations: &str, name: &str| -> Option<String> {
        let mut matched_value = None;
        for declaration in declarations.split(';') {
            let Some((key, value)) = declaration.split_once(':') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case(name) {
                matched_value = Some(strip_important_marker(value));
            }
        }
        matched_value
    };
    let style_value = |tag: &str, name: &str| -> Option<String> {
        let style = attr_value(tag, "style")?;
        declaration_value(&style, name)
    };
    let parse_color = |raw: &str| -> Option<VectorResolvedColor> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("transparent") {
            return None;
        }
        let color = |rgb: (f32, f32, f32)| VectorResolvedColor { rgb, alpha: 1.0 };
        let color_255 =
            |r: u8, g: u8, b: u8| color((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0));
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
        let parse_hue_component = |component: &str| -> Option<f32> {
            let component = component.trim();
            let (raw, multiplier) = if let Some(value) = component.strip_suffix("deg") {
                (value.trim(), 1.0)
            } else if let Some(value) = component.strip_suffix("turn") {
                (value.trim(), 360.0)
            } else if let Some(value) = component.strip_suffix("rad") {
                (value.trim(), 180.0 / std::f32::consts::PI)
            } else if let Some(value) = component.strip_suffix("grad") {
                (value.trim(), 0.9)
            } else {
                (component, 1.0)
            };
            raw.parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| (value * multiplier).rem_euclid(360.0) / 360.0)
        };
        let parse_percent_component = |component: &str| -> Option<f32> {
            component
                .trim()
                .strip_suffix('%')?
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| (value / 100.0).clamp(0.0, 1.0))
        };
        let hsl_to_rgb = |hue: f32, saturation: f32, lightness: f32| -> (f32, f32, f32) {
            if saturation <= 0.0 {
                return (lightness, lightness, lightness);
            }
            let normalize_component = |value: f32| {
                let value = value.clamp(0.0, 1.0);
                if value < 0.000_5 {
                    0.0
                } else if value > 0.999_5 {
                    1.0
                } else {
                    value
                }
            };
            let q = if lightness < 0.5 {
                lightness * (1.0 + saturation)
            } else {
                lightness + saturation - lightness * saturation
            };
            let p = 2.0 * lightness - q;
            let hue_channel = |mut t: f32| {
                if t < 0.0 {
                    t += 1.0;
                }
                if t > 1.0 {
                    t -= 1.0;
                }
                if t < 1.0 / 6.0 {
                    p + (q - p) * 6.0 * t
                } else if t < 0.5 {
                    q
                } else if t < 2.0 / 3.0 {
                    p + (q - p) * (2.0 / 3.0 - t) * 6.0
                } else {
                    p
                }
            };
            (
                normalize_component(hue_channel(hue + 1.0 / 3.0)),
                normalize_component(hue_channel(hue)),
                normalize_component(hue_channel(hue - 1.0 / 3.0)),
            )
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
                return Some(VectorResolvedColor {
                    rgb: (
                        parse_rgb_component(components[0])?,
                        parse_rgb_component(components[1])?,
                        parse_rgb_component(components[2])?,
                    ),
                    alpha,
                });
            }
        }
        if raw.ends_with(')') {
            let (body, is_hsla) = if raw.len() >= 5 && raw[..4].eq_ignore_ascii_case("hsl(") {
                (&raw[4..raw.len() - 1], false)
            } else if raw.len() >= 6 && raw[..5].eq_ignore_ascii_case("hsla(") {
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
                let comma_alpha = if is_hsla && components.len() >= 4 {
                    Some(components[3])
                } else {
                    None
                };
                let alpha = if let Some(alpha) = slash_alpha.or(comma_alpha) {
                    parse_alpha_component(alpha)?
                } else {
                    1.0
                };
                return Some(VectorResolvedColor {
                    rgb: hsl_to_rgb(
                        parse_hue_component(components[0])?,
                        parse_percent_component(components[1])?,
                        parse_percent_component(components[2])?,
                    ),
                    alpha,
                });
            }
        }
        if let Some(hex) = raw.strip_prefix('#') {
            if hex.len() == 6 || hex.len() == 8 {
                return Some(VectorResolvedColor {
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
                return Some(VectorResolvedColor {
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
            "aliceblue" => Some(color_255(240, 248, 255)),
            "antiquewhite" => Some(color_255(250, 235, 215)),
            "aqua" | "cyan" => Some(color_255(0, 255, 255)),
            "aquamarine" => Some(color_255(127, 255, 212)),
            "azure" => Some(color_255(240, 255, 255)),
            "beige" => Some(color_255(245, 245, 220)),
            "bisque" => Some(color_255(255, 228, 196)),
            "black" => Some(color_255(0, 0, 0)),
            "blanchedalmond" => Some(color_255(255, 235, 205)),
            "blue" => Some(color_255(0, 0, 255)),
            "blueviolet" => Some(color_255(138, 43, 226)),
            "brown" => Some(color_255(165, 42, 42)),
            "burlywood" => Some(color_255(222, 184, 135)),
            "cadetblue" => Some(color_255(95, 158, 160)),
            "chartreuse" => Some(color_255(127, 255, 0)),
            "chocolate" => Some(color_255(210, 105, 30)),
            "coral" => Some(color_255(255, 127, 80)),
            "cornflowerblue" => Some(color_255(100, 149, 237)),
            "cornsilk" => Some(color_255(255, 248, 220)),
            "crimson" => Some(color_255(220, 20, 60)),
            "darkblue" => Some(color_255(0, 0, 139)),
            "darkcyan" => Some(color_255(0, 139, 139)),
            "darkgoldenrod" => Some(color_255(184, 134, 11)),
            "darkgray" | "darkgrey" => Some(color_255(169, 169, 169)),
            "darkgreen" => Some(color_255(0, 100, 0)),
            "darkkhaki" => Some(color_255(189, 183, 107)),
            "darkmagenta" => Some(color_255(139, 0, 139)),
            "darkolivegreen" => Some(color_255(85, 107, 47)),
            "darkorange" => Some(color_255(255, 140, 0)),
            "darkorchid" => Some(color_255(153, 50, 204)),
            "darkred" => Some(color_255(139, 0, 0)),
            "darksalmon" => Some(color_255(233, 150, 122)),
            "darkseagreen" => Some(color_255(143, 188, 143)),
            "darkslateblue" => Some(color_255(72, 61, 139)),
            "darkslategray" | "darkslategrey" => Some(color_255(47, 79, 79)),
            "darkturquoise" => Some(color_255(0, 206, 209)),
            "darkviolet" => Some(color_255(148, 0, 211)),
            "deeppink" => Some(color_255(255, 20, 147)),
            "deepskyblue" => Some(color_255(0, 191, 255)),
            "dimgray" | "dimgrey" => Some(color_255(105, 105, 105)),
            "dodgerblue" => Some(color_255(30, 144, 255)),
            "firebrick" => Some(color_255(178, 34, 34)),
            "floralwhite" => Some(color_255(255, 250, 240)),
            "forestgreen" => Some(color_255(34, 139, 34)),
            "fuchsia" | "magenta" => Some(color_255(255, 0, 255)),
            "gainsboro" => Some(color_255(220, 220, 220)),
            "ghostwhite" => Some(color_255(248, 248, 255)),
            "gold" => Some(color_255(255, 215, 0)),
            "goldenrod" => Some(color_255(218, 165, 32)),
            "gray" | "grey" => Some(color_255(128, 128, 128)),
            "green" => Some(color_255(0, 128, 0)),
            "greenyellow" => Some(color_255(173, 255, 47)),
            "honeydew" => Some(color_255(240, 255, 240)),
            "hotpink" => Some(color_255(255, 105, 180)),
            "indianred" => Some(color_255(205, 92, 92)),
            "indigo" => Some(color_255(75, 0, 130)),
            "ivory" => Some(color_255(255, 255, 240)),
            "khaki" => Some(color_255(240, 230, 140)),
            "lavender" => Some(color_255(230, 230, 250)),
            "lavenderblush" => Some(color_255(255, 240, 245)),
            "lawngreen" => Some(color_255(124, 252, 0)),
            "lemonchiffon" => Some(color_255(255, 250, 205)),
            "lightblue" => Some(color_255(173, 216, 230)),
            "lightcoral" => Some(color_255(240, 128, 128)),
            "lightcyan" => Some(color_255(224, 255, 255)),
            "lightgoldenrodyellow" => Some(color_255(250, 250, 210)),
            "lightgray" | "lightgrey" => Some(color_255(211, 211, 211)),
            "lightgreen" => Some(color_255(144, 238, 144)),
            "lightpink" => Some(color_255(255, 182, 193)),
            "lightsalmon" => Some(color_255(255, 160, 122)),
            "lightseagreen" => Some(color_255(32, 178, 170)),
            "lightskyblue" => Some(color_255(135, 206, 250)),
            "lightslategray" | "lightslategrey" => Some(color_255(119, 136, 153)),
            "lightsteelblue" => Some(color_255(176, 196, 222)),
            "lightyellow" => Some(color_255(255, 255, 224)),
            "lime" => Some(color_255(0, 255, 0)),
            "limegreen" => Some(color_255(50, 205, 50)),
            "linen" => Some(color_255(250, 240, 230)),
            "maroon" => Some(color_255(128, 0, 0)),
            "mediumaquamarine" => Some(color_255(102, 205, 170)),
            "mediumblue" => Some(color_255(0, 0, 205)),
            "mediumorchid" => Some(color_255(186, 85, 211)),
            "mediumpurple" => Some(color_255(147, 112, 219)),
            "mediumseagreen" => Some(color_255(60, 179, 113)),
            "mediumslateblue" => Some(color_255(123, 104, 238)),
            "mediumspringgreen" => Some(color_255(0, 250, 154)),
            "mediumturquoise" => Some(color_255(72, 209, 204)),
            "mediumvioletred" => Some(color_255(199, 21, 133)),
            "midnightblue" => Some(color_255(25, 25, 112)),
            "mintcream" => Some(color_255(245, 255, 250)),
            "mistyrose" => Some(color_255(255, 228, 225)),
            "moccasin" => Some(color_255(255, 228, 181)),
            "navajowhite" => Some(color_255(255, 222, 173)),
            "navy" => Some(color_255(0, 0, 128)),
            "oldlace" => Some(color_255(253, 245, 230)),
            "olive" => Some(color_255(128, 128, 0)),
            "olivedrab" => Some(color_255(107, 142, 35)),
            "orange" => Some(color_255(255, 165, 0)),
            "orangered" => Some(color_255(255, 69, 0)),
            "orchid" => Some(color_255(218, 112, 214)),
            "palegoldenrod" => Some(color_255(238, 232, 170)),
            "palegreen" => Some(color_255(152, 251, 152)),
            "paleturquoise" => Some(color_255(175, 238, 238)),
            "palevioletred" => Some(color_255(219, 112, 147)),
            "papayawhip" => Some(color_255(255, 239, 213)),
            "peachpuff" => Some(color_255(255, 218, 185)),
            "peru" => Some(color_255(205, 133, 63)),
            "pink" => Some(color_255(255, 192, 203)),
            "plum" => Some(color_255(221, 160, 221)),
            "powderblue" => Some(color_255(176, 224, 230)),
            "purple" => Some(color_255(128, 0, 128)),
            "rebeccapurple" => Some(color_255(102, 51, 153)),
            "red" => Some(color_255(255, 0, 0)),
            "rosybrown" => Some(color_255(188, 143, 143)),
            "royalblue" => Some(color_255(65, 105, 225)),
            "saddlebrown" => Some(color_255(139, 69, 19)),
            "salmon" => Some(color_255(250, 128, 114)),
            "sandybrown" => Some(color_255(244, 164, 96)),
            "seagreen" => Some(color_255(46, 139, 87)),
            "seashell" => Some(color_255(255, 245, 238)),
            "sienna" => Some(color_255(160, 82, 45)),
            "silver" => Some(color_255(192, 192, 192)),
            "skyblue" => Some(color_255(135, 206, 235)),
            "slateblue" => Some(color_255(106, 90, 205)),
            "slategray" | "slategrey" => Some(color_255(112, 128, 144)),
            "snow" => Some(color_255(255, 250, 250)),
            "springgreen" => Some(color_255(0, 255, 127)),
            "steelblue" => Some(color_255(70, 130, 180)),
            "tan" => Some(color_255(210, 180, 140)),
            "teal" => Some(color_255(0, 128, 128)),
            "thistle" => Some(color_255(216, 191, 216)),
            "tomato" => Some(color_255(255, 99, 71)),
            "turquoise" => Some(color_255(64, 224, 208)),
            "violet" => Some(color_255(238, 130, 238)),
            "wheat" => Some(color_255(245, 222, 179)),
            "white" => Some(color_255(255, 255, 255)),
            "whitesmoke" => Some(color_255(245, 245, 245)),
            "yellow" => Some(color_255(255, 255, 0)),
            "yellowgreen" => Some(color_255(154, 205, 50)),
            _ => Some(color((0.0, 0.0, 0.0))),
        }
    };
    let paint_server_colors = {
        struct VectorPaintServer {
            id: String,
            href: Option<String>,
            color: Option<VectorResolvedColor>,
            current_color: VectorResolvedColor,
            current_color_stop_opacity: Option<f32>,
        }
        type StyleRuleValues = (Option<String>, Option<String>, Option<String>);
        let style_rule_values = |target_tag: &str, target_element_name: &str| -> StyleRuleValues {
            let class_attr = attr_value(target_tag, "class");
            let id_attr = attr_value(target_tag, "id");
            let selector_matches_target = |selector: &str| -> Option<u16> {
                let selector = selector.trim();
                if selector.contains('+') || selector.contains('~') {
                    return None;
                }
                let selector = selector
                    .split(|ch: char| ch.is_whitespace() || ch == '>')
                    .filter(|part| !part.is_empty())
                    .last()
                    .unwrap_or(selector)
                    .trim();
                if selector == "*" {
                    return Some(0);
                }
                if selector.contains('.') {
                    let (element_name, class_selector) =
                        if let Some(class_selector) = selector.strip_prefix('.') {
                            (None, class_selector)
                        } else {
                            let dot_index = selector.find('.')?;
                            (
                                Some(selector[..dot_index].trim()),
                                &selector[dot_index + 1..],
                            )
                        };
                    if element_name
                        .map(|element_name| !element_name.eq_ignore_ascii_case(target_element_name))
                        .unwrap_or(false)
                    {
                        return None;
                    }
                    let class_name = class_selector
                        .chars()
                        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
                        .collect::<String>();
                    if class_name.is_empty() {
                        return None;
                    }
                    let matches = class_attr
                        .as_ref()
                        .map(|class_attr| {
                            class_attr
                                .split_whitespace()
                                .any(|tag_class_name| tag_class_name == class_name)
                        })
                        .unwrap_or(false);
                    return matches.then_some(10 + u16::from(element_name.is_some()));
                }
                if selector.contains('#') {
                    let (element_name, id_selector) =
                        if let Some(id_selector) = selector.strip_prefix('#') {
                            (None, id_selector)
                        } else {
                            let hash_index = selector.find('#')?;
                            (
                                Some(selector[..hash_index].trim()),
                                &selector[hash_index + 1..],
                            )
                        };
                    if element_name
                        .map(|element_name| !element_name.eq_ignore_ascii_case(target_element_name))
                        .unwrap_or(false)
                    {
                        return None;
                    }
                    let id = id_selector
                        .chars()
                        .take_while(|ch| {
                            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':')
                        })
                        .collect::<String>();
                    if id.is_empty() {
                        return None;
                    }
                    return (id_attr.as_deref() == Some(id.as_str()))
                        .then_some(100 + u16::from(element_name.is_some()));
                }
                selector
                    .eq_ignore_ascii_case(target_element_name)
                    .then_some(1)
            };
            let should_replace = |current: Option<(u16, usize)>, specificity: u16, order: usize| {
                current
                    .map(|(current_specificity, current_order)| {
                        specificity > current_specificity
                            || (specificity == current_specificity && order >= current_order)
                    })
                    .unwrap_or(true)
            };
            let mut stop_color: Option<(String, u16, usize)> = None;
            let mut stop_opacity: Option<(String, u16, usize)> = None;
            let mut color: Option<(String, u16, usize)> = None;
            let mut style_rule_order = 0usize;
            let mut style_block_offset = 0usize;
            while let Some(style_start_relative) = svg_content[style_block_offset..].find("<style")
            {
                let style_start = style_block_offset + style_start_relative;
                let style_tag_tail = &svg_content[style_start..];
                if !is_start_tag_named(style_tag_tail, "style") {
                    style_block_offset = style_start + "<style".len();
                    continue;
                }
                let Some(style_tag_end) = find_simple_xml_tag_end(style_tag_tail) else {
                    break;
                };
                let content_start = style_start + style_tag_end + 1;
                let Some(content_end_relative) = svg_content[content_start..].find("</style>")
                else {
                    break;
                };
                let css = svg_content[content_start..content_start + content_end_relative].trim();
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
                    let declarations = &css[body_start..body_end];
                    let declaration_stop_color = declaration_value(declarations, "stop-color");
                    let declaration_stop_opacity = declaration_value(declarations, "stop-opacity");
                    let declaration_color = declaration_value(declarations, "color");
                    for selector in css[css_offset..selector_end].split(',') {
                        if let Some(specificity) = selector_matches_target(selector) {
                            if let Some(value) = declaration_stop_color.clone() {
                                let current = stop_color
                                    .as_ref()
                                    .map(|(_, specificity, order)| (*specificity, *order));
                                if should_replace(current, specificity, style_rule_order) {
                                    stop_color = Some((value, specificity, style_rule_order));
                                }
                            }
                            if let Some(value) = declaration_stop_opacity.clone() {
                                let current = stop_opacity
                                    .as_ref()
                                    .map(|(_, specificity, order)| (*specificity, *order));
                                if should_replace(current, specificity, style_rule_order) {
                                    stop_opacity = Some((value, specificity, style_rule_order));
                                }
                            }
                            if let Some(value) = declaration_color.clone() {
                                let current = color
                                    .as_ref()
                                    .map(|(_, specificity, order)| (*specificity, *order));
                                if should_replace(current, specificity, style_rule_order) {
                                    color = Some((value, specificity, style_rule_order));
                                }
                            }
                        }
                        style_rule_order += 1;
                    }
                    css_offset = body_end + 1;
                }
                style_block_offset = content_start + content_end_relative + "</style>".len();
            }
            (
                stop_color.map(|(value, _, _)| value),
                stop_opacity.map(|(value, _, _)| value),
                color.map(|(value, _, _)| value),
            )
        };
        let paint_server_href = |tag: &str| {
            attr_value(tag, "href")
                .or_else(|| attr_value(tag, "xlink:href"))
                .map(|href| {
                    href.trim()
                        .trim_matches(|ch| ch == '\'' || ch == '"')
                        .trim_start_matches('#')
                        .to_string()
                })
                .filter(|href| !href.is_empty())
        };
        let (_, _, style_rule_root_color) = style_rule_values(svg_tag, "svg");
        let initial_current_color = VectorResolvedColor::opaque((0.0, 0.0, 0.0));
        let resolve_current_color =
            |inline_value: Option<String>,
             style_rule_value: Option<String>,
             presentation_value: Option<String>,
             inherited: VectorResolvedColor| {
                let resolve_value = |value: &str| {
                    let value = value.trim();
                    if value.eq_ignore_ascii_case("inherit") || value.eq_ignore_ascii_case("unset")
                    {
                        return Some(inherited);
                    }
                    if value.eq_ignore_ascii_case("initial") {
                        return Some(initial_current_color);
                    }
                    parse_color(value)
                };
                if let Some(color) = inline_value.as_deref().and_then(resolve_value) {
                    return color;
                }
                if let Some(color) = style_rule_value.as_deref().and_then(resolve_value) {
                    return color;
                }
                if let Some(color) = presentation_value.as_deref().and_then(resolve_value) {
                    return color;
                }
                inherited
            };
        let current_color_depends_on_inherited =
            |inline_value: Option<&str>,
             style_rule_value: Option<&str>,
             presentation_value: Option<&str>| {
                let dependency = |value: &str| {
                    let value = value.trim();
                    if value.eq_ignore_ascii_case("inherit") || value.eq_ignore_ascii_case("unset")
                    {
                        return Some(true);
                    }
                    if value.eq_ignore_ascii_case("initial") {
                        return Some(false);
                    }
                    parse_color(value).map(|_| false)
                };
                inline_value
                    .and_then(dependency)
                    .or_else(|| style_rule_value.and_then(dependency))
                    .or_else(|| presentation_value.and_then(dependency))
                    .unwrap_or(true)
            };
        let root_current_color = resolve_current_color(
            style_value(svg_tag, "color"),
            style_rule_root_color,
            attr_value(svg_tag, "color"),
            initial_current_color,
        );
        let mut servers = Vec::new();
        for gradient_name in ["linearGradient", "radialGradient"] {
            let open_tag = format!("<{gradient_name}");
            let close_tag = format!("</{gradient_name}>");
            let mut search_index = 0usize;
            while let Some(relative) = svg_content[search_index..].find(&open_tag) {
                let gradient_start = search_index + relative;
                let gradient_tail = &svg_content[gradient_start..];
                if !is_start_tag_named(gradient_tail, gradient_name) {
                    search_index = gradient_start + open_tag.len();
                    continue;
                }
                let Some(gradient_tag_end) = find_simple_xml_tag_end(gradient_tail) else {
                    break;
                };
                let gradient_tag = &gradient_tail[..gradient_tag_end];
                let Some(id) = attr_value(gradient_tag, "id") else {
                    search_index = gradient_start + gradient_tag_end + 1;
                    continue;
                };
                let id = id.trim();
                if id.is_empty() {
                    search_index = gradient_start + gradient_tag_end + 1;
                    continue;
                }
                let href = paint_server_href(gradient_tag);
                let (_, _, style_rule_gradient_color) =
                    style_rule_values(gradient_tag, gradient_name);
                let gradient_current_color = resolve_current_color(
                    style_value(gradient_tag, "color"),
                    style_rule_gradient_color,
                    attr_value(gradient_tag, "color"),
                    root_current_color,
                );
                let body_start = gradient_start + gradient_tag_end + 1;
                let (gradient_body, next_index) = if gradient_tag.trim_end().ends_with('/') {
                    ("", body_start)
                } else if let Some(close_relative) = svg_content[body_start..].find(&close_tag) {
                    (
                        &svg_content[body_start..body_start + close_relative],
                        body_start + close_relative + close_tag.len(),
                    )
                } else {
                    (&svg_content[body_start..], svg_content.len())
                };
                let mut server_color = None;
                let mut server_current_color_stop_opacity = None;
                let mut stop_search_index = 0usize;
                while let Some(stop_relative) = gradient_body[stop_search_index..].find("<stop") {
                    let stop_start = stop_search_index + stop_relative;
                    let stop_tail = &gradient_body[stop_start..];
                    if !is_start_tag_named(stop_tail, "stop") {
                        stop_search_index = stop_start + "<stop".len();
                        continue;
                    }
                    let Some(stop_tag_end) = find_simple_xml_tag_end(stop_tail) else {
                        break;
                    };
                    let stop_tag = &stop_tail[..stop_tag_end];
                    let (style_rule_stop_color, style_rule_stop_opacity, style_rule_color) =
                        style_rule_values(stop_tag, "stop");
                    let stop_inline_color = style_value(stop_tag, "color");
                    let stop_presentation_color = attr_value(stop_tag, "color");
                    let stop_current_color_depends_on_gradient = current_color_depends_on_inherited(
                        stop_inline_color.as_deref(),
                        style_rule_color.as_deref(),
                        stop_presentation_color.as_deref(),
                    );
                    let stop_current_color = resolve_current_color(
                        stop_inline_color,
                        style_rule_color,
                        stop_presentation_color,
                        gradient_current_color,
                    );
                    let stop_color_value = style_value(stop_tag, "stop-color")
                        .or(style_rule_stop_color)
                        .or_else(|| attr_value(stop_tag, "stop-color"));
                    let mut uses_current_color_stop = false;
                    let mut color = if let Some(stop_color_value) = stop_color_value {
                        let stop_color_value = stop_color_value.trim();
                        if stop_color_value.eq_ignore_ascii_case("currentColor") {
                            uses_current_color_stop = true;
                            stop_current_color
                        } else if stop_color_value.eq_ignore_ascii_case("transparent") {
                            VectorResolvedColor {
                                rgb: (0.0, 0.0, 0.0),
                                alpha: 0.0,
                            }
                        } else {
                            parse_color(stop_color_value)
                                .unwrap_or_else(|| VectorResolvedColor::opaque((0.0, 0.0, 0.0)))
                        }
                    } else {
                        VectorResolvedColor::opaque((0.0, 0.0, 0.0))
                    };
                    let mut stop_opacity_multiplier = 1.0;
                    if let Some(opacity) = style_value(stop_tag, "stop-opacity")
                        .or(style_rule_stop_opacity)
                        .or_else(|| attr_value(stop_tag, "stop-opacity"))
                    {
                        let opacity = opacity.trim();
                        let parsed_opacity = if let Some(percent) = opacity.strip_suffix('%') {
                            percent
                                .trim()
                                .parse::<f32>()
                                .ok()
                                .filter(|value| value.is_finite())
                                .map(|value| value / 100.0)
                        } else {
                            opacity
                                .parse::<f32>()
                                .ok()
                                .filter(|value| value.is_finite())
                        };
                        if let Some(parsed_opacity) = parsed_opacity {
                            stop_opacity_multiplier = parsed_opacity.clamp(0.0, 1.0);
                            color.alpha *= stop_opacity_multiplier;
                        }
                    }
                    if uses_current_color_stop && stop_current_color_depends_on_gradient {
                        server_current_color_stop_opacity = Some(stop_opacity_multiplier);
                    }
                    server_color = Some(color);
                    break;
                }
                servers.push(VectorPaintServer {
                    id: id.to_string(),
                    href,
                    color: server_color,
                    current_color: gradient_current_color,
                    current_color_stop_opacity: server_current_color_stop_opacity,
                });
                search_index = next_index;
            }
        }
        let mut colors = Vec::new();
        for server in &servers {
            let mut color = server.color;
            let mut href = server.href.as_deref();
            for _ in 0..8 {
                if color.is_some() {
                    break;
                }
                let Some(referenced_id) = href else {
                    break;
                };
                let Some(referenced) = servers
                    .iter()
                    .rev()
                    .find(|candidate| candidate.id == referenced_id && candidate.id != server.id)
                else {
                    break;
                };
                if let Some(opacity) = referenced.current_color_stop_opacity {
                    color = Some(VectorResolvedColor {
                        rgb: server.current_color.rgb,
                        alpha: server.current_color.alpha * opacity,
                    });
                    break;
                }
                color = referenced.color;
                href = referenced.href.as_deref();
            }
            if let Some(color) = color {
                colors.push((server.id.clone(), color));
            }
        }
        colors
    };
    let parse_paint = |raw: &str| -> Option<VectorColor> {
        let raw = raw.trim();
        if raw.len() >= 4 && raw[..4].eq_ignore_ascii_case("url(") {
            let Some(url_end) = raw.find(')') else {
                return Some(VectorColor::Resolved(VectorResolvedColor::opaque((
                    0.0, 0.0, 0.0,
                ))));
            };
            let paint_server_ref = raw[4..url_end]
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"');
            if let Some(id) = paint_server_ref.strip_prefix('#')
                && let Some((_, color)) = paint_server_colors
                    .iter()
                    .rev()
                    .find(|(candidate_id, _)| candidate_id == id)
            {
                return Some(VectorColor::Resolved(*color));
            }
            let fallback = raw[url_end + 1..].trim();
            if fallback.is_empty() {
                return Some(VectorColor::Resolved(VectorResolvedColor::opaque((
                    0.0, 0.0, 0.0,
                ))));
            }
            if fallback.eq_ignore_ascii_case("none") || fallback.eq_ignore_ascii_case("transparent")
            {
                return None;
            }
            if fallback.eq_ignore_ascii_case("currentColor") {
                return Some(VectorColor::CurrentColor);
            }
            if fallback.eq_ignore_ascii_case("context-fill") {
                return Some(VectorColor::ContextFill);
            }
            if fallback.eq_ignore_ascii_case("context-stroke") {
                return Some(VectorColor::ContextStroke);
            }
            return parse_color(fallback).map(VectorColor::Resolved);
        }
        if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("transparent") {
            return None;
        }
        if raw.eq_ignore_ascii_case("currentColor") {
            return Some(VectorColor::CurrentColor);
        }
        if raw.eq_ignore_ascii_case("context-fill") {
            return Some(VectorColor::ContextFill);
        }
        if raw.eq_ignore_ascii_case("context-stroke") {
            return Some(VectorColor::ContextStroke);
        }
        parse_color(raw).map(VectorColor::Resolved)
    };
    #[derive(Debug, Clone, Copy)]
    struct VectorResolvedColor {
        rgb: (f32, f32, f32),
        alpha: f32,
    }
    impl VectorResolvedColor {
        fn opaque(rgb: (f32, f32, f32)) -> Self {
            Self { rgb, alpha: 1.0 }
        }
    }
    #[derive(Debug, Clone, Copy)]
    enum VectorColor {
        Resolved(VectorResolvedColor),
        CurrentColor,
        ContextFill,
        ContextStroke,
    }
    #[derive(Debug, Clone, Copy)]
    enum VectorFontSize {
        Absolute(f32),
        Percent(f32),
    }
    #[derive(Debug, Clone, Copy)]
    enum VectorBaselineShift {
        Offset(f32),
        Percent(f32),
        Super,
        Sub,
    }
    #[derive(Debug, Clone, Copy, Default)]
    struct VectorPresentation {
        // Outer Option means "specified"; inner Option preserves SVG paint "none".
        fill: Option<Option<VectorColor>>,
        fill_rule: Option<VectorFillRule>,
        stroke: Option<Option<VectorColor>>,
        stroke_width: Option<f32>,
        stroke_dasharray: Option<Option<VectorDashArray>>,
        stroke_dashoffset: Option<f32>,
        stroke_linecap: Option<VectorStrokeLineCap>,
        stroke_linejoin: Option<VectorStrokeLineJoin>,
        stroke_miterlimit: Option<f32>,
        paint_order: Option<VectorPaintOrder>,
        color: Option<VectorResolvedColor>,
        display: Option<bool>,
        visibility: Option<bool>,
        opacity: Option<f32>,
        fill_opacity: Option<f32>,
        stroke_opacity: Option<f32>,
        text_anchor: Option<VectorTextAnchor>,
        font_size: Option<VectorFontSize>,
        font_family: Option<VectorFontFamily>,
        font_series: Option<FontSeries>,
        font_shape: Option<FontShape>,
        letter_spacing: Option<f32>,
        word_spacing: Option<f32>,
        text_decoration: Option<VectorTextDecoration>,
        text_decoration_color: Option<Option<VectorColor>>,
        text_decoration_thickness: Option<f32>,
        text_decoration_style: Option<VectorTextDecorationStyle>,
        text_baseline: Option<VectorTextBaseline>,
        baseline_shift: Option<VectorBaselineShift>,
        vector_effect_non_scaling_stroke: Option<bool>,
        marker_start: Option<Option<u64>>,
        marker_mid: Option<Option<u64>>,
        marker_end: Option<Option<u64>>,
        clip_path: Option<Option<u64>>,
    }
    let parse_opacity = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("initial") || raw.eq_ignore_ascii_case("unset") {
            return Some(1.0);
        }
        if raw.eq_ignore_ascii_case("inherit") {
            return None;
        }
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
    let parse_stroke_length = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("initial") {
            return Some(0.0);
        }
        parse_diagonal_length(raw).filter(|value| value.is_finite())
    };
    let parse_stroke_width = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("initial") {
            return Some(1.0);
        }
        parse_stroke_length(raw).filter(|width| *width >= 0.0)
    };
    let parse_letter_spacing = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("normal") || raw.eq_ignore_ascii_case("initial") {
            return Some(0.0);
        }
        parse_x_length(raw).filter(|value| value.is_finite())
    };
    let parse_word_spacing = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("normal") || raw.eq_ignore_ascii_case("initial") {
            return Some(0.0);
        }
        parse_x_length(raw).filter(|value| value.is_finite())
    };
    let parse_text_decoration = |raw: &str| -> Option<VectorTextDecoration> {
        let raw = raw.trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("inherit") {
            return None;
        }
        if raw.eq_ignore_ascii_case("none")
            || raw.eq_ignore_ascii_case("initial")
            || raw.eq_ignore_ascii_case("unset")
        {
            return Some(VectorTextDecoration::default());
        }
        let mut decoration = VectorTextDecoration::default();
        for token in raw.split_whitespace() {
            match token.to_ascii_lowercase().as_str() {
                "underline" => decoration.underline = true,
                "overline" => decoration.overline = true,
                "line-through" => decoration.line_through = true,
                _ => {}
            }
        }
        (decoration.underline || decoration.overline || decoration.line_through)
            .then_some(decoration)
    };
    let parse_text_decoration_thickness = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("initial") || raw.eq_ignore_ascii_case("unset") {
            return Some(0.0);
        }
        if raw.is_empty()
            || raw.eq_ignore_ascii_case("inherit")
            || raw.eq_ignore_ascii_case("auto")
            || raw.eq_ignore_ascii_case("from-font")
        {
            return None;
        }
        parse_x_length(raw).filter(|value| value.is_finite() && *value > 0.0)
    };
    let parse_text_decoration_style = |raw: &str| -> Option<VectorTextDecorationStyle> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "solid" | "initial" | "unset" => Some(VectorTextDecorationStyle::Solid),
            "double" => Some(VectorTextDecorationStyle::Double),
            "wavy" => Some(VectorTextDecorationStyle::Wavy),
            "dashed" => Some(VectorTextDecorationStyle::Dashed),
            "dotted" => Some(VectorTextDecorationStyle::Dotted),
            _ => None,
        }
    };
    let parse_display = |raw: &str| -> Option<bool> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("inherit") || raw.is_empty() {
            return None;
        }
        Some(!raw.eq_ignore_ascii_case("none"))
    };
    let parse_visibility = |raw: &str| -> Option<bool> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "visible" | "initial" => Some(true),
            "hidden" | "collapse" => Some(false),
            "inherit" | "unset" => None,
            _ => None,
        }
    };
    let parse_dasharray = |raw: &str| -> Option<Option<VectorDashArray>> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("initial") {
            return Some(None);
        }
        if raw.eq_ignore_ascii_case("inherit") || raw.eq_ignore_ascii_case("unset") {
            return None;
        }
        let mut values = [0.0_f32; 8];
        let mut len = 0usize;
        let mut has_positive_value = false;
        for component in raw
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|component| !component.is_empty())
        {
            let value = parse_stroke_length(component)?;
            if !value.is_finite() || value < 0.0 {
                return None;
            }
            has_positive_value |= value > 0.0;
            if len < values.len() {
                values[len] = value;
                len += 1;
            }
        }
        (len > 0 && has_positive_value).then_some(Some(VectorDashArray {
            values,
            len,
            offset_ratio: 0.0,
        }))
    };
    let parse_stroke_linecap = |raw: &str| -> Option<VectorStrokeLineCap> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "butt" | "initial" => Some(VectorStrokeLineCap::Butt),
            "round" => Some(VectorStrokeLineCap::Round),
            "square" => Some(VectorStrokeLineCap::Square),
            _ => None,
        }
    };
    let parse_stroke_linejoin = |raw: &str| -> Option<VectorStrokeLineJoin> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "miter" | "miter-clip" | "initial" => Some(VectorStrokeLineJoin::Miter),
            "round" => Some(VectorStrokeLineJoin::Round),
            "bevel" => Some(VectorStrokeLineJoin::Bevel),
            _ => None,
        }
    };
    let parse_stroke_miterlimit = |raw: &str| -> Option<f32> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("initial") {
            return Some(4.0);
        }
        if raw.eq_ignore_ascii_case("inherit") || raw.eq_ignore_ascii_case("unset") {
            return None;
        }
        parse_number_prefix(raw).filter(|limit| limit.is_finite() && *limit >= 1.0)
    };
    let parse_fill_rule = |raw: &str| -> Option<VectorFillRule> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "nonzero" | "initial" => Some(VectorFillRule::NonZero),
            "evenodd" => Some(VectorFillRule::EvenOdd),
            _ => None,
        }
    };
    let parse_text_anchor = |raw: &str| -> Option<VectorTextAnchor> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "start" | "initial" => Some(VectorTextAnchor::Start),
            "middle" => Some(VectorTextAnchor::Middle),
            "end" => Some(VectorTextAnchor::End),
            _ => None,
        }
    };
    let parse_text_baseline = |raw: &str| -> Option<VectorTextBaseline> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "alphabetic" | "baseline" | "initial" => Some(VectorTextBaseline::Alphabetic),
            "middle" | "central" => Some(VectorTextBaseline::Middle),
            _ => None,
        }
    };
    let parse_baseline_shift = |raw: &str| -> Option<VectorBaselineShift> {
        let raw = raw.trim();
        match raw.to_ascii_lowercase().as_str() {
            "baseline" | "initial" => Some(VectorBaselineShift::Offset(0.0)),
            "super" => Some(VectorBaselineShift::Super),
            "sub" => Some(VectorBaselineShift::Sub),
            _ => {
                if let Some(percent) = raw.strip_suffix('%') {
                    return percent
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(|value| VectorBaselineShift::Percent(value / 100.0));
                }
                parse_number_prefix(raw)
                    .filter(|offset| offset.is_finite())
                    .map(VectorBaselineShift::Offset)
            }
        }
    };
    let parse_font_series = |raw: &str| -> Option<FontSeries> {
        let raw = raw.trim().to_ascii_lowercase();
        match raw.as_str() {
            "normal" | "lighter" | "initial" => Some(FontSeries::Regular),
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
            "normal" | "initial" => Some(FontShape::Upright),
            "italic" | "oblique" => Some(FontShape::Italic),
            _ => None,
        }
    };
    let parse_font_size = |raw: &str| -> Option<VectorFontSize> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("initial") {
            return Some(VectorFontSize::Absolute(12.0));
        }
        if let Some(percent) = raw.strip_suffix('%') {
            return percent
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(|value| VectorFontSize::Percent(value / 100.0));
        }
        parse_number_prefix(raw)
            .filter(|font_size| *font_size > 0.0)
            .map(VectorFontSize::Absolute)
    };
    let parse_font_family = |raw: &str| -> Option<VectorFontFamily> {
        if raw.trim().eq_ignore_ascii_case("initial") {
            return Some(VectorFontFamily::Serif);
        }
        raw.split(',').find_map(|family| {
            let family = family
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"')
                .to_ascii_lowercase();
            match family.as_str() {
                "serif" | "times" | "times new roman" | "dejavu serif" | "liberation serif" => {
                    Some(VectorFontFamily::Serif)
                }
                "sans" | "sans-serif" | "helvetica" | "arial" | "dejavu sans"
                | "liberation sans" => Some(VectorFontFamily::Sans),
                "mono" | "monospace" | "courier" | "courier new" | "dejavu sans mono"
                | "liberation mono" => Some(VectorFontFamily::Mono),
                _ => None,
            }
        })
    };
    let parse_vector_effect = |raw: &str| -> Option<bool> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "non-scaling-stroke" => Some(true),
            "default" | "none" | "initial" | "unset" => Some(false),
            _ => None,
        }
    };
    let parse_paint_order = |raw: &str| -> Option<VectorPaintOrder> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("normal") || raw.eq_ignore_ascii_case("initial") {
            return Some(VectorPaintOrder::Normal);
        }
        if raw.is_empty() || raw.eq_ignore_ascii_case("inherit") {
            return None;
        }
        let mut fill_index = None;
        let mut stroke_index = None;
        for (index, token) in raw.split_whitespace().enumerate() {
            match token.to_ascii_lowercase().as_str() {
                "fill" => fill_index = fill_index.or(Some(index)),
                "stroke" => stroke_index = stroke_index.or(Some(index)),
                "markers" => {}
                _ => return None,
            }
        }
        let stroke_first = stroke_index
            .map(|stroke_index| {
                fill_index
                    .map(|fill_index| stroke_index < fill_index)
                    .unwrap_or(true)
            })
            .unwrap_or(false);
        Some(if stroke_first {
            VectorPaintOrder::StrokeFill
        } else {
            VectorPaintOrder::Normal
        })
    };
    let clip_path_id_hash = |id: &str| -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    };
    let parse_url_fragment_reference = |raw: &str| -> Option<Option<u64>> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("initial") {
            return Some(None);
        }
        if raw.len() >= 4 && raw[..4].eq_ignore_ascii_case("url(") {
            let url_end = raw.find(')')?;
            let reference = raw[4..url_end]
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"');
            return reference
                .strip_prefix('#')
                .filter(|id| !id.is_empty())
                .map(|id| Some(clip_path_id_hash(id)));
        }
        None
    };
    let parse_clip_path = |raw: &str| -> Option<Option<u64>> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("unset") {
            return Some(None);
        }
        parse_url_fragment_reference(raw)
    };
    let parse_marker_reference =
        |raw: &str| -> Option<Option<u64>> { parse_url_fragment_reference(raw) };
    let decode_data_image_uri = |raw: &str| -> Option<EmbeddedRasterImage> {
        let raw = raw.trim();
        if raw.len() < "data:".len()
            || !raw.as_bytes()[.."data:".len()].eq_ignore_ascii_case(b"data:")
        {
            return None;
        }
        let data = &raw["data:".len()..];
        let comma = data.find(',')?;
        let metadata = data[..comma].trim().to_ascii_lowercase();
        let media_type = metadata.split(';').next()?.trim();
        if !matches!(media_type, "image/png" | "image/jpeg" | "image/jpg") {
            return None;
        }
        let payload = &data[comma + 1..];
        let percent_decode_payload = |payload: &[u8]| -> Option<Vec<u8>> {
            let mut bytes = Vec::new();
            let mut index = 0usize;
            while index < payload.len() {
                if payload[index] == b'%' {
                    let high = payload.get(index + 1).copied()?;
                    let low = payload.get(index + 2).copied()?;
                    let hex = |byte: u8| -> Option<u8> {
                        match byte {
                            b'0'..=b'9' => Some(byte - b'0'),
                            b'a'..=b'f' => Some(byte - b'a' + 10),
                            b'A'..=b'F' => Some(byte - b'A' + 10),
                            _ => None,
                        }
                    };
                    bytes.push((hex(high)? << 4) | hex(low)?);
                    index += 3;
                } else {
                    bytes.push(payload[index]);
                    index += 1;
                }
            }
            Some(bytes)
        };
        let bytes = if metadata.split(';').any(|part| part.trim() == "base64") {
            let payload = percent_decode_payload(payload.as_bytes())?;
            let mut bytes = Vec::new();
            let mut buffer = 0u32;
            let mut bits = 0u8;
            let mut padded = false;
            for byte in payload
                .into_iter()
                .filter(|byte| !byte.is_ascii_whitespace())
            {
                if byte == b'=' {
                    padded = true;
                    continue;
                }
                if padded {
                    return None;
                }
                let value = match byte {
                    b'A'..=b'Z' => u32::from(byte - b'A'),
                    b'a'..=b'z' => u32::from(byte - b'a' + 26),
                    b'0'..=b'9' => u32::from(byte - b'0' + 52),
                    b'+' => 62,
                    b'/' => 63,
                    _ => return None,
                };
                buffer = (buffer << 6) | value;
                bits += 6;
                if bits >= 8 {
                    bits -= 8;
                    bytes.push((buffer >> bits) as u8);
                    if bits > 0 {
                        buffer &= (1 << bits) - 1;
                    } else {
                        buffer = 0;
                    }
                }
            }
            bytes
        } else {
            percent_decode_payload(payload.as_bytes())?
        };
        decode_raster_image(&bytes)
    };
    let mut decode_image_href = |raw: &str| -> Option<EmbeddedRasterImage> {
        decode_data_image_uri(raw).or_else(|| {
            let href = raw.trim();
            if href.is_empty() || href.starts_with('#') {
                return None;
            }
            resolve_embedded_asset(href).and_then(|bytes| decode_raster_image(&bytes))
        })
    };
    let first_some = |left: Option<String>, right: Option<String>| left.or(right);
    let initial_color = || VectorResolvedColor::opaque((0.0, 0.0, 0.0));
    let parse_optional_fill_paint = |value: Option<String>| -> Option<Option<VectorColor>> {
        let value = value?;
        let value = value.trim();
        if value.eq_ignore_ascii_case("inherit") || value.eq_ignore_ascii_case("unset") {
            return None;
        }
        if value.eq_ignore_ascii_case("initial") {
            return Some(Some(VectorColor::Resolved(initial_color())));
        }
        Some(parse_paint(value))
    };
    let parse_optional_stroke_paint = |value: Option<String>| -> Option<Option<VectorColor>> {
        let value = value?;
        let value = value.trim();
        if value.eq_ignore_ascii_case("inherit") || value.eq_ignore_ascii_case("unset") {
            return None;
        }
        if value.eq_ignore_ascii_case("initial") {
            return Some(None);
        }
        Some(parse_paint(value))
    };
    let parse_optional_paint = |value: Option<String>| -> Option<Option<VectorColor>> {
        let value = value?;
        let value = value.trim();
        if value.eq_ignore_ascii_case("inherit") {
            return None;
        }
        if value.eq_ignore_ascii_case("initial") || value.eq_ignore_ascii_case("unset") {
            return Some(Some(VectorColor::CurrentColor));
        }
        Some(parse_paint(value))
    };
    let parse_optional_color = |value: Option<String>| -> Option<VectorResolvedColor> {
        let value = value?;
        let value = value.trim();
        if value.eq_ignore_ascii_case("inherit") || value.eq_ignore_ascii_case("unset") {
            return None;
        }
        if value.eq_ignore_ascii_case("initial") {
            return Some(initial_color());
        }
        parse_color(value)
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
                                    paint_order: Option<String>,
                                    color: Option<String>,
                                    display: Option<String>,
                                    visibility: Option<String>,
                                    opacity: Option<String>,
                                    fill_opacity: Option<String>,
                                    stroke_opacity: Option<String>,
                                    text_anchor: Option<String>,
                                    font_size: Option<String>,
                                    font_family: Option<String>,
                                    font_weight: Option<String>,
                                    font_style: Option<String>,
                                    letter_spacing: Option<String>,
                                    word_spacing: Option<String>,
                                    text_decoration: Option<String>,
                                    text_decoration_color: Option<String>,
                                    text_decoration_thickness: Option<String>,
                                    text_decoration_style: Option<String>,
                                    text_baseline: Option<String>,
                                    baseline_shift: Option<String>,
                                    vector_effect: Option<String>,
                                    marker: Option<String>,
                                    marker_start: Option<String>,
                                    marker_mid: Option<String>,
                                    marker_end: Option<String>,
                                    clip_path: Option<String>|
     -> VectorPresentation {
        let marker = marker.as_deref().and_then(parse_marker_reference);
        let text_decoration_color = parse_optional_paint(text_decoration_color).or_else(|| {
            let text_decoration = text_decoration.as_deref()?;
            for token in text_decoration.split_whitespace() {
                let token = token.trim();
                let lower = token.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "none"
                        | "underline"
                        | "overline"
                        | "line-through"
                        | "blink"
                        | "solid"
                        | "double"
                        | "dotted"
                        | "dashed"
                        | "wavy"
                ) {
                    continue;
                }
                if lower == "transparent" {
                    return Some(None);
                }
                if lower == "initial" || lower == "unset" {
                    return Some(Some(VectorColor::CurrentColor));
                }
                if let Some(color) = parse_paint(token) {
                    return Some(Some(color));
                }
            }
            None
        });
        let text_decoration_thickness = text_decoration_thickness
            .as_deref()
            .and_then(parse_text_decoration_thickness)
            .or_else(|| {
                let text_decoration = text_decoration.as_deref()?;
                for token in text_decoration.split_whitespace() {
                    if let Some(thickness) = parse_text_decoration_thickness(token) {
                        return Some(thickness);
                    }
                }
                None
            });
        let text_decoration_style = text_decoration_style
            .as_deref()
            .and_then(parse_text_decoration_style)
            .or_else(|| {
                let text_decoration = text_decoration.as_deref()?;
                for token in text_decoration.split_whitespace() {
                    if let Some(style) = parse_text_decoration_style(token) {
                        return Some(style);
                    }
                }
                None
            });
        VectorPresentation {
            fill: parse_optional_fill_paint(fill),
            fill_rule: fill_rule.as_deref().and_then(parse_fill_rule),
            stroke: parse_optional_stroke_paint(stroke),
            stroke_width: stroke_width.as_deref().and_then(parse_stroke_width),
            stroke_dasharray: stroke_dasharray.as_deref().and_then(parse_dasharray),
            stroke_dashoffset: stroke_dashoffset.as_deref().and_then(parse_stroke_length),
            stroke_linecap: stroke_linecap.as_deref().and_then(parse_stroke_linecap),
            stroke_linejoin: stroke_linejoin.as_deref().and_then(parse_stroke_linejoin),
            stroke_miterlimit: stroke_miterlimit
                .as_deref()
                .and_then(parse_stroke_miterlimit),
            paint_order: paint_order.as_deref().and_then(parse_paint_order),
            color: parse_optional_color(color),
            display: display.as_deref().and_then(parse_display),
            visibility: visibility.as_deref().and_then(parse_visibility),
            opacity: opacity.as_deref().and_then(parse_opacity),
            fill_opacity: fill_opacity.as_deref().and_then(parse_opacity),
            stroke_opacity: stroke_opacity.as_deref().and_then(parse_opacity),
            text_anchor: text_anchor.as_deref().and_then(parse_text_anchor),
            font_size: font_size.as_deref().and_then(parse_font_size),
            font_family: font_family.as_deref().and_then(parse_font_family),
            font_series: font_weight.as_deref().and_then(parse_font_series),
            font_shape: font_style.as_deref().and_then(parse_font_shape),
            letter_spacing: letter_spacing.as_deref().and_then(parse_letter_spacing),
            word_spacing: word_spacing.as_deref().and_then(parse_word_spacing),
            text_decoration: text_decoration.as_deref().and_then(parse_text_decoration),
            text_decoration_color,
            text_decoration_thickness,
            text_decoration_style,
            text_baseline: text_baseline.as_deref().and_then(parse_text_baseline),
            baseline_shift: baseline_shift.as_deref().and_then(parse_baseline_shift),
            vector_effect_non_scaling_stroke: vector_effect
                .as_deref()
                .and_then(parse_vector_effect),
            marker_start: marker_start
                .as_deref()
                .and_then(parse_marker_reference)
                .or(marker),
            marker_mid: marker_mid
                .as_deref()
                .and_then(parse_marker_reference)
                .or(marker),
            marker_end: marker_end
                .as_deref()
                .and_then(parse_marker_reference)
                .or(marker),
            clip_path: clip_path.as_deref().and_then(parse_clip_path),
        }
    };
    let parse_attr_presentation = |tag: &str| -> VectorPresentation {
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
            attr_value(tag, "paint-order"),
            attr_value(tag, "color"),
            attr_value(tag, "display"),
            attr_value(tag, "visibility"),
            attr_value(tag, "opacity"),
            attr_value(tag, "fill-opacity"),
            attr_value(tag, "stroke-opacity"),
            attr_value(tag, "text-anchor"),
            attr_value(tag, "font-size"),
            attr_value(tag, "font-family"),
            attr_value(tag, "font-weight"),
            attr_value(tag, "font-style"),
            attr_value(tag, "letter-spacing"),
            attr_value(tag, "word-spacing"),
            first_some(
                attr_value(tag, "text-decoration-line"),
                attr_value(tag, "text-decoration"),
            ),
            attr_value(tag, "text-decoration-color"),
            attr_value(tag, "text-decoration-thickness"),
            attr_value(tag, "text-decoration-style"),
            first_some(
                attr_value(tag, "dominant-baseline"),
                attr_value(tag, "alignment-baseline"),
            ),
            attr_value(tag, "baseline-shift"),
            attr_value(tag, "vector-effect"),
            attr_value(tag, "marker"),
            attr_value(tag, "marker-start"),
            attr_value(tag, "marker-mid"),
            attr_value(tag, "marker-end"),
            attr_value(tag, "clip-path"),
        )
    };
    let parse_inline_style_presentation = |tag: &str| -> VectorPresentation {
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
            style_value(tag, "paint-order"),
            style_value(tag, "color"),
            style_value(tag, "display"),
            style_value(tag, "visibility"),
            style_value(tag, "opacity"),
            style_value(tag, "fill-opacity"),
            style_value(tag, "stroke-opacity"),
            style_value(tag, "text-anchor"),
            style_value(tag, "font-size"),
            style_value(tag, "font-family"),
            style_value(tag, "font-weight"),
            style_value(tag, "font-style"),
            style_value(tag, "letter-spacing"),
            style_value(tag, "word-spacing"),
            first_some(
                style_value(tag, "text-decoration-line"),
                style_value(tag, "text-decoration"),
            ),
            style_value(tag, "text-decoration-color"),
            style_value(tag, "text-decoration-thickness"),
            style_value(tag, "text-decoration-style"),
            first_some(
                style_value(tag, "dominant-baseline"),
                style_value(tag, "alignment-baseline"),
            ),
            style_value(tag, "baseline-shift"),
            style_value(tag, "vector-effect"),
            style_value(tag, "marker"),
            style_value(tag, "marker-start"),
            style_value(tag, "marker-mid"),
            style_value(tag, "marker-end"),
            style_value(tag, "clip-path"),
        )
    };
    let parse_declaration_presentation = |declarations: &str| -> VectorPresentation {
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
            declaration_value(declarations, "paint-order"),
            declaration_value(declarations, "color"),
            declaration_value(declarations, "display"),
            declaration_value(declarations, "visibility"),
            declaration_value(declarations, "opacity"),
            declaration_value(declarations, "fill-opacity"),
            declaration_value(declarations, "stroke-opacity"),
            declaration_value(declarations, "text-anchor"),
            declaration_value(declarations, "font-size"),
            declaration_value(declarations, "font-family"),
            declaration_value(declarations, "font-weight"),
            declaration_value(declarations, "font-style"),
            declaration_value(declarations, "letter-spacing"),
            declaration_value(declarations, "word-spacing"),
            first_some(
                declaration_value(declarations, "text-decoration-line"),
                declaration_value(declarations, "text-decoration"),
            ),
            declaration_value(declarations, "text-decoration-color"),
            declaration_value(declarations, "text-decoration-thickness"),
            declaration_value(declarations, "text-decoration-style"),
            first_some(
                declaration_value(declarations, "dominant-baseline"),
                declaration_value(declarations, "alignment-baseline"),
            ),
            declaration_value(declarations, "baseline-shift"),
            declaration_value(declarations, "vector-effect"),
            declaration_value(declarations, "marker"),
            declaration_value(declarations, "marker-start"),
            declaration_value(declarations, "marker-mid"),
            declaration_value(declarations, "marker-end"),
            declaration_value(declarations, "clip-path"),
        )
    };
    let declaration_inherit_or_unset = |declarations: &str, name: &str| {
        declaration_value(declarations, name)
            .map(|value| {
                let value = value.trim().to_ascii_lowercase();
                value == "inherit" || value == "unset"
            })
            .unwrap_or(false)
    };
    #[derive(Debug, Clone)]
    enum VectorStyleSelector {
        Universal,
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
    struct VectorStyleRule {
        selector: VectorStyleSelector,
        specificity: u16,
        presentation: VectorPresentation,
        fill_inherit_or_unset: bool,
        fill_rule_inherit_or_unset: bool,
        stroke_inherit_or_unset: bool,
        stroke_width_inherit_or_unset: bool,
        stroke_dasharray_inherit_or_unset: bool,
        stroke_dashoffset_inherit_or_unset: bool,
        stroke_linecap_inherit_or_unset: bool,
        stroke_linejoin_inherit_or_unset: bool,
        stroke_miterlimit_inherit_or_unset: bool,
        paint_order_inherit_or_unset: bool,
        color_inherit_or_unset: bool,
        display_inherit: bool,
        opacity_inherit: bool,
        fill_opacity_inherit_or_unset: bool,
        stroke_opacity_inherit_or_unset: bool,
        visibility_inherit_or_unset: bool,
        marker_start_inherit_or_unset: bool,
        marker_mid_inherit_or_unset: bool,
        marker_end_inherit_or_unset: bool,
        text_anchor_inherit_or_unset: bool,
        letter_spacing_inherit_or_unset: bool,
        word_spacing_inherit_or_unset: bool,
        text_decoration_inherit: bool,
        text_decoration_color_inherit: bool,
        text_decoration_thickness_inherit: bool,
        text_decoration_style_inherit: bool,
        font_size_inherit_or_unset: bool,
        font_family_inherit_or_unset: bool,
        font_series_inherit_or_unset: bool,
        font_shape_inherit_or_unset: bool,
        text_baseline_inherit_or_unset: bool,
        baseline_shift_inherit_or_unset: bool,
        vector_effect_inherit: bool,
        clip_path_inherit: bool,
    }
    #[derive(Debug, Clone, Copy)]
    enum VectorCascadeAction<T> {
        Value(T),
        Clear,
    }
    #[derive(Debug, Clone, Copy)]
    struct VectorCascadeEntry<T> {
        action: VectorCascadeAction<T>,
        specificity: u16,
        order: usize,
    }
    #[derive(Debug, Clone, Copy, Default)]
    struct VectorStyleCascade {
        presentation: VectorPresentation,
        fill_clear: bool,
        fill_rule_clear: bool,
        stroke_clear: bool,
        stroke_width_clear: bool,
        stroke_dasharray_clear: bool,
        stroke_dashoffset_clear: bool,
        stroke_linecap_clear: bool,
        stroke_linejoin_clear: bool,
        stroke_miterlimit_clear: bool,
        paint_order_clear: bool,
        color_clear: bool,
        display_clear: bool,
        visibility_clear: bool,
        opacity_clear: bool,
        fill_opacity_clear: bool,
        stroke_opacity_clear: bool,
        text_anchor_clear: bool,
        letter_spacing_clear: bool,
        word_spacing_clear: bool,
        text_decoration_clear: bool,
        text_decoration_color_clear: bool,
        text_decoration_thickness_clear: bool,
        text_decoration_style_clear: bool,
        font_size_clear: bool,
        font_family_clear: bool,
        font_series_clear: bool,
        font_shape_clear: bool,
        text_baseline_clear: bool,
        baseline_shift_clear: bool,
        vector_effect_non_scaling_stroke_clear: bool,
        marker_start_clear: bool,
        marker_mid_clear: bool,
        marker_end_clear: bool,
        clip_path_clear: bool,
    }
    let valid_svg_element_name = |element_name: &str| {
        !element_name.is_empty()
            && element_name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    };
    let parse_style_selector = |selector: &str| -> Option<VectorStyleSelector> {
        let selector = selector.trim();
        if selector.contains('+') || selector.contains('~') {
            return None;
        }
        let selector = selector
            .split(|ch: char| ch.is_whitespace() || ch == '>')
            .filter(|part| !part.is_empty())
            .last()
            .unwrap_or(selector)
            .trim();
        if selector.chars().any(char::is_whitespace) {
            return None;
        }
        if selector == "*" {
            Some(VectorStyleSelector::Universal)
        } else if selector.contains('.') {
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
            (!class_name.is_empty()).then_some(VectorStyleSelector::Class {
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
            (!id.is_empty()).then_some(VectorStyleSelector::Id { element_name, id })
        } else if valid_svg_element_name(selector) {
            Some(VectorStyleSelector::Type {
                element_name: selector.to_ascii_lowercase(),
            })
        } else {
            None
        }
    };
    let selector_specificity = |selector: &VectorStyleSelector| -> u16 {
        match selector {
            VectorStyleSelector::Universal => 0,
            VectorStyleSelector::Type { .. } => 1,
            VectorStyleSelector::Class { element_name, .. } => {
                10 + u16::from(element_name.is_some())
            }
            VectorStyleSelector::Id { element_name, .. } => 100 + u16::from(element_name.is_some()),
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
        let Some(style_tag_end) = find_simple_xml_tag_end(style_tag_tail) else {
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
            let declarations = &css[body_start..body_end];
            let presentation = parse_declaration_presentation(declarations);
            let fill_inherit_or_unset = declaration_inherit_or_unset(declarations, "fill");
            let fill_rule_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "fill-rule");
            let stroke_inherit_or_unset = declaration_inherit_or_unset(declarations, "stroke");
            let stroke_width_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-width");
            let stroke_dasharray_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-dasharray");
            let stroke_dashoffset_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-dashoffset");
            let stroke_linecap_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-linecap");
            let stroke_linejoin_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-linejoin");
            let stroke_miterlimit_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-miterlimit");
            let paint_order_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "paint-order");
            let color_inherit_or_unset = declaration_inherit_or_unset(declarations, "color");
            let display_inherit = declaration_value(declarations, "display")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false);
            let opacity_inherit = declaration_value(declarations, "opacity")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false);
            let fill_opacity_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "fill-opacity");
            let stroke_opacity_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "stroke-opacity");
            let visibility_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "visibility");
            let marker_inherit_or_unset = declaration_inherit_or_unset(declarations, "marker");
            let marker_start_inherit_or_unset = marker_inherit_or_unset
                || declaration_inherit_or_unset(declarations, "marker-start");
            let marker_mid_inherit_or_unset =
                marker_inherit_or_unset || declaration_inherit_or_unset(declarations, "marker-mid");
            let marker_end_inherit_or_unset =
                marker_inherit_or_unset || declaration_inherit_or_unset(declarations, "marker-end");
            let text_anchor_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "text-anchor");
            let letter_spacing_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "letter-spacing");
            let word_spacing_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "word-spacing");
            let text_decoration_inherit = first_some(
                declaration_value(declarations, "text-decoration-line"),
                declaration_value(declarations, "text-decoration"),
            )
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false);
            let text_decoration_color_inherit = declaration_value(declarations, "text-decoration")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false)
                || declaration_value(declarations, "text-decoration-color")
                    .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                    .unwrap_or(false);
            let text_decoration_thickness_inherit =
                declaration_value(declarations, "text-decoration")
                    .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                    .unwrap_or(false)
                    || declaration_value(declarations, "text-decoration-thickness")
                        .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                        .unwrap_or(false);
            let text_decoration_style_inherit = declaration_value(declarations, "text-decoration")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false)
                || declaration_value(declarations, "text-decoration-style")
                    .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                    .unwrap_or(false);
            let font_size_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "font-size");
            let font_family_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "font-family");
            let font_series_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "font-weight");
            let font_shape_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "font-style");
            let text_baseline_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "dominant-baseline")
                    || declaration_inherit_or_unset(declarations, "alignment-baseline");
            let baseline_shift_inherit_or_unset =
                declaration_inherit_or_unset(declarations, "baseline-shift");
            let vector_effect_inherit = declaration_value(declarations, "vector-effect")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false);
            let clip_path_inherit = declaration_value(declarations, "clip-path")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false);
            for selector in css[css_offset..selector_end].split(',') {
                let Some(selector) = parse_style_selector(selector) else {
                    continue;
                };
                let specificity = selector_specificity(&selector);
                style_rules.push(VectorStyleRule {
                    selector,
                    specificity,
                    presentation,
                    fill_inherit_or_unset,
                    fill_rule_inherit_or_unset,
                    stroke_inherit_or_unset,
                    stroke_width_inherit_or_unset,
                    stroke_dasharray_inherit_or_unset,
                    stroke_dashoffset_inherit_or_unset,
                    stroke_linecap_inherit_or_unset,
                    stroke_linejoin_inherit_or_unset,
                    stroke_miterlimit_inherit_or_unset,
                    paint_order_inherit_or_unset,
                    color_inherit_or_unset,
                    display_inherit,
                    opacity_inherit,
                    fill_opacity_inherit_or_unset,
                    stroke_opacity_inherit_or_unset,
                    visibility_inherit_or_unset,
                    marker_start_inherit_or_unset,
                    marker_mid_inherit_or_unset,
                    marker_end_inherit_or_unset,
                    text_anchor_inherit_or_unset,
                    letter_spacing_inherit_or_unset,
                    word_spacing_inherit_or_unset,
                    text_decoration_inherit,
                    text_decoration_color_inherit,
                    text_decoration_thickness_inherit,
                    text_decoration_style_inherit,
                    font_size_inherit_or_unset,
                    font_family_inherit_or_unset,
                    font_series_inherit_or_unset,
                    font_shape_inherit_or_unset,
                    text_baseline_inherit_or_unset,
                    baseline_shift_inherit_or_unset,
                    vector_effect_inherit,
                    clip_path_inherit,
                });
            }
            css_offset = body_end + 1;
        }
        style_block_offset = content_start + content_end_relative + "</style>".len();
    }
    let overlay_presentation =
        |base: VectorPresentation, local: VectorPresentation| -> VectorPresentation {
            VectorPresentation {
                fill: local.fill.or(base.fill),
                fill_rule: local.fill_rule.or(base.fill_rule),
                stroke: local.stroke.or(base.stroke),
                stroke_width: local.stroke_width.or(base.stroke_width),
                stroke_dasharray: local.stroke_dasharray.or(base.stroke_dasharray),
                stroke_dashoffset: local.stroke_dashoffset.or(base.stroke_dashoffset),
                stroke_linecap: local.stroke_linecap.or(base.stroke_linecap),
                stroke_linejoin: local.stroke_linejoin.or(base.stroke_linejoin),
                stroke_miterlimit: local.stroke_miterlimit.or(base.stroke_miterlimit),
                paint_order: local.paint_order.or(base.paint_order),
                color: local.color.or(base.color),
                display: local.display.or(base.display),
                visibility: local.visibility.or(base.visibility),
                opacity: local.opacity.or(base.opacity),
                fill_opacity: local.fill_opacity.or(base.fill_opacity),
                stroke_opacity: local.stroke_opacity.or(base.stroke_opacity),
                text_anchor: local.text_anchor.or(base.text_anchor),
                font_size: local.font_size.or(base.font_size),
                font_family: local.font_family.or(base.font_family),
                font_series: local.font_series.or(base.font_series),
                font_shape: local.font_shape.or(base.font_shape),
                letter_spacing: local.letter_spacing.or(base.letter_spacing),
                word_spacing: local.word_spacing.or(base.word_spacing),
                text_decoration: local.text_decoration.or(base.text_decoration),
                text_decoration_color: local.text_decoration_color.or(base.text_decoration_color),
                text_decoration_thickness: local
                    .text_decoration_thickness
                    .or(base.text_decoration_thickness),
                text_decoration_style: local.text_decoration_style.or(base.text_decoration_style),
                text_baseline: local.text_baseline.or(base.text_baseline),
                baseline_shift: local.baseline_shift.or(base.baseline_shift),
                vector_effect_non_scaling_stroke: local
                    .vector_effect_non_scaling_stroke
                    .or(base.vector_effect_non_scaling_stroke),
                marker_start: local.marker_start.or(base.marker_start),
                marker_mid: local.marker_mid.or(base.marker_mid),
                marker_end: local.marker_end.or(base.marker_end),
                clip_path: local.clip_path.or(base.clip_path),
            }
        };
    let inherit_presentation = |parent: VectorPresentation,
                                local: VectorPresentation|
     -> VectorPresentation {
        let opacity = match (parent.opacity, local.opacity) {
            (Some(parent), Some(local)) => Some((parent * local).clamp(0.0, 1.0)),
            (Some(parent), None) => Some(parent),
            (None, Some(local)) => Some(local),
            (None, None) => None,
        };
        let display = match (parent.display, local.display) {
            (Some(false), _) => Some(false),
            (_, Some(local)) => Some(local),
            (Some(parent), None) => Some(parent),
            (None, None) => None,
        };
        let font_size = match (parent.font_size, local.font_size) {
            (_, Some(VectorFontSize::Absolute(size))) => Some(VectorFontSize::Absolute(size)),
            (Some(VectorFontSize::Absolute(parent_size)), Some(VectorFontSize::Percent(scale))) => {
                Some(VectorFontSize::Absolute(parent_size * scale))
            }
            (Some(VectorFontSize::Percent(parent_scale)), Some(VectorFontSize::Percent(scale))) => {
                Some(VectorFontSize::Percent(parent_scale * scale))
            }
            (None, Some(VectorFontSize::Percent(scale))) => Some(VectorFontSize::Percent(scale)),
            (Some(parent_size), None) => Some(parent_size),
            (None, None) => None,
        };
        VectorPresentation {
            fill: local.fill.or(parent.fill),
            fill_rule: local.fill_rule.or(parent.fill_rule),
            stroke: local.stroke.or(parent.stroke),
            stroke_width: local.stroke_width.or(parent.stroke_width),
            stroke_dasharray: local.stroke_dasharray.or(parent.stroke_dasharray),
            stroke_dashoffset: local.stroke_dashoffset.or(parent.stroke_dashoffset),
            stroke_linecap: local.stroke_linecap.or(parent.stroke_linecap),
            stroke_linejoin: local.stroke_linejoin.or(parent.stroke_linejoin),
            stroke_miterlimit: local.stroke_miterlimit.or(parent.stroke_miterlimit),
            paint_order: local.paint_order.or(parent.paint_order),
            color: local.color.or(parent.color),
            display,
            visibility: local.visibility.or(parent.visibility),
            opacity,
            fill_opacity: local.fill_opacity.or(parent.fill_opacity),
            stroke_opacity: local.stroke_opacity.or(parent.stroke_opacity),
            text_anchor: local.text_anchor.or(parent.text_anchor),
            font_size,
            font_family: local.font_family.or(parent.font_family),
            font_series: local.font_series.or(parent.font_series),
            font_shape: local.font_shape.or(parent.font_shape),
            letter_spacing: local.letter_spacing.or(parent.letter_spacing),
            word_spacing: local.word_spacing.or(parent.word_spacing),
            text_decoration: local.text_decoration.or(parent.text_decoration),
            text_decoration_color: local.text_decoration_color.or(parent.text_decoration_color),
            text_decoration_thickness: local
                .text_decoration_thickness
                .or(parent.text_decoration_thickness),
            text_decoration_style: local.text_decoration_style.or(parent.text_decoration_style),
            text_baseline: local.text_baseline.or(parent.text_baseline),
            baseline_shift: local.baseline_shift.or(parent.baseline_shift),
            vector_effect_non_scaling_stroke: local.vector_effect_non_scaling_stroke,
            marker_start: local.marker_start.or(parent.marker_start),
            marker_mid: local.marker_mid.or(parent.marker_mid),
            marker_end: local.marker_end.or(parent.marker_end),
            clip_path: local.clip_path.or(parent.clip_path),
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
    let style_rule_presentation_for = |tag: &str| -> VectorStyleCascade {
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
        let mut fill: Option<VectorCascadeEntry<Option<VectorColor>>> = None;
        let mut fill_rule: Option<VectorCascadeEntry<VectorFillRule>> = None;
        let mut stroke: Option<VectorCascadeEntry<Option<VectorColor>>> = None;
        let mut stroke_width: Option<VectorCascadeEntry<f32>> = None;
        let mut stroke_dasharray: Option<VectorCascadeEntry<Option<VectorDashArray>>> = None;
        let mut stroke_dashoffset: Option<VectorCascadeEntry<f32>> = None;
        let mut stroke_linecap: Option<VectorCascadeEntry<VectorStrokeLineCap>> = None;
        let mut stroke_linejoin: Option<VectorCascadeEntry<VectorStrokeLineJoin>> = None;
        let mut stroke_miterlimit: Option<VectorCascadeEntry<f32>> = None;
        let mut paint_order: Option<VectorCascadeEntry<VectorPaintOrder>> = None;
        let mut color: Option<VectorCascadeEntry<VectorResolvedColor>> = None;
        let mut display: Option<VectorCascadeEntry<bool>> = None;
        let mut visibility: Option<VectorCascadeEntry<bool>> = None;
        let mut opacity: Option<VectorCascadeEntry<f32>> = None;
        let mut fill_opacity: Option<VectorCascadeEntry<f32>> = None;
        let mut stroke_opacity: Option<VectorCascadeEntry<f32>> = None;
        let mut text_anchor: Option<VectorCascadeEntry<VectorTextAnchor>> = None;
        let mut font_size: Option<VectorCascadeEntry<VectorFontSize>> = None;
        let mut font_family: Option<VectorCascadeEntry<VectorFontFamily>> = None;
        let mut font_series: Option<VectorCascadeEntry<FontSeries>> = None;
        let mut font_shape: Option<VectorCascadeEntry<FontShape>> = None;
        let mut letter_spacing: Option<VectorCascadeEntry<f32>> = None;
        let mut word_spacing: Option<VectorCascadeEntry<f32>> = None;
        let mut text_decoration: Option<VectorCascadeEntry<VectorTextDecoration>> = None;
        let mut text_decoration_color: Option<VectorCascadeEntry<Option<VectorColor>>> = None;
        let mut text_decoration_thickness: Option<VectorCascadeEntry<f32>> = None;
        let mut text_decoration_style: Option<VectorCascadeEntry<VectorTextDecorationStyle>> = None;
        let mut text_baseline: Option<VectorCascadeEntry<VectorTextBaseline>> = None;
        let mut baseline_shift: Option<VectorCascadeEntry<VectorBaselineShift>> = None;
        let mut vector_effect_non_scaling_stroke: Option<VectorCascadeEntry<bool>> = None;
        let mut marker_start: Option<VectorCascadeEntry<Option<u64>>> = None;
        let mut marker_mid: Option<VectorCascadeEntry<Option<u64>>> = None;
        let mut marker_end: Option<VectorCascadeEntry<Option<u64>>> = None;
        let mut clip_path: Option<VectorCascadeEntry<Option<u64>>> = None;
        for (order, rule) in style_rules.iter().enumerate() {
            let matches = match &rule.selector {
                VectorStyleSelector::Universal => true,
                VectorStyleSelector::Type { element_name } => {
                    tag_element_name.as_deref() == Some(element_name.as_str())
                }
                VectorStyleSelector::Class {
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
                VectorStyleSelector::Id { element_name, id } => {
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
                if rule.fill_inherit_or_unset {
                    let current = fill.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.fill {
                    let current = fill.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.fill_rule_inherit_or_unset {
                    let current = fill_rule.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill_rule = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.fill_rule {
                    let current = fill_rule.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill_rule = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_inherit_or_unset {
                    let current = stroke.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke {
                    let current = stroke.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_width_inherit_or_unset {
                    let current = stroke_width.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_width = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_width {
                    let current = stroke_width.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_width = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_dasharray_inherit_or_unset {
                    let current = stroke_dasharray.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_dasharray = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_dasharray {
                    let current = stroke_dasharray.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_dasharray = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_dashoffset_inherit_or_unset {
                    let current = stroke_dashoffset.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_dashoffset = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_dashoffset {
                    let current = stroke_dashoffset.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_dashoffset = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_linecap_inherit_or_unset {
                    let current = stroke_linecap.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_linecap = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_linecap {
                    let current = stroke_linecap.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_linecap = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_linejoin_inherit_or_unset {
                    let current = stroke_linejoin.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_linejoin = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_linejoin {
                    let current = stroke_linejoin.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_linejoin = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_miterlimit_inherit_or_unset {
                    let current = stroke_miterlimit.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_miterlimit = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.stroke_miterlimit {
                    let current = stroke_miterlimit.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_miterlimit = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.paint_order_inherit_or_unset {
                    let current = paint_order.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        paint_order = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .paint_order
                    .filter(|_| !rule.paint_order_inherit_or_unset)
                {
                    let current = paint_order.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        paint_order = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.color_inherit_or_unset {
                    let current = color.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        color = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.color {
                    let current = color.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        color = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.display_inherit {
                    let current = display.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        display = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.display.filter(|_| !rule.display_inherit) {
                    let current = display.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        display = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.visibility_inherit_or_unset {
                    let current = visibility.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        visibility = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .visibility
                    .filter(|_| !rule.visibility_inherit_or_unset)
                {
                    let current = visibility.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        visibility = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.opacity_inherit {
                    let current = opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        opacity = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule.presentation.opacity.filter(|_| !rule.opacity_inherit) {
                    let current = opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        opacity = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.fill_opacity_inherit_or_unset {
                    let current = fill_opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill_opacity = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .fill_opacity
                    .filter(|_| !rule.fill_opacity_inherit_or_unset)
                {
                    let current = fill_opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        fill_opacity = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.stroke_opacity_inherit_or_unset {
                    let current = stroke_opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_opacity = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .stroke_opacity
                    .filter(|_| !rule.stroke_opacity_inherit_or_unset)
                {
                    let current = stroke_opacity.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        stroke_opacity = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.text_anchor_inherit_or_unset {
                    let current = text_anchor.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_anchor = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .text_anchor
                    .filter(|_| !rule.text_anchor_inherit_or_unset)
                {
                    let current = text_anchor.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_anchor = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.font_size_inherit_or_unset {
                    let current = font_size.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_size = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .font_size
                    .filter(|_| !rule.font_size_inherit_or_unset)
                {
                    let current = font_size.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_size = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.font_family_inherit_or_unset {
                    let current = font_family.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_family = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .font_family
                    .filter(|_| !rule.font_family_inherit_or_unset)
                {
                    let current = font_family.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_family = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.font_series_inherit_or_unset {
                    let current = font_series.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_series = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .font_series
                    .filter(|_| !rule.font_series_inherit_or_unset)
                {
                    let current = font_series.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_series = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.font_shape_inherit_or_unset {
                    let current = font_shape.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_shape = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .font_shape
                    .filter(|_| !rule.font_shape_inherit_or_unset)
                {
                    let current = font_shape.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        font_shape = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.letter_spacing_inherit_or_unset {
                    let current = letter_spacing.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        letter_spacing = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .letter_spacing
                    .filter(|_| !rule.letter_spacing_inherit_or_unset)
                {
                    let current = letter_spacing.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        letter_spacing = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.word_spacing_inherit_or_unset {
                    let current = word_spacing.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        word_spacing = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .word_spacing
                    .filter(|_| !rule.word_spacing_inherit_or_unset)
                {
                    let current = word_spacing.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        word_spacing = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.text_decoration_inherit {
                    let current = text_decoration.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .text_decoration
                    .filter(|_| !rule.text_decoration_inherit)
                {
                    let current = text_decoration.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.text_decoration_color_inherit {
                    let current =
                        text_decoration_color.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration_color = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .text_decoration_color
                    .filter(|_| !rule.text_decoration_color_inherit)
                {
                    let current =
                        text_decoration_color.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration_color = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.text_decoration_thickness_inherit {
                    let current =
                        text_decoration_thickness.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration_thickness = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .text_decoration_thickness
                    .filter(|_| !rule.text_decoration_thickness_inherit)
                {
                    let current =
                        text_decoration_thickness.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration_thickness = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.text_decoration_style_inherit {
                    let current =
                        text_decoration_style.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration_style = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .text_decoration_style
                    .filter(|_| !rule.text_decoration_style_inherit)
                {
                    let current =
                        text_decoration_style.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_decoration_style = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.text_baseline_inherit_or_unset {
                    let current = text_baseline.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_baseline = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .text_baseline
                    .filter(|_| !rule.text_baseline_inherit_or_unset)
                {
                    let current = text_baseline.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        text_baseline = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.baseline_shift_inherit_or_unset {
                    let current = baseline_shift.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        baseline_shift = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .baseline_shift
                    .filter(|_| !rule.baseline_shift_inherit_or_unset)
                {
                    let current = baseline_shift.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        baseline_shift = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.vector_effect_inherit {
                    let current = vector_effect_non_scaling_stroke
                        .map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        vector_effect_non_scaling_stroke = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .vector_effect_non_scaling_stroke
                    .filter(|_| !rule.vector_effect_inherit)
                {
                    let current = vector_effect_non_scaling_stroke
                        .map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        vector_effect_non_scaling_stroke = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.marker_start_inherit_or_unset {
                    let current = marker_start.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        marker_start = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .marker_start
                    .filter(|_| !rule.marker_start_inherit_or_unset)
                {
                    let current = marker_start.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        marker_start = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.marker_mid_inherit_or_unset {
                    let current = marker_mid.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        marker_mid = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .marker_mid
                    .filter(|_| !rule.marker_mid_inherit_or_unset)
                {
                    let current = marker_mid.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        marker_mid = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.marker_end_inherit_or_unset {
                    let current = marker_end.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        marker_end = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .marker_end
                    .filter(|_| !rule.marker_end_inherit_or_unset)
                {
                    let current = marker_end.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        marker_end = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if rule.clip_path_inherit {
                    let current = clip_path.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        clip_path = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Clear,
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
                if let Some(value) = rule
                    .presentation
                    .clip_path
                    .filter(|_| !rule.clip_path_inherit)
                {
                    let current = clip_path.map(|value| (value.specificity, value.order));
                    if should_replace_cascade_value(current, rule.specificity, order) {
                        clip_path = Some(VectorCascadeEntry {
                            action: VectorCascadeAction::Value(value),
                            specificity: rule.specificity,
                            order,
                        });
                    }
                }
            }
        }
        VectorStyleCascade {
            presentation: VectorPresentation {
                fill: match fill {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                fill_rule: match fill_rule {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke: match stroke {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_width: match stroke_width {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_dasharray: match stroke_dasharray {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_dashoffset: match stroke_dashoffset {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_linecap: match stroke_linecap {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_linejoin: match stroke_linejoin {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_miterlimit: match stroke_miterlimit {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                paint_order: match paint_order {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                color: match color {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                display: match display {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                visibility: match visibility {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                opacity: match opacity {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                fill_opacity: match fill_opacity {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                stroke_opacity: match stroke_opacity {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                text_anchor: match text_anchor {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                font_size: match font_size {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                font_family: match font_family {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                font_series: match font_series {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                font_shape: match font_shape {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                letter_spacing: match letter_spacing {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                word_spacing: match word_spacing {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                text_decoration: match text_decoration {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                text_decoration_color: match text_decoration_color {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                text_decoration_thickness: match text_decoration_thickness {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                text_decoration_style: match text_decoration_style {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                text_baseline: match text_baseline {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                baseline_shift: match baseline_shift {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                vector_effect_non_scaling_stroke: match vector_effect_non_scaling_stroke {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                marker_start: match marker_start {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                marker_mid: match marker_mid {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                marker_end: match marker_end {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
                clip_path: match clip_path {
                    Some(VectorCascadeEntry {
                        action: VectorCascadeAction::Value(value),
                        ..
                    }) => Some(value),
                    _ => None,
                },
            },
            fill_clear: matches!(
                fill.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            fill_rule_clear: matches!(
                fill_rule.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_clear: matches!(
                stroke.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_width_clear: matches!(
                stroke_width.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_dasharray_clear: matches!(
                stroke_dasharray.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_dashoffset_clear: matches!(
                stroke_dashoffset.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_linecap_clear: matches!(
                stroke_linecap.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_linejoin_clear: matches!(
                stroke_linejoin.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_miterlimit_clear: matches!(
                stroke_miterlimit.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            paint_order_clear: matches!(
                paint_order.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            color_clear: matches!(
                color.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            display_clear: matches!(
                display.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            visibility_clear: matches!(
                visibility.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            opacity_clear: matches!(
                opacity.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            fill_opacity_clear: matches!(
                fill_opacity.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            stroke_opacity_clear: matches!(
                stroke_opacity.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            text_anchor_clear: matches!(
                text_anchor.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            letter_spacing_clear: matches!(
                letter_spacing.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            word_spacing_clear: matches!(
                word_spacing.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            text_decoration_clear: matches!(
                text_decoration.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            text_decoration_color_clear: matches!(
                text_decoration_color.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            text_decoration_thickness_clear: matches!(
                text_decoration_thickness.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            text_decoration_style_clear: matches!(
                text_decoration_style.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            font_size_clear: matches!(
                font_size.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            font_family_clear: matches!(
                font_family.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            font_series_clear: matches!(
                font_series.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            font_shape_clear: matches!(
                font_shape.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            text_baseline_clear: matches!(
                text_baseline.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            baseline_shift_clear: matches!(
                baseline_shift.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            vector_effect_non_scaling_stroke_clear: matches!(
                vector_effect_non_scaling_stroke.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            marker_start_clear: matches!(
                marker_start.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            marker_mid_clear: matches!(
                marker_mid.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            marker_end_clear: matches!(
                marker_end.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
            clip_path_clear: matches!(
                clip_path.map(|value| value.action),
                Some(VectorCascadeAction::Clear)
            ),
        }
    };
    let parse_presentation = |tag: &str| -> VectorPresentation {
        let attr_presentation = parse_attr_presentation(tag);
        let style_cascade = style_rule_presentation_for(tag);
        let inline_style_presentation = parse_inline_style_presentation(tag);
        let mut presentation = overlay_presentation(attr_presentation, style_cascade.presentation);
        if style_cascade.fill_clear {
            presentation.fill = None;
        }
        if style_cascade.fill_rule_clear {
            presentation.fill_rule = None;
        }
        if style_cascade.stroke_clear {
            presentation.stroke = None;
        }
        if style_cascade.stroke_width_clear {
            presentation.stroke_width = None;
        }
        if style_cascade.stroke_dasharray_clear {
            presentation.stroke_dasharray = None;
        }
        if style_cascade.stroke_dashoffset_clear {
            presentation.stroke_dashoffset = None;
        }
        if style_cascade.stroke_linecap_clear {
            presentation.stroke_linecap = None;
        }
        if style_cascade.stroke_linejoin_clear {
            presentation.stroke_linejoin = None;
        }
        if style_cascade.stroke_miterlimit_clear {
            presentation.stroke_miterlimit = None;
        }
        if style_cascade.paint_order_clear {
            presentation.paint_order = None;
        }
        if style_cascade.color_clear {
            presentation.color = None;
        }
        if style_cascade.display_clear {
            presentation.display = None;
        }
        if style_cascade.visibility_clear {
            presentation.visibility = None;
        }
        if style_cascade.opacity_clear {
            presentation.opacity = None;
        }
        if style_cascade.fill_opacity_clear {
            presentation.fill_opacity = None;
        }
        if style_cascade.stroke_opacity_clear {
            presentation.stroke_opacity = None;
        }
        if style_cascade.text_anchor_clear {
            presentation.text_anchor = None;
        }
        if style_cascade.letter_spacing_clear {
            presentation.letter_spacing = None;
        }
        if style_cascade.word_spacing_clear {
            presentation.word_spacing = None;
        }
        if style_cascade.text_decoration_clear {
            presentation.text_decoration = None;
        }
        if style_cascade.text_decoration_color_clear {
            presentation.text_decoration_color = None;
        }
        if style_cascade.text_decoration_thickness_clear {
            presentation.text_decoration_thickness = None;
        }
        if style_cascade.text_decoration_style_clear {
            presentation.text_decoration_style = None;
        }
        if style_cascade.font_size_clear {
            presentation.font_size = None;
        }
        if style_cascade.font_family_clear {
            presentation.font_family = None;
        }
        if style_cascade.font_series_clear {
            presentation.font_series = None;
        }
        if style_cascade.font_shape_clear {
            presentation.font_shape = None;
        }
        if style_cascade.text_baseline_clear {
            presentation.text_baseline = None;
        }
        if style_cascade.baseline_shift_clear {
            presentation.baseline_shift = None;
        }
        if style_cascade.vector_effect_non_scaling_stroke_clear {
            presentation.vector_effect_non_scaling_stroke = None;
        }
        if style_cascade.marker_start_clear {
            presentation.marker_start = None;
        }
        if style_cascade.marker_mid_clear {
            presentation.marker_mid = None;
        }
        if style_cascade.marker_end_clear {
            presentation.marker_end = None;
        }
        if style_cascade.clip_path_clear {
            presentation.clip_path = None;
        }
        let mut presentation = overlay_presentation(presentation, inline_style_presentation);
        let inline_inherit_or_unset = |name: &str| {
            style_value(tag, name)
                .map(|value| {
                    let value = value.trim().to_ascii_lowercase();
                    value == "inherit" || value == "unset"
                })
                .unwrap_or(false)
        };
        if inline_inherit_or_unset("fill") {
            presentation.fill = None;
        }
        if inline_inherit_or_unset("fill-rule") {
            presentation.fill_rule = None;
        }
        if inline_inherit_or_unset("stroke") {
            presentation.stroke = None;
        }
        if inline_inherit_or_unset("stroke-width") {
            presentation.stroke_width = None;
        }
        if inline_inherit_or_unset("stroke-dasharray") {
            presentation.stroke_dasharray = None;
        }
        if inline_inherit_or_unset("stroke-dashoffset") {
            presentation.stroke_dashoffset = None;
        }
        if inline_inherit_or_unset("stroke-linecap") {
            presentation.stroke_linecap = None;
        }
        if inline_inherit_or_unset("stroke-linejoin") {
            presentation.stroke_linejoin = None;
        }
        if inline_inherit_or_unset("stroke-miterlimit") {
            presentation.stroke_miterlimit = None;
        }
        if inline_inherit_or_unset("paint-order") {
            presentation.paint_order = None;
        }
        if inline_inherit_or_unset("color") {
            presentation.color = None;
        }
        if inline_inherit_or_unset("visibility") {
            presentation.visibility = None;
        }
        if style_value(tag, "opacity")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
        {
            presentation.opacity = None;
        }
        if inline_inherit_or_unset("fill-opacity") {
            presentation.fill_opacity = None;
        }
        if inline_inherit_or_unset("stroke-opacity") {
            presentation.stroke_opacity = None;
        }
        if inline_inherit_or_unset("font-size") {
            presentation.font_size = None;
        }
        if inline_inherit_or_unset("font-family") {
            presentation.font_family = None;
        }
        if inline_inherit_or_unset("font-weight") {
            presentation.font_series = None;
        }
        if inline_inherit_or_unset("font-style") {
            presentation.font_shape = None;
        }
        if inline_inherit_or_unset("text-anchor") {
            presentation.text_anchor = None;
        }
        if inline_inherit_or_unset("letter-spacing") {
            presentation.letter_spacing = None;
        }
        if inline_inherit_or_unset("word-spacing") {
            presentation.word_spacing = None;
        }
        if first_some(
            style_value(tag, "text-decoration-line"),
            style_value(tag, "text-decoration"),
        )
        .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
        .unwrap_or(false)
        {
            presentation.text_decoration = None;
        }
        if style_value(tag, "text-decoration")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
            || style_value(tag, "text-decoration-color")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false)
        {
            presentation.text_decoration_color = None;
        }
        if style_value(tag, "text-decoration")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
            || style_value(tag, "text-decoration-thickness")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false)
        {
            presentation.text_decoration_thickness = None;
        }
        if style_value(tag, "text-decoration")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
            || style_value(tag, "text-decoration-style")
                .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
                .unwrap_or(false)
        {
            presentation.text_decoration_style = None;
        }
        if inline_inherit_or_unset("dominant-baseline")
            || inline_inherit_or_unset("alignment-baseline")
        {
            presentation.text_baseline = None;
        }
        if inline_inherit_or_unset("baseline-shift") {
            presentation.baseline_shift = None;
        }
        if style_value(tag, "vector-effect")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
        {
            presentation.vector_effect_non_scaling_stroke = None;
        }
        if inline_inherit_or_unset("marker") {
            presentation.marker_start = None;
            presentation.marker_mid = None;
            presentation.marker_end = None;
        }
        if inline_inherit_or_unset("marker-start") {
            presentation.marker_start = None;
        }
        if inline_inherit_or_unset("marker-mid") {
            presentation.marker_mid = None;
        }
        if inline_inherit_or_unset("marker-end") {
            presentation.marker_end = None;
        }
        if style_value(tag, "clip-path")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
        {
            presentation.clip_path = None;
        }
        if style_value(tag, "display")
            .map(|value| value.trim().eq_ignore_ascii_case("inherit"))
            .unwrap_or(false)
        {
            presentation.display = None;
        }
        presentation
    };
    let resolved_font_size = |presentation: VectorPresentation| -> f32 {
        match presentation.font_size {
            Some(VectorFontSize::Absolute(size)) => size,
            Some(VectorFontSize::Percent(scale)) => 12.0 * scale,
            None => 12.0,
        }
    };
    let baseline_y_offset = |presentation: VectorPresentation, font_size: f32| -> f32 {
        match presentation
            .text_baseline
            .unwrap_or(VectorTextBaseline::Alphabetic)
        {
            VectorTextBaseline::Alphabetic => 0.0,
            VectorTextBaseline::Middle => font_size * 0.5,
        }
    };
    let baseline_shift_y_offset = |presentation: VectorPresentation, font_size: f32| -> f32 {
        match presentation.baseline_shift {
            Some(VectorBaselineShift::Offset(offset)) => -offset,
            Some(VectorBaselineShift::Percent(scale)) => -font_size * scale,
            Some(VectorBaselineShift::Super) => -font_size * 0.6,
            Some(VectorBaselineShift::Sub) => font_size * 0.2,
            None => 0.0,
        }
    };
    let presentation_is_visible = |presentation: VectorPresentation| {
        presentation.display.unwrap_or(true) && presentation.visibility.unwrap_or(true)
    };
    let root_presentation = parse_presentation(svg_tag);
    let stroke_width_ratio = |presentation: VectorPresentation| -> f32 {
        presentation.stroke_width.unwrap_or(1.0) / view_box.2
    };
    let transformed_stroke_dasharray_ratio = |presentation: VectorPresentation,
                                              transform: VectorTransform|
     -> Option<VectorDashArray> {
        presentation
            .stroke_dasharray
            .unwrap_or(None)
            .map(|mut dasharray| {
                let stroke_scale = if presentation
                    .vector_effect_non_scaling_stroke
                    .unwrap_or(false)
                {
                    1.0
                } else {
                    transform.stroke_scale
                };
                for index in 0..dasharray.len {
                    dasharray.values[index] = dasharray.values[index] * stroke_scale / view_box.2;
                }
                dasharray.offset_ratio =
                    presentation.stroke_dashoffset.unwrap_or(0.0) * stroke_scale / view_box.2;
                dasharray
            })
    };
    let stroke_style = |presentation: VectorPresentation| -> VectorStrokeStyle {
        VectorStrokeStyle {
            linecap: presentation
                .stroke_linecap
                .unwrap_or(VectorStrokeLineCap::Butt),
            linejoin: presentation
                .stroke_linejoin
                .unwrap_or(VectorStrokeLineJoin::Miter),
            miterlimit: presentation.stroke_miterlimit.unwrap_or(4.0),
        }
    };
    let paint_from_resolved_color =
        |color: VectorResolvedColor, opacity: f32| -> Option<VectorPaint> {
            let opacity = (opacity * color.alpha).clamp(0.0, 1.0);
            (opacity > 0.0).then_some(VectorPaint {
                rgb: color.rgb,
                opacity,
            })
        };
    let paint_from_context = |paint: Option<VectorPaint>, opacity: f32| -> Option<VectorPaint> {
        let paint = paint?;
        let opacity = (opacity * paint.opacity).clamp(0.0, 1.0);
        (opacity > 0.0).then_some(VectorPaint {
            rgb: paint.rgb,
            opacity,
        })
    };
    let paint_from_color_with_context = |color: Option<VectorColor>,
                                         opacity: f32,
                                         current_color: VectorResolvedColor,
                                         context_fill: Option<VectorPaint>,
                                         context_stroke: Option<VectorPaint>|
     -> Option<VectorPaint> {
        match color? {
            VectorColor::Resolved(color) => paint_from_resolved_color(color, opacity),
            VectorColor::CurrentColor => paint_from_resolved_color(current_color, opacity),
            VectorColor::ContextFill => paint_from_context(context_fill, opacity),
            VectorColor::ContextStroke => paint_from_context(context_stroke, opacity),
        }
    };
    let fill_paint_with_context = |presentation: VectorPresentation,
                                   default_rgb: Option<(f32, f32, f32)>,
                                   context_fill: Option<VectorPaint>,
                                   context_stroke: Option<VectorPaint>|
     -> Option<VectorPaint> {
        let current_color = presentation
            .color
            .unwrap_or_else(|| VectorResolvedColor::opaque((0.0, 0.0, 0.0)));
        paint_from_color_with_context(
            presentation.fill.unwrap_or_else(|| {
                default_rgb
                    .map(VectorResolvedColor::opaque)
                    .map(VectorColor::Resolved)
            }),
            presentation.opacity.unwrap_or(1.0) * presentation.fill_opacity.unwrap_or(1.0),
            current_color,
            context_fill,
            context_stroke,
        )
    };
    let fill_paint = |presentation: VectorPresentation,
                      default_rgb: Option<(f32, f32, f32)>|
     -> Option<VectorPaint> {
        fill_paint_with_context(presentation, default_rgb, None, None)
    };
    let fill_rule = |presentation: VectorPresentation| -> VectorFillRule {
        presentation.fill_rule.unwrap_or(VectorFillRule::NonZero)
    };
    let stroke_paint_with_context = |presentation: VectorPresentation,
                                     context_fill: Option<VectorPaint>,
                                     context_stroke: Option<VectorPaint>|
     -> Option<VectorPaint> {
        let current_color = presentation
            .color
            .unwrap_or_else(|| VectorResolvedColor::opaque((0.0, 0.0, 0.0)));
        paint_from_color_with_context(
            presentation.stroke.unwrap_or(None),
            presentation.opacity.unwrap_or(1.0) * presentation.stroke_opacity.unwrap_or(1.0),
            current_color,
            context_fill,
            context_stroke,
        )
    };
    let stroke_paint = |presentation: VectorPresentation| -> Option<VectorPaint> {
        stroke_paint_with_context(presentation, None, None)
    };
    let text_decoration_paint = |presentation: VectorPresentation| -> Option<Option<VectorPaint>> {
        let color = presentation.text_decoration_color?;
        let current_color = presentation
            .color
            .unwrap_or_else(|| VectorResolvedColor::opaque((0.0, 0.0, 0.0)));
        Some(paint_from_color_with_context(
            color,
            presentation.opacity.unwrap_or(1.0),
            current_color,
            None,
            None,
        ))
    };
    #[derive(Debug, Clone, Copy)]
    struct VectorTransform {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
        stroke_scale: f32,
        axis_aligned: bool,
    }
    let identity_transform = VectorTransform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
        stroke_scale: 1.0,
        axis_aligned: true,
    };
    let compose_transform = |inner: VectorTransform,
                             outer: VectorTransform,
                             outer_stroke_scale: f32|
     -> VectorTransform {
        VectorTransform {
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
    let parse_transform = |tag: &str| -> Option<VectorTransform> {
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
                        VectorTransform {
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
                        VectorTransform {
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
                        VectorTransform {
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
                        VectorTransform {
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
                        VectorTransform {
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
                        VectorTransform {
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
        if value.abs() <= 0.000_1 {
            return 0.0;
        }
        let rounded = value.round();
        if (value - rounded).abs() <= 0.000_1 {
            rounded
        } else {
            value
        }
    };
    let apply_transform = |transform: VectorTransform, x: f32, y: f32| -> Option<(f32, f32)> {
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
        |presentation: VectorPresentation, transform: VectorTransform| -> f32 {
            if presentation
                .vector_effect_non_scaling_stroke
                .unwrap_or(false)
            {
                stroke_width_ratio(presentation)
            } else {
                stroke_width_ratio(presentation) * transform.stroke_scale
            }
        };
    let ellipse_path_ops = |cx: f32,
                            cy: f32,
                            rx: f32,
                            ry: f32,
                            transform: VectorTransform|
     -> Option<Vec<VectorPathOp>> {
        let kappa = 0.552_284_8_f32;
        let transform_point = |x: f32, y: f32| -> Option<(f32, f32)> {
            Some(normalize_point(apply_transform(transform, x, y)?))
        };
        Some(vec![
            VectorPathOp::MoveTo(transform_point(cx + rx, cy)?),
            VectorPathOp::CubicTo {
                ctrl1: transform_point(cx + rx, cy + kappa * ry)?,
                ctrl2: transform_point(cx + kappa * rx, cy + ry)?,
                to: transform_point(cx, cy + ry)?,
            },
            VectorPathOp::CubicTo {
                ctrl1: transform_point(cx - kappa * rx, cy + ry)?,
                ctrl2: transform_point(cx - rx, cy + kappa * ry)?,
                to: transform_point(cx - rx, cy)?,
            },
            VectorPathOp::CubicTo {
                ctrl1: transform_point(cx - rx, cy - kappa * ry)?,
                ctrl2: transform_point(cx - kappa * rx, cy - ry)?,
                to: transform_point(cx, cy - ry)?,
            },
            VectorPathOp::CubicTo {
                ctrl1: transform_point(cx + kappa * rx, cy - ry)?,
                ctrl2: transform_point(cx + rx, cy - kappa * ry)?,
                to: transform_point(cx + rx, cy)?,
            },
            VectorPathOp::Close,
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
    let parse_transformed_points =
        |raw: &str, transform: VectorTransform| -> Option<Vec<(f32, f32)>> {
            let raw_points = parse_raw_points(raw)?;
            let mut points = Vec::new();
            for (x, y) in raw_points {
                points.push(apply_transform(transform, x, y)?);
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
    struct VectorArcCommand {
        current: (f32, f32),
        rx: f32,
        ry: f32,
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        to: (f32, f32),
        transform: VectorTransform,
    }
    type SimplePathParse = Option<(Vec<VectorPathOp>, bool)>;
    let arc_to_cubics = |arc: VectorArcCommand| -> Option<Vec<VectorPathOp>> {
        let VectorArcCommand {
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
            return Some(vec![VectorPathOp::LineTo(normalize_point(
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
            ops.push(VectorPathOp::CubicTo {
                ctrl1: normalize_point(apply_transform(transform, ctrl1.0, ctrl1.1)?),
                ctrl2: normalize_point(apply_transform(transform, ctrl2.0, ctrl2.1)?),
                to: normalize_point(apply_transform(transform, segment_to.0, segment_to.1)?),
            });
        }
        Some(ops)
    };
    let parse_path = |raw: &str, transform: VectorTransform| -> SimplePathParse {
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
                    ops.push(VectorPathOp::Close);
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
                    ops.push(VectorPathOp::MoveTo(normalize_point(apply_transform(
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
                    ops.push(VectorPathOp::LineTo(normalize_point(apply_transform(
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
                    ops.push(VectorPathOp::LineTo(normalize_point(apply_transform(
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
                    ops.push(VectorPathOp::LineTo(normalize_point(apply_transform(
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
                    ops.push(VectorPathOp::CubicTo {
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
                    ops.push(VectorPathOp::CubicTo {
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
                    ops.push(VectorPathOp::CubicTo {
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
                    ops.push(VectorPathOp::CubicTo {
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
                    ops.extend(arc_to_cubics(VectorArcCommand {
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
    let mut defs_ranges = Vec::new();
    let mut defs_stack = Vec::new();
    let mut defs_search_index = 0usize;
    while let Some(relative) = svg_content[defs_search_index..].find('<') {
        let defs_tag_start = defs_search_index + relative;
        let defs_tag_tail = &svg_content[defs_tag_start..];
        let Some(defs_tag_end) = find_simple_xml_tag_end(defs_tag_tail) else {
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
    struct VectorClipPathDefinition {
        id_hash: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        transform: VectorTransform,
    }
    let mut clip_path_definitions = Vec::new();
    let mut clip_path_search_index = 0usize;
    while let Some(relative) = svg_content[clip_path_search_index..].find("<clipPath") {
        let clip_path_start = clip_path_search_index + relative;
        let clip_path_tail = &svg_content[clip_path_start..];
        if !is_start_tag_named(clip_path_tail, "clipPath") {
            clip_path_search_index = clip_path_start + "<clipPath".len();
            continue;
        }
        let Some(clip_path_tag_end) = find_simple_xml_tag_end(clip_path_tail) else {
            break;
        };
        let clip_path_tag = &clip_path_tail[..clip_path_tag_end];
        let Some(id) = attr_value(clip_path_tag, "id").filter(|id| !id.trim().is_empty()) else {
            clip_path_search_index = clip_path_start + clip_path_tag_end + 1;
            continue;
        };
        if clip_path_tag.trim_end().ends_with('/') {
            clip_path_search_index = clip_path_start + clip_path_tag_end + 1;
            continue;
        }
        let body_start = clip_path_start + clip_path_tag_end + 1;
        let Some(body_end_relative) = svg_content[body_start..].find("</clipPath>") else {
            clip_path_search_index = body_start;
            continue;
        };
        let body = &svg_content[body_start..body_start + body_end_relative];
        let next_index = body_start + body_end_relative + "</clipPath>".len();
        let clip_path_group_transform_for = |element_start: usize| -> Option<VectorTransform> {
            let mut group_stack: Vec<Option<VectorTransform>> = Vec::new();
            let mut group_search_index = 0usize;
            while let Some(group_relative) = body[group_search_index..].find('<') {
                let group_start = group_search_index + group_relative;
                if group_start >= element_start {
                    break;
                }
                let group_tail = &body[group_start..];
                let Some(group_tag_end) = find_simple_xml_tag_end(group_tail) else {
                    break;
                };
                let is_group_close = group_tail.starts_with("</g")
                    && group_tail[3..]
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_whitespace() || ch == '>');
                let is_group_open = group_tail.starts_with("<g")
                    && group_tail[2..]
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '>' | '/'));
                if is_group_close {
                    group_stack.pop();
                } else if is_group_open {
                    let group_tag = &group_tail[..group_tag_end];
                    if !group_tag.trim_end().ends_with('/') {
                        let local_transform = parse_transform(group_tag);
                        let transform = if let Some(Some(parent_transform)) = group_stack.last() {
                            local_transform.map(|local_transform| {
                                compose_transform(
                                    local_transform,
                                    *parent_transform,
                                    parent_transform.stroke_scale,
                                )
                            })
                        } else if group_stack.last().is_some() {
                            None
                        } else {
                            local_transform
                        };
                        group_stack.push(transform);
                    }
                }
                group_search_index = group_start + group_tag_end + 1;
            }
            group_stack
                .last()
                .copied()
                .unwrap_or(Some(identity_transform))
        };
        let Some(rect_relative) = body.find("<rect") else {
            clip_path_search_index = next_index;
            continue;
        };
        let rect_tail = &body[rect_relative..];
        if !is_start_tag_named(rect_tail, "rect") {
            clip_path_search_index = next_index;
            continue;
        }
        let Some(rect_tag_end) = find_simple_xml_tag_end(rect_tail) else {
            clip_path_search_index = next_index;
            continue;
        };
        let rect_tag = &rect_tail[..rect_tag_end];
        let x = attr_value(rect_tag, "x")
            .as_deref()
            .and_then(parse_x_length)
            .unwrap_or(0.0);
        let y = attr_value(rect_tag, "y")
            .as_deref()
            .and_then(parse_y_length)
            .unwrap_or(0.0);
        let Some(width) = attr_value(rect_tag, "width")
            .as_deref()
            .and_then(parse_x_length)
        else {
            clip_path_search_index = next_index;
            continue;
        };
        let Some(height) = attr_value(rect_tag, "height")
            .as_deref()
            .and_then(parse_y_length)
        else {
            clip_path_search_index = next_index;
            continue;
        };
        if width <= 0.0 || height <= 0.0 {
            clip_path_search_index = next_index;
            continue;
        }
        let Some(clip_path_transform) = parse_transform(clip_path_tag) else {
            clip_path_search_index = next_index;
            continue;
        };
        let Some(rect_transform) = parse_transform(rect_tag) else {
            clip_path_search_index = next_index;
            continue;
        };
        let Some(group_transform) = clip_path_group_transform_for(rect_relative) else {
            clip_path_search_index = next_index;
            continue;
        };
        let rect_transform = compose_transform(
            rect_transform,
            group_transform,
            group_transform.stroke_scale,
        );
        clip_path_definitions.push(VectorClipPathDefinition {
            id_hash: clip_path_id_hash(id.trim()),
            x,
            y,
            width,
            height,
            transform: compose_transform(
                rect_transform,
                clip_path_transform,
                clip_path_transform.stroke_scale,
            ),
        });
        clip_path_search_index = next_index;
    }
    let clip_rect_for = |presentation: VectorPresentation,
                         transform: VectorTransform|
     -> Option<VectorClipRect> {
        let clip_path_id = presentation.clip_path??;
        let definition = clip_path_definitions
            .iter()
            .rev()
            .find(|definition| definition.id_hash == clip_path_id)?;
        let transform = compose_transform(definition.transform, transform, transform.stroke_scale);
        if !transform.axis_aligned {
            return None;
        }
        let corner_a = apply_transform(transform, definition.x, definition.y)?;
        let corner_b = apply_transform(
            transform,
            definition.x + definition.width,
            definition.y + definition.height,
        )?;
        let x = corner_a.0.min(corner_b.0);
        let y = corner_a.1.min(corner_b.1);
        let width = (corner_b.0 - corner_a.0).abs();
        let height = (corner_b.1 - corner_a.1).abs();
        (width > 0.0 && height > 0.0).then_some(VectorClipRect {
            x_ratio: (x - view_box.0) / view_box.2,
            y_ratio: (y - view_box.1) / view_box.3,
            width_ratio: width / view_box.2,
            height_ratio: height / view_box.3,
        })
    };
    let marker_fragment_id = |presentation: VectorPresentation, specific: &str| -> Option<u64> {
        match specific {
            "marker-start" => presentation.marker_start,
            "marker-mid" => presentation.marker_mid,
            "marker-end" => presentation.marker_end,
            _ => None,
        }
        .flatten()
    };
    #[derive(Debug, Clone, Copy)]
    struct VectorGroupTransform {
        content_start: usize,
        content_end: usize,
        transform: Option<VectorTransform>,
        presentation: VectorPresentation,
    }
    #[derive(Debug, Clone)]
    struct VectorGroupDefinition {
        id: String,
        content_start: usize,
        content_end: usize,
    }
    #[derive(Debug, Clone)]
    struct VectorSymbolDefinition {
        id: String,
        content_start: usize,
        content_end: usize,
        view_box: Option<(f32, f32, f32, f32)>,
        preserve_aspect_ratio: VectorPreserveAspectRatio,
        presentation: VectorPresentation,
    }
    let mut group_transforms = Vec::new();
    let mut group_definitions = Vec::new();
    let mut group_stack: Vec<(
        usize,
        usize,
        Option<VectorTransform>,
        VectorPresentation,
        Option<String>,
    )> = Vec::new();
    let mut group_search_index = 0usize;
    while let Some(relative) = svg_content[group_search_index..].find('<') {
        let group_tag_start = group_search_index + relative;
        let group_tag_tail = &svg_content[group_tag_start..];
        let Some(group_tag_end) = find_simple_xml_tag_end(group_tag_tail) else {
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
            if let Some((tag_start, content_start, transform, presentation, id)) = group_stack.pop()
                && content_start <= group_tag_start
            {
                group_transforms.push(VectorGroupTransform {
                    content_start,
                    content_end: group_tag_start,
                    transform,
                    presentation,
                });
                if let Some(id) = id
                    && in_defs(tag_start)
                {
                    group_definitions.push(VectorGroupDefinition {
                        id,
                        content_start,
                        content_end: group_tag_start,
                    });
                }
            }
        } else if is_group_open {
            let group_tag = &group_tag_tail[..group_tag_end];
            let local_transform = parse_transform(group_tag);
            let local_presentation = parse_presentation(group_tag);
            let presentation = inherit_presentation(
                group_stack
                    .last()
                    .map(|(_, _, _, parent_presentation, _)| *parent_presentation)
                    .unwrap_or(root_presentation),
                local_presentation,
            );
            let transform = if let Some((_, _, Some(parent_transform), _, _)) = group_stack.last() {
                local_transform.map(|local| {
                    compose_transform(local, *parent_transform, parent_transform.stroke_scale)
                })
            } else if group_stack.last().is_some() {
                None
            } else {
                local_transform
            };
            if !group_tag.trim_end().ends_with('/') {
                group_stack.push((
                    group_tag_start,
                    group_tag_start + group_tag_end + 1,
                    transform,
                    presentation,
                    attr_value(group_tag, "id").filter(|id| !id.trim().is_empty()),
                ));
            }
        }
        group_search_index = group_tag_start + group_tag_end + 1;
    }
    while let Some((tag_start, content_start, transform, presentation, id)) = group_stack.pop() {
        group_transforms.push(VectorGroupTransform {
            content_start,
            content_end: svg_content.len(),
            transform,
            presentation,
        });
        if let Some(id) = id
            && in_defs(tag_start)
        {
            group_definitions.push(VectorGroupDefinition {
                id,
                content_start,
                content_end: svg_content.len(),
            });
        }
    }
    let mut symbol_definitions = Vec::new();
    let mut symbol_search_index = 0usize;
    while let Some(relative) = svg_content[symbol_search_index..].find("<symbol") {
        let symbol_start = symbol_search_index + relative;
        let symbol_tail = &svg_content[symbol_start..];
        if !is_start_tag_named(symbol_tail, "symbol") {
            symbol_search_index = symbol_start + "<symbol".len();
            continue;
        }
        let Some(symbol_tag_end) = find_simple_xml_tag_end(symbol_tail) else {
            break;
        };
        let symbol_tag = &symbol_tail[..symbol_tag_end];
        let Some(id) = attr_value(symbol_tag, "id").filter(|id| !id.trim().is_empty()) else {
            symbol_search_index = symbol_start + symbol_tag_end + 1;
            continue;
        };
        if symbol_tag.trim_end().ends_with('/') {
            symbol_search_index = symbol_start + symbol_tag_end + 1;
            continue;
        }
        let content_start = symbol_start + symbol_tag_end + 1;
        let Some(content_end_relative) = svg_content[content_start..].find("</symbol>") else {
            symbol_search_index = content_start;
            continue;
        };
        let content_end = content_start + content_end_relative;
        symbol_definitions.push(VectorSymbolDefinition {
            id,
            content_start,
            content_end,
            view_box: parse_view_box(symbol_tag),
            preserve_aspect_ratio: attr_value(symbol_tag, "preserveAspectRatio")
                .as_deref()
                .and_then(|raw| parse_preserve_aspect_ratio(raw))
                .unwrap_or(default_preserve_aspect_ratio),
            presentation: inherit_presentation(root_presentation, parse_presentation(symbol_tag)),
        });
        symbol_search_index = content_end + "</symbol>".len();
    }
    let group_state_for = |element_start: usize| -> (Option<VectorTransform>, VectorPresentation) {
        let mut selected_group: Option<&VectorGroupTransform> = None;
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
        |tag: &str, element_start: usize| -> Option<(VectorTransform, VectorPresentation)> {
            let (group_transform, group_presentation) = group_state_for(element_start);
            let group_transform = group_transform?;
            let element_transform = parse_transform(tag)?;
            let presentation = inherit_presentation(group_presentation, parse_presentation(tag));
            if !presentation_is_visible(presentation) {
                return None;
            }
            Some((
                compose_transform(
                    element_transform,
                    group_transform,
                    group_transform.stroke_scale,
                ),
                presentation,
            ))
        };
    #[derive(Debug, Clone, Copy)]
    enum VectorMarkerOrient {
        Auto,
        AutoStartReverse,
        Angle(f32),
    }
    #[derive(Debug, Clone)]
    struct VectorMarkerShape {
        path_data: String,
        transform: VectorTransform,
        presentation: VectorPresentation,
    }
    #[derive(Debug, Clone)]
    struct VectorMarkerDefinition {
        id_hash: u64,
        marker_width: f32,
        marker_height: f32,
        ref_x: f32,
        ref_y: f32,
        marker_units_stroke_width: bool,
        orient: VectorMarkerOrient,
        view_box: Option<(f32, f32, f32, f32)>,
        preserve_aspect_ratio: VectorPreserveAspectRatio,
        shapes: Vec<VectorMarkerShape>,
    }
    let parse_marker_orient = |raw: Option<String>| -> VectorMarkerOrient {
        let Some(raw) = raw else {
            return VectorMarkerOrient::Angle(0.0);
        };
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("auto") {
            return VectorMarkerOrient::Auto;
        }
        if raw.eq_ignore_ascii_case("auto-start-reverse") {
            return VectorMarkerOrient::AutoStartReverse;
        }
        parse_number_prefix(raw)
            .map(VectorMarkerOrient::Angle)
            .unwrap_or(VectorMarkerOrient::Angle(0.0))
    };
    let mut marker_definitions = Vec::new();
    let mut marker_search_index = 0usize;
    while let Some(relative) = svg_content[marker_search_index..].find("<marker") {
        let marker_start = marker_search_index + relative;
        let marker_tail = &svg_content[marker_start..];
        if !is_start_tag_named(marker_tail, "marker") {
            marker_search_index = marker_start + "<marker".len();
            continue;
        }
        let Some(marker_tag_end) = find_simple_xml_tag_end(marker_tail) else {
            break;
        };
        let marker_tag = &marker_tail[..marker_tag_end];
        let Some(id) = attr_value(marker_tag, "id").filter(|id| !id.trim().is_empty()) else {
            marker_search_index = marker_start + marker_tag_end + 1;
            continue;
        };
        if marker_tag.trim_end().ends_with('/') {
            marker_search_index = marker_start + marker_tag_end + 1;
            continue;
        }
        let body_start = marker_start + marker_tag_end + 1;
        let Some(body_end_relative) = svg_content[body_start..].find("</marker>") else {
            marker_search_index = body_start;
            continue;
        };
        let body_end = body_start + body_end_relative;
        let body = &svg_content[body_start..body_end];
        let marker_width = attr_value(marker_tag, "markerWidth")
            .as_deref()
            .and_then(parse_number_prefix)
            .filter(|value| *value > 0.0)
            .unwrap_or(3.0);
        let marker_height = attr_value(marker_tag, "markerHeight")
            .as_deref()
            .and_then(parse_number_prefix)
            .filter(|value| *value > 0.0)
            .unwrap_or(3.0);
        let ref_x = attr_value(marker_tag, "refX")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let ref_y = attr_value(marker_tag, "refY")
            .as_deref()
            .and_then(parse_number_prefix)
            .unwrap_or(0.0);
        let marker_units_stroke_width = attr_value(marker_tag, "markerUnits")
            .map(|value| !value.trim().eq_ignore_ascii_case("userSpaceOnUse"))
            .unwrap_or(true);
        let presentation = inherit_presentation(root_presentation, parse_presentation(marker_tag));
        if !presentation_is_visible(presentation) {
            marker_search_index = body_end + "</marker>".len();
            continue;
        }
        let marker_group_state_for = |element_start: usize| {
            let mut selected_group: Option<&VectorGroupTransform> = None;
            for group in &group_transforms {
                if body_start <= group.content_start
                    && group.content_end <= body_end
                    && group.content_start <= element_start
                    && element_start < group.content_end
                {
                    selected_group = match selected_group {
                        Some(selected) if selected.content_start > group.content_start => {
                            Some(selected)
                        }
                        _ => Some(group),
                    };
                }
            }
            selected_group
                .map(|group| {
                    group
                        .transform
                        .map(|transform| (transform, Some(group.presentation)))
                })
                .unwrap_or(Some((identity_transform, None)))
        };
        let mut shapes = Vec::new();
        let mut body_search_index = 0usize;
        while let Some(child_relative) = body[body_search_index..].find('<') {
            let child_start_in_body = body_search_index + child_relative;
            let child_start = body_start + child_start_in_body;
            let child_tail = &body[child_start_in_body..];
            let Some(child_tag_end) = find_simple_xml_tag_end(child_tail) else {
                break;
            };
            let child_tag = &child_tail[..child_tag_end];
            if child_tag.starts_with("</") {
                body_search_index = child_start_in_body + child_tag_end + 1;
                continue;
            }
            let path_data = if is_start_tag_named(child_tail, "path") {
                attr_value(child_tag, "d")
            } else if is_start_tag_named(child_tail, "polygon") {
                attr_value(child_tag, "points")
                    .as_deref()
                    .and_then(parse_raw_points)
                    .and_then(|points| path_data_from_points(&points, true))
            } else if is_start_tag_named(child_tail, "polyline") {
                attr_value(child_tag, "points")
                    .as_deref()
                    .and_then(parse_raw_points)
                    .and_then(|points| path_data_from_points(&points, false))
            } else if is_start_tag_named(child_tail, "rect") {
                let x = attr_value(child_tag, "x")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let y = attr_value(child_tag, "y")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let width = attr_value(child_tag, "width")
                    .as_deref()
                    .and_then(parse_number_prefix);
                let height = attr_value(child_tag, "height")
                    .as_deref()
                    .and_then(parse_number_prefix);
                let rx_raw = attr_value(child_tag, "rx")
                    .as_deref()
                    .and_then(parse_number_prefix);
                let ry_raw = attr_value(child_tag, "ry")
                    .as_deref()
                    .and_then(parse_number_prefix);
                if let Some((width, height)) = width.zip(height)
                    && width.is_finite()
                    && height.is_finite()
                    && width > 0.0
                    && height > 0.0
                {
                    let rounded_radii = match (rx_raw, ry_raw) {
                        (Some(rx), Some(ry)) if rx > 0.0 && ry > 0.0 => {
                            Some((rx.min(width / 2.0), ry.min(height / 2.0)))
                        }
                        (Some(radius), None) | (None, Some(radius)) if radius > 0.0 => {
                            let radius = radius.min(width / 2.0).min(height / 2.0);
                            Some((radius, radius))
                        }
                        _ => None,
                    };
                    if let Some((rx, ry)) = rounded_radii {
                        let kappa = 0.552_284_8_f32;
                        Some(format!(
                            "M {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} Z",
                            x + rx,
                            y,
                            x + width - rx,
                            y,
                            x + width - rx + kappa * rx,
                            y,
                            x + width,
                            y + ry - kappa * ry,
                            x + width,
                            y + ry,
                            x + width,
                            y + height - ry,
                            x + width,
                            y + height - ry + kappa * ry,
                            x + width - rx + kappa * rx,
                            y + height,
                            x + width - rx,
                            y + height,
                            x + rx,
                            y + height,
                            x + rx - kappa * rx,
                            y + height,
                            x,
                            y + height - ry + kappa * ry,
                            x,
                            y + height - ry,
                            x,
                            y + ry,
                            x,
                            y + ry - kappa * ry,
                            x + rx - kappa * rx,
                            y,
                            x + rx,
                            y
                        ))
                    } else {
                        rect_path_data(x, y, width, height)
                    }
                } else {
                    None
                }
            } else if is_start_tag_named(child_tail, "line") {
                let x1 = attr_value(child_tag, "x1")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let y1 = attr_value(child_tag, "y1")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let x2 = attr_value(child_tag, "x2")
                    .as_deref()
                    .and_then(parse_number_prefix);
                let y2 = attr_value(child_tag, "y2")
                    .as_deref()
                    .and_then(parse_number_prefix);
                x2.zip(y2)
                    .and_then(|(x2, y2)| path_data_from_points(&[(x1, y1), (x2, y2)], false))
            } else if is_start_tag_named(child_tail, "circle") {
                let cx = attr_value(child_tag, "cx")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let cy = attr_value(child_tag, "cy")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                attr_value(child_tag, "r")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .and_then(|radius| ellipse_path_data(cx, cy, radius, radius))
            } else if is_start_tag_named(child_tail, "ellipse") {
                let cx = attr_value(child_tag, "cx")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let cy = attr_value(child_tag, "cy")
                    .as_deref()
                    .and_then(parse_number_prefix)
                    .unwrap_or(0.0);
                let rx = attr_value(child_tag, "rx")
                    .as_deref()
                    .and_then(parse_number_prefix);
                let ry = attr_value(child_tag, "ry")
                    .as_deref()
                    .and_then(parse_number_prefix);
                rx.zip(ry)
                    .and_then(|(rx, ry)| ellipse_path_data(cx, cy, rx, ry))
            } else {
                None
            };
            if let Some(path_data) = path_data
                && let Some(child_transform) = parse_transform(child_tag)
                && let Some((group_transform, group_presentation)) =
                    marker_group_state_for(child_start)
            {
                let child_transform = compose_transform(
                    child_transform,
                    group_transform,
                    group_transform.stroke_scale,
                );
                let base_presentation = group_presentation
                    .map(|group_presentation| {
                        inherit_presentation(presentation, group_presentation)
                    })
                    .unwrap_or(presentation);
                let child_presentation =
                    inherit_presentation(base_presentation, parse_presentation(child_tag));
                if presentation_is_visible(child_presentation) {
                    shapes.push(VectorMarkerShape {
                        path_data,
                        transform: child_transform,
                        presentation: child_presentation,
                    });
                }
            }
            body_search_index = child_start_in_body + child_tag_end + 1;
        }
        if !shapes.is_empty() {
            marker_definitions.push(VectorMarkerDefinition {
                id_hash: clip_path_id_hash(id.trim()),
                marker_width,
                marker_height,
                ref_x,
                ref_y,
                marker_units_stroke_width,
                orient: parse_marker_orient(attr_value(marker_tag, "orient")),
                view_box: parse_view_box(marker_tag),
                preserve_aspect_ratio: attr_value(marker_tag, "preserveAspectRatio")
                    .as_deref()
                    .and_then(|raw| parse_preserve_aspect_ratio(raw))
                    .unwrap_or(default_preserve_aspect_ratio),
                shapes,
            });
        }
        marker_search_index = body_end + "</marker>".len();
    }
    let marker_paths = |marker_id: u64,
                        endpoint_x: f32,
                        endpoint_y: f32,
                        tangent_dx: f32,
                        tangent_dy: f32,
                        at_start: bool,
                        line_presentation: VectorPresentation,
                        line_transform: VectorTransform|
     -> Option<Vec<VectorPath>> {
        let definition = marker_definitions
            .iter()
            .rev()
            .find(|definition| definition.id_hash == marker_id)?;
        let line_length = tangent_dx.hypot(tangent_dy);
        if !line_length.is_finite() || line_length <= f32::EPSILON {
            return None;
        }
        let stroke_width_user = (transformed_stroke_width_ratio(line_presentation, line_transform)
            * view_box.2)
            .max(0.0);
        let marker_units_scale = if definition.marker_units_stroke_width {
            stroke_width_user.max(0.000_1)
        } else {
            1.0
        };
        let viewport_width = definition.marker_width * marker_units_scale;
        let viewport_height = definition.marker_height * marker_units_scale;
        if !viewport_width.is_finite()
            || !viewport_height.is_finite()
            || viewport_width <= 0.0
            || viewport_height <= 0.0
        {
            return None;
        }
        let marker_view_box = definition.view_box.unwrap_or((
            0.0,
            0.0,
            definition.marker_width,
            definition.marker_height,
        ));
        let (scale_x, scale_y, offset_x, offset_y) =
            if definition.preserve_aspect_ratio.scale == VectorAspectScale::None {
                (
                    viewport_width / marker_view_box.2,
                    viewport_height / marker_view_box.3,
                    0.0,
                    0.0,
                )
            } else {
                let scale = match definition.preserve_aspect_ratio.scale {
                    VectorAspectScale::Meet => (viewport_width / marker_view_box.2)
                        .min(viewport_height / marker_view_box.3),
                    VectorAspectScale::Slice => (viewport_width / marker_view_box.2)
                        .max(viewport_height / marker_view_box.3),
                    VectorAspectScale::None => unreachable!(),
                };
                let fit_width = marker_view_box.2 * scale;
                let fit_height = marker_view_box.3 * scale;
                let offset_x = match definition.preserve_aspect_ratio.x_align {
                    VectorAspectAlign::Min => 0.0,
                    VectorAspectAlign::Mid => (viewport_width - fit_width) / 2.0,
                    VectorAspectAlign::Max => viewport_width - fit_width,
                };
                let offset_y = match definition.preserve_aspect_ratio.y_align {
                    VectorAspectAlign::Min => 0.0,
                    VectorAspectAlign::Mid => (viewport_height - fit_height) / 2.0,
                    VectorAspectAlign::Max => viewport_height - fit_height,
                };
                (scale, scale, offset_x, offset_y)
            };
        if !scale_x.is_finite() || !scale_y.is_finite() || scale_x == 0.0 || scale_y == 0.0 {
            return None;
        }
        let view_box_transform = VectorTransform {
            a: scale_x,
            b: 0.0,
            c: 0.0,
            d: scale_y,
            e: offset_x - marker_view_box.0 * scale_x,
            f: offset_y - marker_view_box.1 * scale_y,
            stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
            axis_aligned: true,
        };
        let ref_point = apply_transform(view_box_transform, definition.ref_x, definition.ref_y)?;
        let ref_transform = VectorTransform {
            e: -ref_point.0,
            f: -ref_point.1,
            ..identity_transform
        };
        let tangent_angle_degrees = tangent_dy.atan2(tangent_dx).to_degrees();
        let angle_degrees = match definition.orient {
            VectorMarkerOrient::Auto => tangent_angle_degrees,
            VectorMarkerOrient::AutoStartReverse if at_start => tangent_angle_degrees + 180.0,
            VectorMarkerOrient::AutoStartReverse => tangent_angle_degrees,
            VectorMarkerOrient::Angle(angle) => angle,
        };
        let radians = angle_degrees.to_radians();
        let cos = radians.cos();
        let sin = radians.sin();
        let rotate_transform = VectorTransform {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            stroke_scale: 1.0,
            axis_aligned: sin.abs() <= f32::EPSILON,
            ..identity_transform
        };
        let translate_transform = VectorTransform {
            e: endpoint_x,
            f: endpoint_y,
            ..identity_transform
        };
        let context_fill = fill_paint(line_presentation, None);
        let context_stroke = stroke_paint(line_presentation);
        let mut paths = Vec::new();
        for shape in &definition.shapes {
            let transform = compose_transform(
                shape.transform,
                view_box_transform,
                view_box_transform.stroke_scale,
            );
            let transform = compose_transform(transform, ref_transform, ref_transform.stroke_scale);
            let transform =
                compose_transform(transform, rotate_transform, rotate_transform.stroke_scale);
            let transform = compose_transform(
                transform,
                translate_transform,
                translate_transform.stroke_scale,
            );
            let Some((ops, _closed)) = parse_path(&shape.path_data, transform) else {
                continue;
            };
            let fill = fill_paint_with_context(
                shape.presentation,
                Some((0.0, 0.0, 0.0)),
                context_fill,
                context_stroke,
            );
            let stroke =
                stroke_paint_with_context(shape.presentation, context_fill, context_stroke);
            if fill.is_none() && stroke.is_none() {
                continue;
            }
            paths.push(VectorPath {
                ops,
                fill,
                fill_rule: fill_rule(shape.presentation),
                stroke,
                stroke_width_ratio: transformed_stroke_width_ratio(shape.presentation, transform),
                stroke_dasharray: transformed_stroke_dasharray_ratio(shape.presentation, transform),
                stroke_style: stroke_style(shape.presentation),
                paint_order: shape
                    .presentation
                    .paint_order
                    .unwrap_or(VectorPaintOrder::Normal),
                clip_rect: None,
            });
        }
        (!paths.is_empty()).then_some(paths)
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
        let Some(rect_end) = find_simple_xml_tag_end(rect_tail) else {
            break;
        };
        if in_defs(rect_start) {
            search_index = rect_start + rect_end + 1;
            continue;
        }
        let rect_tag = &rect_tail[..rect_end];
        let x = attr_value(rect_tag, "x")
            .as_deref()
            .and_then(parse_x_length)
            .unwrap_or(0.0);
        let y = attr_value(rect_tag, "y")
            .as_deref()
            .and_then(parse_y_length)
            .unwrap_or(0.0);
        let Some(width) = attr_value(rect_tag, "width")
            .as_deref()
            .and_then(parse_x_length)
        else {
            search_index = rect_start + rect_end + 1;
            continue;
        };
        let Some(height) = attr_value(rect_tag, "height")
            .as_deref()
            .and_then(parse_y_length)
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
                .and_then(parse_x_length);
            let ry_raw = attr_value(rect_tag, "ry")
                .as_deref()
                .and_then(parse_y_length);
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
                    shape_paths.push(VectorPath {
                        ops: vec![
                            VectorPathOp::MoveTo(start),
                            VectorPathOp::LineTo(top_end),
                            VectorPathOp::CubicTo {
                                ctrl1: top_right_ctrl1,
                                ctrl2: top_right_ctrl2,
                                to: right_start,
                            },
                            VectorPathOp::LineTo(right_end),
                            VectorPathOp::CubicTo {
                                ctrl1: bottom_right_ctrl1,
                                ctrl2: bottom_right_ctrl2,
                                to: bottom_start,
                            },
                            VectorPathOp::LineTo(bottom_end),
                            VectorPathOp::CubicTo {
                                ctrl1: bottom_left_ctrl1,
                                ctrl2: bottom_left_ctrl2,
                                to: left_start,
                            },
                            VectorPathOp::LineTo(left_end),
                            VectorPathOp::CubicTo {
                                ctrl1: top_left_ctrl1,
                                ctrl2: top_left_ctrl2,
                                to: start,
                            },
                            VectorPathOp::Close,
                        ],
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: transformed_stroke_dasharray_ratio(
                            presentation,
                            transform,
                        ),
                        stroke_style: stroke_style(presentation),
                        paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                        clip_rect: clip_rect_for(presentation, transform),
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
                    rect_polys.push(VectorPoly {
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
                        stroke_dasharray: transformed_stroke_dasharray_ratio(
                            presentation,
                            transform,
                        ),
                        stroke_style: stroke_style(presentation),
                        paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                        clip_rect: clip_rect_for(presentation, transform),
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
                rects.push(VectorRect {
                    x_ratio: (x - view_box.0) / view_box.2,
                    y_ratio: (y - view_box.1) / view_box.3,
                    width_ratio: width / view_box.2,
                    height_ratio: height / view_box.3,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                    stroke_style: stroke_style(presentation),
                    paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                    clip_rect: clip_rect_for(presentation, transform),
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
        let Some(line_end) = find_simple_xml_tag_end(line_tail) else {
            break;
        };
        if in_defs(line_start) {
            search_index = line_start + line_end + 1;
            continue;
        }
        let line_tag = &line_tail[..line_end];
        let Some(x1) = attr_value(line_tag, "x1")
            .as_deref()
            .and_then(parse_x_length)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some(y1) = attr_value(line_tag, "y1")
            .as_deref()
            .and_then(parse_y_length)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some(x2) = attr_value(line_tag, "x2")
            .as_deref()
            .and_then(parse_x_length)
        else {
            search_index = line_start + line_end + 1;
            continue;
        };
        let Some(y2) = attr_value(line_tag, "y2")
            .as_deref()
            .and_then(parse_y_length)
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
            let marker_start_id = marker_fragment_id(presentation, "marker-start");
            let marker_end_id = marker_fragment_id(presentation, "marker-end");
            let tangent_dx = x2 - x1;
            let tangent_dy = y2 - y1;
            if let Some(marker_start_id) = marker_start_id
                && let Some(marker_paths) = marker_paths(
                    marker_start_id,
                    x1,
                    y1,
                    tangent_dx,
                    tangent_dy,
                    true,
                    presentation,
                    transform,
                )
            {
                shape_paths.extend(marker_paths);
            }
            if let Some(marker_end_id) = marker_end_id
                && let Some(marker_paths) = marker_paths(
                    marker_end_id,
                    x2,
                    y2,
                    tangent_dx,
                    tangent_dy,
                    false,
                    presentation,
                    transform,
                )
            {
                shape_paths.extend(marker_paths);
            }
            lines.push(VectorLine {
                x1_ratio: (x1 - view_box.0) / view_box.2,
                y1_ratio: (y1 - view_box.1) / view_box.3,
                x2_ratio: (x2 - view_box.0) / view_box.2,
                y2_ratio: (y2 - view_box.1) / view_box.3,
                stroke,
                stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                stroke_style: stroke_style(presentation),
                clip_rect: clip_rect_for(presentation, transform),
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
        let Some(circle_end) = find_simple_xml_tag_end(circle_tail) else {
            break;
        };
        if in_defs(circle_start) {
            search_index = circle_start + circle_end + 1;
            continue;
        }
        let circle_tag = &circle_tail[..circle_end];
        let cx = attr_value(circle_tag, "cx")
            .as_deref()
            .and_then(parse_x_length)
            .unwrap_or(0.0);
        let cy = attr_value(circle_tag, "cy")
            .as_deref()
            .and_then(parse_y_length)
            .unwrap_or(0.0);
        let Some(radius) = attr_value(circle_tag, "r")
            .as_deref()
            .and_then(parse_diagonal_length)
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
                    shape_paths.push(VectorPath {
                        ops,
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: transformed_stroke_dasharray_ratio(
                            presentation,
                            transform,
                        ),
                        stroke_style: stroke_style(presentation),
                        paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                        clip_rect: clip_rect_for(presentation, transform),
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
                ellipses.push(VectorEllipse {
                    cx_ratio: (center.0 - view_box.0) / view_box.2,
                    cy_ratio: (center.1 - view_box.1) / view_box.3,
                    rx_ratio: rx / view_box.2,
                    ry_ratio: ry / view_box.3,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                    stroke_style: stroke_style(presentation),
                    paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                    clip_rect: clip_rect_for(presentation, transform),
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
        let Some(ellipse_end) = find_simple_xml_tag_end(ellipse_tail) else {
            break;
        };
        if in_defs(ellipse_start) {
            search_index = ellipse_start + ellipse_end + 1;
            continue;
        }
        let ellipse_tag = &ellipse_tail[..ellipse_end];
        let cx = attr_value(ellipse_tag, "cx")
            .as_deref()
            .and_then(parse_x_length)
            .unwrap_or(0.0);
        let cy = attr_value(ellipse_tag, "cy")
            .as_deref()
            .and_then(parse_y_length)
            .unwrap_or(0.0);
        let Some(rx) = attr_value(ellipse_tag, "rx")
            .as_deref()
            .and_then(parse_x_length)
        else {
            search_index = ellipse_start + ellipse_end + 1;
            continue;
        };
        let Some(ry) = attr_value(ellipse_tag, "ry")
            .as_deref()
            .and_then(parse_y_length)
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
                    shape_paths.push(VectorPath {
                        ops,
                        fill,
                        fill_rule: fill_rule(presentation),
                        stroke,
                        stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                        stroke_dasharray: transformed_stroke_dasharray_ratio(
                            presentation,
                            transform,
                        ),
                        stroke_style: stroke_style(presentation),
                        paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                        clip_rect: clip_rect_for(presentation, transform),
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
                ellipses.push(VectorEllipse {
                    cx_ratio: (center.0 - view_box.0) / view_box.2,
                    cy_ratio: (center.1 - view_box.1) / view_box.3,
                    rx_ratio: rx / view_box.2,
                    ry_ratio: ry / view_box.3,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                    stroke_style: stroke_style(presentation),
                    paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                    clip_rect: clip_rect_for(presentation, transform),
                });
            }
        }
        search_index = ellipse_start + ellipse_end + 1;
    }
    let mut polys = rect_polys;
    let push_poly_markers = |shape_paths: &mut Vec<VectorPath>,
                             _tag: &str,
                             points: &[(f32, f32)],
                             closed: bool,
                             presentation: VectorPresentation,
                             transform: VectorTransform| {
        if points.len() < 2 {
            return;
        }
        let marker_start_id = marker_fragment_id(presentation, "marker-start");
        let marker_mid_id = marker_fragment_id(presentation, "marker-mid");
        let marker_end_id = marker_fragment_id(presentation, "marker-end");
        if marker_start_id.is_none() && marker_mid_id.is_none() && marker_end_id.is_none() {
            return;
        }
        let push_marker = |shape_paths: &mut Vec<VectorPath>,
                           marker_id: Option<u64>,
                           endpoint: (f32, f32),
                           tangent: (f32, f32),
                           at_start: bool| {
            if let Some(marker_id) = marker_id
                && let Some(marker_paths) = marker_paths(
                    marker_id,
                    endpoint.0,
                    endpoint.1,
                    tangent.0,
                    tangent.1,
                    at_start,
                    presentation,
                    transform,
                )
            {
                shape_paths.extend(marker_paths);
            }
        };
        push_marker(
            shape_paths,
            marker_start_id,
            points[0],
            (points[1].0 - points[0].0, points[1].1 - points[0].1),
            true,
        );
        if let Some(marker_mid_id) = marker_mid_id {
            if closed {
                for point_index in 0..points.len() {
                    let prev = points[(point_index + points.len() - 1) % points.len()];
                    let current = points[point_index];
                    let next = points[(point_index + 1) % points.len()];
                    push_marker(
                        shape_paths,
                        Some(marker_mid_id),
                        current,
                        (next.0 - prev.0, next.1 - prev.1),
                        false,
                    );
                }
            } else {
                for point_index in 1..points.len().saturating_sub(1) {
                    let prev = points[point_index - 1];
                    let current = points[point_index];
                    let next = points[point_index + 1];
                    push_marker(
                        shape_paths,
                        Some(marker_mid_id),
                        current,
                        (next.0 - prev.0, next.1 - prev.1),
                        false,
                    );
                }
            }
        }
        let last_index = points.len() - 1;
        let end_point = if closed {
            points[0]
        } else {
            points[last_index]
        };
        let end_prev = if closed {
            points[last_index]
        } else {
            points[last_index - 1]
        };
        push_marker(
            shape_paths,
            marker_end_id,
            end_point,
            (end_point.0 - end_prev.0, end_point.1 - end_prev.1),
            false,
        );
    };
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<polyline") {
        let poly_start = search_index + relative;
        let poly_tail = &svg_content[poly_start..];
        if !is_start_tag_named(poly_tail, "polyline") {
            search_index = poly_start + "<polyline".len();
            continue;
        }
        let Some(poly_end) = find_simple_xml_tag_end(poly_tail) else {
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
                .and_then(|points| parse_transformed_points(points, transform))
        {
            push_poly_markers(
                &mut shape_paths,
                poly_tag,
                &points,
                false,
                presentation,
                transform,
            );
            let points = points.into_iter().map(normalize_point).collect();
            let fill = fill_paint(presentation, None);
            let stroke = stroke_paint(presentation);
            if fill.is_some() || stroke.is_some() {
                polys.push(VectorPoly {
                    points,
                    closed: false,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                    stroke_style: stroke_style(presentation),
                    paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                    clip_rect: clip_rect_for(presentation, transform),
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
        let Some(poly_end) = find_simple_xml_tag_end(poly_tail) else {
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
                .and_then(|points| parse_transformed_points(points, transform))
        {
            push_poly_markers(
                &mut shape_paths,
                poly_tag,
                &points,
                true,
                presentation,
                transform,
            );
            let points = points.into_iter().map(normalize_point).collect();
            let fill = fill_paint(presentation, Some((0.0, 0.0, 0.0)));
            let stroke = stroke_paint(presentation);
            if fill.is_some() || stroke.is_some() {
                polys.push(VectorPoly {
                    points,
                    closed: true,
                    fill,
                    fill_rule: fill_rule(presentation),
                    stroke,
                    stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                    stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                    stroke_style: stroke_style(presentation),
                    paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                    clip_rect: clip_rect_for(presentation, transform),
                });
            }
        }
        search_index = poly_start + poly_end + 1;
    }
    let mut paths = shape_paths;
    let push_path_markers = |paths: &mut Vec<VectorPath>,
                             _tag: &str,
                             ops: &[VectorPathOp],
                             presentation: VectorPresentation,
                             transform: VectorTransform| {
        let marker_start_id = marker_fragment_id(presentation, "marker-start");
        let marker_mid_id = marker_fragment_id(presentation, "marker-mid");
        let marker_end_id = marker_fragment_id(presentation, "marker-end");
        if marker_start_id.is_none() && marker_mid_id.is_none() && marker_end_id.is_none() {
            return;
        }
        #[derive(Debug, Clone, Copy)]
        struct VectorPathMarkerSegment {
            start: (f32, f32),
            end: (f32, f32),
            start_tangent: (f32, f32),
            end_tangent: (f32, f32),
        }
        let denormalize_point = |point: (f32, f32)| -> (f32, f32) {
            (
                point.0 * view_box.2 + view_box.0,
                point.1 * view_box.3 + view_box.1,
            )
        };
        let tangent_or_fallback = |primary: (f32, f32), fallback: (f32, f32)| {
            if primary.0.hypot(primary.1) > f32::EPSILON {
                primary
            } else {
                fallback
            }
        };
        let mut segments = Vec::new();
        let mut current = None;
        let mut subpath_start = None;
        for op in ops {
            match *op {
                VectorPathOp::MoveTo(point) => {
                    current = Some(point);
                    subpath_start = Some(point);
                }
                VectorPathOp::LineTo(to) => {
                    if let Some(from) = current {
                        let start = denormalize_point(from);
                        let end = denormalize_point(to);
                        let tangent = (end.0 - start.0, end.1 - start.1);
                        segments.push(VectorPathMarkerSegment {
                            start,
                            end,
                            start_tangent: tangent,
                            end_tangent: tangent,
                        });
                    }
                    current = Some(to);
                }
                VectorPathOp::CubicTo { ctrl1, ctrl2, to } => {
                    if let Some(from) = current {
                        let start = denormalize_point(from);
                        let ctrl1 = denormalize_point(ctrl1);
                        let ctrl2 = denormalize_point(ctrl2);
                        let end = denormalize_point(to);
                        let chord = (end.0 - start.0, end.1 - start.1);
                        let start_tangent =
                            tangent_or_fallback((ctrl1.0 - start.0, ctrl1.1 - start.1), chord);
                        let end_tangent =
                            tangent_or_fallback((end.0 - ctrl2.0, end.1 - ctrl2.1), chord);
                        segments.push(VectorPathMarkerSegment {
                            start,
                            end,
                            start_tangent,
                            end_tangent,
                        });
                    }
                    current = Some(to);
                }
                VectorPathOp::Close => {
                    if let (Some(from), Some(to)) = (current, subpath_start) {
                        let start = denormalize_point(from);
                        let end = denormalize_point(to);
                        let tangent = (end.0 - start.0, end.1 - start.1);
                        if tangent.0.hypot(tangent.1) > f32::EPSILON {
                            segments.push(VectorPathMarkerSegment {
                                start,
                                end,
                                start_tangent: tangent,
                                end_tangent: tangent,
                            });
                        }
                        current = Some(to);
                    }
                }
            }
        }
        if segments.is_empty() {
            return;
        }
        let push_marker = |paths: &mut Vec<VectorPath>,
                           marker_id: Option<u64>,
                           endpoint: (f32, f32),
                           tangent: (f32, f32),
                           at_start: bool| {
            if let Some(marker_id) = marker_id
                && let Some(marker_paths) = marker_paths(
                    marker_id,
                    endpoint.0,
                    endpoint.1,
                    tangent.0,
                    tangent.1,
                    at_start,
                    presentation,
                    transform,
                )
            {
                paths.extend(marker_paths);
            }
        };
        let first = segments[0];
        push_marker(
            paths,
            marker_start_id,
            first.start,
            first.start_tangent,
            true,
        );
        if let Some(marker_mid_id) = marker_mid_id {
            for pair in segments.windows(2) {
                let previous = pair[0];
                let next = pair[1];
                let tangent = tangent_or_fallback(
                    (
                        previous.end_tangent.0 + next.start_tangent.0,
                        previous.end_tangent.1 + next.start_tangent.1,
                    ),
                    next.start_tangent,
                );
                push_marker(paths, Some(marker_mid_id), previous.end, tangent, false);
            }
        }
        let last = segments[segments.len() - 1];
        push_marker(paths, marker_end_id, last.end, last.end_tangent, false);
    };
    let push_simple_svg_path = |paths: &mut Vec<VectorPath>,
                                tag: &str,
                                transform: VectorTransform,
                                presentation: VectorPresentation,
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
        let marker_ops = ops.clone();
        paths.push(VectorPath {
            ops,
            fill,
            fill_rule: fill_rule(presentation),
            stroke,
            stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
            stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
            stroke_style: stroke_style(presentation),
            paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
            clip_rect: clip_rect_for(presentation, transform),
        });
        push_path_markers(paths, tag, &marker_ops, presentation, transform);
        true
    };
    #[derive(Debug, Clone)]
    struct VectorPathLikeDefinition {
        id: String,
        tag: String,
        start: usize,
        path_data: String,
        alias_base_presentation: Option<VectorPresentation>,
        alias_transform: Option<VectorTransform>,
        alias_presentation: Option<VectorPresentation>,
    }
    let push_path_like_definition = |path_like_definitions: &mut Vec<VectorPathLikeDefinition>,
                                     element_id: Option<String>,
                                     tag: &str,
                                     start: usize,
                                     path_data: String| {
        if let Some(id) = element_id.filter(|id| !id.trim().is_empty()) {
            path_like_definitions.push(VectorPathLikeDefinition {
                id,
                tag: tag.to_string(),
                start,
                path_data: path_data.clone(),
                alias_base_presentation: None,
                alias_transform: None,
                alias_presentation: None,
            });
        }
        for group_definition in &group_definitions {
            if group_definition.content_start <= start && start < group_definition.content_end {
                path_like_definitions.push(VectorPathLikeDefinition {
                    id: group_definition.id.clone(),
                    tag: tag.to_string(),
                    start,
                    path_data: path_data.clone(),
                    alias_base_presentation: None,
                    alias_transform: None,
                    alias_presentation: None,
                });
            }
        }
        for symbol_definition in &symbol_definitions {
            if symbol_definition.content_start <= start && start < symbol_definition.content_end {
                path_like_definitions.push(VectorPathLikeDefinition {
                    id: symbol_definition.id.clone(),
                    tag: tag.to_string(),
                    start,
                    path_data: path_data.clone(),
                    alias_base_presentation: None,
                    alias_transform: None,
                    alias_presentation: None,
                });
            }
        }
    };
    let mut path_like_definitions = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<path") {
        let path_start = search_index + relative;
        let path_tail = &svg_content[path_start..];
        if !is_start_tag_named(path_tail, "path") {
            search_index = path_start + "<path".len();
            continue;
        }
        let Some(path_end) = find_simple_xml_tag_end(path_tail) else {
            break;
        };
        if !in_defs(path_start) {
            search_index = path_start + path_end + 1;
            continue;
        }
        let path_tag = &path_tail[..path_end];
        if let Some(path_data) = attr_value(path_tag, "d") {
            push_path_like_definition(
                &mut path_like_definitions,
                attr_value(path_tag, "id"),
                path_tag,
                path_start,
                path_data,
            );
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
        let Some(rect_end) = find_simple_xml_tag_end(rect_tail) else {
            break;
        };
        if !in_defs(rect_start) {
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
        let rx_raw = attr_value(rect_tag, "rx")
            .as_deref()
            .and_then(parse_number_prefix);
        let ry_raw = attr_value(rect_tag, "ry")
            .as_deref()
            .and_then(parse_number_prefix);
        if let (Some(width), Some(height)) = (
            attr_value(rect_tag, "width")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(rect_tag, "height")
                .as_deref()
                .and_then(parse_number_prefix),
        ) && width.is_finite()
            && height.is_finite()
            && width > 0.0
            && height > 0.0
        {
            let rounded_radii = match (rx_raw, ry_raw) {
                (Some(rx), Some(ry)) if rx > 0.0 && ry > 0.0 => {
                    Some((rx.min(width / 2.0), ry.min(height / 2.0)))
                }
                (Some(radius), None) | (None, Some(radius)) if radius > 0.0 => {
                    let radius = radius.min(width / 2.0).min(height / 2.0);
                    Some((radius, radius))
                }
                _ => None,
            };
            let path_data = if let Some((rx, ry)) = rounded_radii {
                let kappa = 0.552_284_8_f32;
                Some(format!(
                    "M {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} Z",
                    x + rx,
                    y,
                    x + width - rx,
                    y,
                    x + width - rx + kappa * rx,
                    y,
                    x + width,
                    y + ry - kappa * ry,
                    x + width,
                    y + ry,
                    x + width,
                    y + height - ry,
                    x + width,
                    y + height - ry + kappa * ry,
                    x + width - rx + kappa * rx,
                    y + height,
                    x + width - rx,
                    y + height,
                    x + rx,
                    y + height,
                    x + rx - kappa * rx,
                    y + height,
                    x,
                    y + height - ry + kappa * ry,
                    x,
                    y + height - ry,
                    x,
                    y + ry,
                    x,
                    y + ry - kappa * ry,
                    x + rx - kappa * rx,
                    y,
                    x + rx,
                    y
                ))
            } else {
                rect_path_data(x, y, width, height)
            };
            if let Some(path_data) = path_data {
                push_path_like_definition(
                    &mut path_like_definitions,
                    attr_value(rect_tag, "id"),
                    rect_tag,
                    rect_start,
                    path_data,
                );
            }
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
        let Some(circle_end) = find_simple_xml_tag_end(circle_tail) else {
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
        if let Some(radius) = attr_value(circle_tag, "r")
            .as_deref()
            .and_then(parse_number_prefix)
            && let Some(path_data) = ellipse_path_data(cx, cy, radius, radius)
        {
            push_path_like_definition(
                &mut path_like_definitions,
                attr_value(circle_tag, "id"),
                circle_tag,
                circle_start,
                path_data,
            );
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
        let Some(ellipse_end) = find_simple_xml_tag_end(ellipse_tail) else {
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
        if let (Some(rx), Some(ry)) = (
            attr_value(ellipse_tag, "rx")
                .as_deref()
                .and_then(parse_number_prefix),
            attr_value(ellipse_tag, "ry")
                .as_deref()
                .and_then(parse_number_prefix),
        ) && let Some(path_data) = ellipse_path_data(cx, cy, rx, ry)
        {
            push_path_like_definition(
                &mut path_like_definitions,
                attr_value(ellipse_tag, "id"),
                ellipse_tag,
                ellipse_start,
                path_data,
            );
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
        let Some(line_end) = find_simple_xml_tag_end(line_tail) else {
            break;
        };
        if !in_defs(line_start) {
            search_index = line_start + line_end + 1;
            continue;
        }
        let line_tag = &line_tail[..line_end];
        if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
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
        ) {
            push_path_like_definition(
                &mut path_like_definitions,
                attr_value(line_tag, "id"),
                line_tag,
                line_start,
                format!("M {} {} L {} {}", x1, y1, x2, y2),
            );
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
        let Some(poly_end) = find_simple_xml_tag_end(poly_tail) else {
            break;
        };
        if !in_defs(poly_start) {
            search_index = poly_start + poly_end + 1;
            continue;
        }
        let poly_tag = &poly_tail[..poly_end];
        if let Some(points_raw) = attr_value(poly_tag, "points")
            && let Some(points) = parse_raw_points(&points_raw)
            && let Some(path_data) = path_data_from_points(&points, false)
        {
            push_path_like_definition(
                &mut path_like_definitions,
                attr_value(poly_tag, "id"),
                poly_tag,
                poly_start,
                path_data,
            );
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
        let Some(poly_end) = find_simple_xml_tag_end(poly_tail) else {
            break;
        };
        if !in_defs(poly_start) {
            search_index = poly_start + poly_end + 1;
            continue;
        }
        let poly_tag = &poly_tail[..poly_end];
        if let Some(points_raw) = attr_value(poly_tag, "points")
            && let Some(points) = parse_raw_points(&points_raw)
            && let Some(path_data) = path_data_from_points(&points, true)
        {
            push_path_like_definition(
                &mut path_like_definitions,
                attr_value(poly_tag, "id"),
                poly_tag,
                poly_start,
                path_data,
            );
        }
        search_index = poly_start + poly_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<use") {
        let use_start = search_index + relative;
        let use_tail = &svg_content[use_start..];
        if !is_start_tag_named(use_tail, "use") {
            search_index = use_start + "<use".len();
            continue;
        }
        let Some(use_end) = find_simple_xml_tag_end(use_tail) else {
            break;
        };
        if !in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.trim().strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
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
            VectorTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        let referenced_definitions = path_like_definitions
            .iter()
            .filter(|definition| definition.id == reference_id)
            .cloned()
            .collect::<Vec<_>>();
        for definition in referenced_definitions {
            let mut alias_outer_transform = translated_use_transform;
            let mut alias_base_presentation = definition.alias_base_presentation;
            if let Some(symbol_definition) = symbol_definitions
                .iter()
                .rev()
                .find(|symbol_definition| symbol_definition.id == reference_id)
            {
                alias_base_presentation = Some(symbol_definition.presentation);
                if let Some((view_box_x, view_box_y, view_box_width, view_box_height)) =
                    symbol_definition.view_box
                {
                    let use_width = attr_value(use_tag, "width")
                        .as_deref()
                        .and_then(parse_x_length)
                        .unwrap_or(view_box_width);
                    let use_height = attr_value(use_tag, "height")
                        .as_deref()
                        .and_then(parse_y_length)
                        .unwrap_or(view_box_height);
                    if !use_width.is_finite()
                        || !use_height.is_finite()
                        || use_width <= 0.0
                        || use_height <= 0.0
                    {
                        continue;
                    }
                    let (scale_x, scale_y, offset_x, offset_y) = match symbol_definition
                        .preserve_aspect_ratio
                        .scale
                    {
                        VectorAspectScale::None => (
                            use_width / view_box_width,
                            use_height / view_box_height,
                            0.0,
                            0.0,
                        ),
                        VectorAspectScale::Meet | VectorAspectScale::Slice => {
                            let viewport_aspect = use_width / use_height;
                            let view_box_aspect = view_box_width / view_box_height;
                            let fit_width = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_width,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height * view_box_aspect
                                    } else {
                                        use_width
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width
                                    } else {
                                        use_height * view_box_aspect
                                    }
                                }
                            };
                            let fit_height = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_height,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height
                                    } else {
                                        use_width / view_box_aspect
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width / view_box_aspect
                                    } else {
                                        use_height
                                    }
                                }
                            };
                            if !fit_width.is_finite()
                                || !fit_height.is_finite()
                                || fit_width <= 0.0
                                || fit_height <= 0.0
                            {
                                continue;
                            }
                            let remaining_x = use_width - fit_width;
                            let remaining_y = use_height - fit_height;
                            let offset_x = match symbol_definition.preserve_aspect_ratio.x_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_x / 2.0,
                                VectorAspectAlign::Max => remaining_x,
                            };
                            let offset_y = match symbol_definition.preserve_aspect_ratio.y_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_y / 2.0,
                                VectorAspectAlign::Max => remaining_y,
                            };
                            (
                                fit_width / view_box_width,
                                fit_height / view_box_height,
                                offset_x,
                                offset_y,
                            )
                        }
                    };
                    let symbol_transform = VectorTransform {
                        a: scale_x,
                        d: scale_y,
                        e: offset_x - view_box_x * scale_x,
                        f: offset_y - view_box_y * scale_y,
                        stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
                        ..identity_transform
                    };
                    alias_outer_transform = compose_transform(
                        symbol_transform,
                        alias_outer_transform,
                        alias_outer_transform.stroke_scale,
                    );
                }
            }
            let alias_transform = definition
                .alias_transform
                .map(|base_transform| {
                    compose_transform(
                        base_transform,
                        alias_outer_transform,
                        alias_outer_transform.stroke_scale,
                    )
                })
                .unwrap_or(alias_outer_transform);
            let alias_presentation = definition
                .alias_presentation
                .map(|base_presentation| inherit_presentation(base_presentation, use_presentation))
                .unwrap_or(use_presentation);
            let mut alias_ids = Vec::new();
            if let Some(id) = attr_value(use_tag, "id").filter(|id| !id.trim().is_empty()) {
                alias_ids.push(id);
            }
            for group_definition in &group_definitions {
                if group_definition.content_start <= use_start
                    && use_start < group_definition.content_end
                    && !alias_ids.iter().any(|id| id == &group_definition.id)
                {
                    alias_ids.push(group_definition.id.clone());
                }
            }
            for symbol_definition in &symbol_definitions {
                if symbol_definition.content_start <= use_start
                    && use_start < symbol_definition.content_end
                    && !alias_ids.iter().any(|id| id == &symbol_definition.id)
                {
                    alias_ids.push(symbol_definition.id.clone());
                }
            }
            for id in alias_ids {
                path_like_definitions.push(VectorPathLikeDefinition {
                    id,
                    tag: definition.tag.clone(),
                    start: definition.start,
                    path_data: definition.path_data.clone(),
                    alias_base_presentation,
                    alias_transform: Some(alias_transform),
                    alias_presentation: Some(alias_presentation),
                });
            }
        }
        search_index = use_start + use_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<path") {
        let path_start = search_index + relative;
        let path_tail = &svg_content[path_start..];
        if !is_start_tag_named(path_tail, "path") {
            search_index = path_start + "<path".len();
            continue;
        }
        let Some(path_end) = find_simple_xml_tag_end(path_tail) else {
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
            push_simple_svg_path(&mut paths, path_tag, transform, presentation, &path_data);
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
        let Some(use_end) = find_simple_xml_tag_end(use_tail) else {
            break;
        };
        if in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.trim().strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        if !path_like_definitions
            .iter()
            .any(|definition| definition.id == reference_id)
        {
            search_index = use_start + use_end + 1;
            continue;
        }
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
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
            VectorTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        for definition in path_like_definitions
            .iter()
            .filter(|definition| definition.id == reference_id)
        {
            let Some((definition_transform, definition_presentation)) =
                parse_element_state(&definition.tag, definition.start)
            else {
                continue;
            };
            let mut definition_transform = definition_transform;
            let mut definition_presentation = definition_presentation;
            if let Some(alias_base_presentation) = definition.alias_base_presentation {
                definition_presentation =
                    inherit_presentation(alias_base_presentation, definition_presentation);
            }
            if let Some(alias_transform) = definition.alias_transform {
                definition_transform = compose_transform(
                    definition_transform,
                    alias_transform,
                    alias_transform.stroke_scale,
                );
            }
            if let Some(alias_presentation) = definition.alias_presentation {
                definition_presentation =
                    inherit_presentation(definition_presentation, alias_presentation);
            }
            let mut outer_transform = translated_use_transform;
            if let Some(symbol_definition) =
                symbol_definitions.iter().rev().find(|symbol_definition| {
                    symbol_definition.id == reference_id
                        && ((symbol_definition.content_start <= definition.start
                            && definition.start < symbol_definition.content_end)
                            || definition.id == symbol_definition.id)
                })
            {
                definition_presentation =
                    inherit_presentation(symbol_definition.presentation, definition_presentation);
                if let Some((view_box_x, view_box_y, view_box_width, view_box_height)) =
                    symbol_definition.view_box
                {
                    let use_width = attr_value(use_tag, "width")
                        .as_deref()
                        .and_then(parse_x_length)
                        .unwrap_or(view_box_width);
                    let use_height = attr_value(use_tag, "height")
                        .as_deref()
                        .and_then(parse_y_length)
                        .unwrap_or(view_box_height);
                    if !use_width.is_finite()
                        || !use_height.is_finite()
                        || use_width <= 0.0
                        || use_height <= 0.0
                    {
                        continue;
                    }
                    let (scale_x, scale_y, offset_x, offset_y) = match symbol_definition
                        .preserve_aspect_ratio
                        .scale
                    {
                        VectorAspectScale::None => (
                            use_width / view_box_width,
                            use_height / view_box_height,
                            0.0,
                            0.0,
                        ),
                        VectorAspectScale::Meet | VectorAspectScale::Slice => {
                            let viewport_aspect = use_width / use_height;
                            let view_box_aspect = view_box_width / view_box_height;
                            let fit_width = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_width,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height * view_box_aspect
                                    } else {
                                        use_width
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width
                                    } else {
                                        use_height * view_box_aspect
                                    }
                                }
                            };
                            let fit_height = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_height,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height
                                    } else {
                                        use_width / view_box_aspect
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width / view_box_aspect
                                    } else {
                                        use_height
                                    }
                                }
                            };
                            if !fit_width.is_finite()
                                || !fit_height.is_finite()
                                || fit_width <= 0.0
                                || fit_height <= 0.0
                            {
                                continue;
                            }
                            let remaining_x = use_width - fit_width;
                            let remaining_y = use_height - fit_height;
                            let offset_x = match symbol_definition.preserve_aspect_ratio.x_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_x / 2.0,
                                VectorAspectAlign::Max => remaining_x,
                            };
                            let offset_y = match symbol_definition.preserve_aspect_ratio.y_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_y / 2.0,
                                VectorAspectAlign::Max => remaining_y,
                            };
                            (
                                fit_width / view_box_width,
                                fit_height / view_box_height,
                                offset_x,
                                offset_y,
                            )
                        }
                    };
                    let symbol_transform = VectorTransform {
                        a: scale_x,
                        d: scale_y,
                        e: offset_x - view_box_x * scale_x,
                        f: offset_y - view_box_y * scale_y,
                        stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
                        ..identity_transform
                    };
                    outer_transform = compose_transform(
                        symbol_transform,
                        outer_transform,
                        outer_transform.stroke_scale,
                    );
                }
            }
            let transform = compose_transform(
                definition_transform,
                outer_transform,
                outer_transform.stroke_scale,
            );
            let presentation = inherit_presentation(definition_presentation, use_presentation);
            push_simple_svg_path(
                &mut paths,
                &definition.tag,
                transform,
                presentation,
                &definition.path_data,
            );
        }
        search_index = use_start + use_end + 1;
    }
    #[derive(Debug, Clone)]
    struct VectorImageDefinition {
        id: String,
        tag: String,
        start: usize,
        image: EmbeddedRasterImage,
        alias_transform: Option<VectorTransform>,
        alias_presentation: Option<VectorPresentation>,
    }
    let mut image_definitions = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<image") {
        let image_start = search_index + relative;
        let image_tail = &svg_content[image_start..];
        if !is_start_tag_named(image_tail, "image") {
            search_index = image_start + "<image".len();
            continue;
        }
        let Some(image_end) = find_simple_xml_tag_end(image_tail) else {
            break;
        };
        if !in_defs(image_start) {
            search_index = image_start + image_end + 1;
            continue;
        }
        let image_tag = &image_tail[..image_end];
        let mut definition_ids = Vec::new();
        if let Some(id) = attr_value(image_tag, "id").filter(|id| !id.trim().is_empty()) {
            definition_ids.push(id);
        }
        for group_definition in &group_definitions {
            if group_definition.content_start <= image_start
                && image_start < group_definition.content_end
                && !definition_ids.iter().any(|id| id == &group_definition.id)
            {
                definition_ids.push(group_definition.id.clone());
            }
        }
        for symbol_definition in &symbol_definitions {
            if symbol_definition.content_start <= image_start
                && image_start < symbol_definition.content_end
                && !definition_ids.iter().any(|id| id == &symbol_definition.id)
            {
                definition_ids.push(symbol_definition.id.clone());
            }
        }
        if definition_ids.is_empty() {
            search_index = image_start + image_end + 1;
            continue;
        }
        let Some(decoded_image) = attr_value(image_tag, "href")
            .or_else(|| attr_value(image_tag, "xlink:href"))
            .as_deref()
            .and_then(|href| decode_image_href(href))
        else {
            search_index = image_start + image_end + 1;
            continue;
        };
        for id in definition_ids {
            image_definitions.push(VectorImageDefinition {
                id,
                tag: image_tag.to_string(),
                start: image_start,
                image: decoded_image.clone(),
                alias_transform: None,
                alias_presentation: None,
            });
        }
        search_index = image_start + image_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<use") {
        let use_start = search_index + relative;
        let use_tail = &svg_content[use_start..];
        if !is_start_tag_named(use_tail, "use") {
            search_index = use_start + "<use".len();
            continue;
        }
        let Some(use_end) = find_simple_xml_tag_end(use_tail) else {
            break;
        };
        if !in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let mut alias_ids = Vec::new();
        if let Some(id) = attr_value(use_tag, "id").filter(|id| !id.trim().is_empty()) {
            alias_ids.push(id);
        }
        for group_definition in &group_definitions {
            if group_definition.content_start <= use_start
                && use_start < group_definition.content_end
                && !alias_ids.iter().any(|id| id == &group_definition.id)
            {
                alias_ids.push(group_definition.id.clone());
            }
        }
        for symbol_definition in &symbol_definitions {
            if symbol_definition.content_start <= use_start
                && use_start < symbol_definition.content_end
                && !alias_ids.iter().any(|id| id == &symbol_definition.id)
            {
                alias_ids.push(symbol_definition.id.clone());
            }
        }
        if alias_ids.is_empty() {
            search_index = use_start + use_end + 1;
            continue;
        }
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.trim().strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
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
            VectorTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        let referenced_definitions = image_definitions
            .iter()
            .filter(|definition| definition.id == reference_id)
            .cloned()
            .collect::<Vec<_>>();
        for definition in referenced_definitions {
            let mut alias_outer_transform = translated_use_transform;
            let mut alias_use_presentation = use_presentation;
            if let Some(symbol_definition) =
                symbol_definitions.iter().rev().find(|symbol_definition| {
                    symbol_definition.id == reference_id
                        && ((symbol_definition.content_start <= definition.start
                            && definition.start < symbol_definition.content_end)
                            || definition.id == symbol_definition.id)
                })
            {
                alias_use_presentation =
                    inherit_presentation(symbol_definition.presentation, alias_use_presentation);
                if let Some((view_box_x, view_box_y, view_box_width, view_box_height)) =
                    symbol_definition.view_box
                {
                    let use_width = attr_value(use_tag, "width")
                        .as_deref()
                        .and_then(parse_x_length)
                        .unwrap_or(view_box_width);
                    let use_height = attr_value(use_tag, "height")
                        .as_deref()
                        .and_then(parse_y_length)
                        .unwrap_or(view_box_height);
                    if !use_width.is_finite()
                        || !use_height.is_finite()
                        || use_width <= 0.0
                        || use_height <= 0.0
                    {
                        continue;
                    }
                    let (scale_x, scale_y, offset_x, offset_y) = match symbol_definition
                        .preserve_aspect_ratio
                        .scale
                    {
                        VectorAspectScale::None => (
                            use_width / view_box_width,
                            use_height / view_box_height,
                            0.0,
                            0.0,
                        ),
                        VectorAspectScale::Meet | VectorAspectScale::Slice => {
                            let viewport_aspect = use_width / use_height;
                            let view_box_aspect = view_box_width / view_box_height;
                            let fit_width = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_width,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height * view_box_aspect
                                    } else {
                                        use_width
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width
                                    } else {
                                        use_height * view_box_aspect
                                    }
                                }
                            };
                            let fit_height = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_height,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height
                                    } else {
                                        use_width / view_box_aspect
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width / view_box_aspect
                                    } else {
                                        use_height
                                    }
                                }
                            };
                            if !fit_width.is_finite()
                                || !fit_height.is_finite()
                                || fit_width <= 0.0
                                || fit_height <= 0.0
                            {
                                continue;
                            }
                            let remaining_x = use_width - fit_width;
                            let remaining_y = use_height - fit_height;
                            let offset_x = match symbol_definition.preserve_aspect_ratio.x_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_x / 2.0,
                                VectorAspectAlign::Max => remaining_x,
                            };
                            let offset_y = match symbol_definition.preserve_aspect_ratio.y_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_y / 2.0,
                                VectorAspectAlign::Max => remaining_y,
                            };
                            (
                                fit_width / view_box_width,
                                fit_height / view_box_height,
                                offset_x,
                                offset_y,
                            )
                        }
                    };
                    let symbol_transform = VectorTransform {
                        a: scale_x,
                        d: scale_y,
                        e: offset_x - view_box_x * scale_x,
                        f: offset_y - view_box_y * scale_y,
                        stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
                        ..identity_transform
                    };
                    alias_outer_transform = compose_transform(
                        symbol_transform,
                        alias_outer_transform,
                        alias_outer_transform.stroke_scale,
                    );
                }
            }
            let alias_transform = definition
                .alias_transform
                .map(|base_transform| {
                    compose_transform(
                        base_transform,
                        alias_outer_transform,
                        alias_outer_transform.stroke_scale,
                    )
                })
                .unwrap_or(alias_outer_transform);
            let alias_presentation = definition
                .alias_presentation
                .map(|base_presentation| {
                    inherit_presentation(base_presentation, alias_use_presentation)
                })
                .unwrap_or(alias_use_presentation);
            for id in &alias_ids {
                image_definitions.push(VectorImageDefinition {
                    id: id.clone(),
                    tag: definition.tag.clone(),
                    start: definition.start,
                    image: definition.image.clone(),
                    alias_transform: Some(alias_transform),
                    alias_presentation: Some(alias_presentation),
                });
            }
        }
        search_index = use_start + use_end + 1;
    }
    let mut embedded_images = Vec::new();
    let push_embedded_image = |embedded_images: &mut Vec<VectorEmbeddedImage>,
                               image_tag: &str,
                               transform: VectorTransform,
                               presentation: VectorPresentation,
                               decoded_image: EmbeddedRasterImage| {
        let x = attr_value(image_tag, "x")
            .as_deref()
            .and_then(parse_x_length)
            .unwrap_or(0.0);
        let y = attr_value(image_tag, "y")
            .as_deref()
            .and_then(parse_y_length)
            .unwrap_or(0.0);
        let Some(width) = attr_value(image_tag, "width")
            .as_deref()
            .and_then(parse_x_length)
        else {
            return;
        };
        let Some(height) = attr_value(image_tag, "height")
            .as_deref()
            .and_then(parse_y_length)
        else {
            return;
        };
        if width <= 0.0 || height <= 0.0 || !transform.axis_aligned {
            return;
        }
        let Some(corner_a) = apply_transform(transform, x, y) else {
            return;
        };
        let Some(corner_b) = apply_transform(transform, x + width, y + height) else {
            return;
        };
        let x = corner_a.0.min(corner_b.0);
        let y = corner_a.1.min(corner_b.1);
        let width = (corner_b.0 - corner_a.0).abs();
        let height = (corner_b.1 - corner_a.1).abs();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let opacity = presentation.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }
        embedded_images.push(VectorEmbeddedImage {
            x_ratio: (x - view_box.0) / view_box.2,
            y_ratio: (y - view_box.1) / view_box.3,
            width_ratio: width / view_box.2,
            height_ratio: height / view_box.3,
            image: decoded_image,
            preserve_aspect_ratio: attr_value(image_tag, "preserveAspectRatio")
                .as_deref()
                .and_then(|raw| parse_preserve_aspect_ratio(raw))
                .unwrap_or(default_preserve_aspect_ratio),
            opacity,
            clip_rect: clip_rect_for(presentation, transform),
        });
    };
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<image") {
        let image_start = search_index + relative;
        let image_tail = &svg_content[image_start..];
        if !is_start_tag_named(image_tail, "image") {
            search_index = image_start + "<image".len();
            continue;
        }
        let Some(image_end) = find_simple_xml_tag_end(image_tail) else {
            break;
        };
        if in_defs(image_start) {
            search_index = image_start + image_end + 1;
            continue;
        }
        let image_tag = &image_tail[..image_end];
        let Some(decoded_image) = attr_value(image_tag, "href")
            .or_else(|| attr_value(image_tag, "xlink:href"))
            .as_deref()
            .and_then(|href| decode_image_href(href))
        else {
            search_index = image_start + image_end + 1;
            continue;
        };
        let Some((transform, presentation)) = parse_element_state(image_tag, image_start) else {
            search_index = image_start + image_end + 1;
            continue;
        };
        push_embedded_image(
            &mut embedded_images,
            image_tag,
            transform,
            presentation,
            decoded_image,
        );
        search_index = image_start + image_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<use") {
        let use_start = search_index + relative;
        let use_tail = &svg_content[use_start..];
        if !is_start_tag_named(use_tail, "use") {
            search_index = use_start + "<use".len();
            continue;
        }
        let Some(use_end) = find_simple_xml_tag_end(use_tail) else {
            break;
        };
        if in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.trim().strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        if !image_definitions
            .iter()
            .any(|definition| definition.id == reference_id)
        {
            search_index = use_start + use_end + 1;
            continue;
        }
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
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
            VectorTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        for definition in image_definitions
            .iter()
            .filter(|definition| definition.id == reference_id)
        {
            let Some((definition_transform, definition_presentation)) =
                parse_element_state(&definition.tag, definition.start)
            else {
                continue;
            };
            let mut definition_transform = definition_transform;
            let mut definition_presentation = definition_presentation;
            if let Some(alias_transform) = definition.alias_transform {
                definition_transform = compose_transform(
                    definition_transform,
                    alias_transform,
                    alias_transform.stroke_scale,
                );
            }
            if let Some(alias_presentation) = definition.alias_presentation {
                definition_presentation =
                    inherit_presentation(definition_presentation, alias_presentation);
            }
            let mut outer_transform = translated_use_transform;
            if let Some(symbol_definition) =
                symbol_definitions.iter().rev().find(|symbol_definition| {
                    symbol_definition.id == reference_id
                        && ((symbol_definition.content_start <= definition.start
                            && definition.start < symbol_definition.content_end)
                            || definition.id == symbol_definition.id)
                })
            {
                definition_presentation =
                    inherit_presentation(symbol_definition.presentation, definition_presentation);
                if let Some((view_box_x, view_box_y, view_box_width, view_box_height)) =
                    symbol_definition.view_box
                {
                    let use_width = attr_value(use_tag, "width")
                        .as_deref()
                        .and_then(parse_x_length)
                        .unwrap_or(view_box_width);
                    let use_height = attr_value(use_tag, "height")
                        .as_deref()
                        .and_then(parse_y_length)
                        .unwrap_or(view_box_height);
                    if !use_width.is_finite()
                        || !use_height.is_finite()
                        || use_width <= 0.0
                        || use_height <= 0.0
                    {
                        continue;
                    }
                    let (scale_x, scale_y, offset_x, offset_y) = match symbol_definition
                        .preserve_aspect_ratio
                        .scale
                    {
                        VectorAspectScale::None => (
                            use_width / view_box_width,
                            use_height / view_box_height,
                            0.0,
                            0.0,
                        ),
                        VectorAspectScale::Meet | VectorAspectScale::Slice => {
                            let viewport_aspect = use_width / use_height;
                            let view_box_aspect = view_box_width / view_box_height;
                            let fit_width = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_width,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height * view_box_aspect
                                    } else {
                                        use_width
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width
                                    } else {
                                        use_height * view_box_aspect
                                    }
                                }
                            };
                            let fit_height = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_height,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height
                                    } else {
                                        use_width / view_box_aspect
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width / view_box_aspect
                                    } else {
                                        use_height
                                    }
                                }
                            };
                            if !fit_width.is_finite()
                                || !fit_height.is_finite()
                                || fit_width <= 0.0
                                || fit_height <= 0.0
                            {
                                continue;
                            }
                            let remaining_x = use_width - fit_width;
                            let remaining_y = use_height - fit_height;
                            let offset_x = match symbol_definition.preserve_aspect_ratio.x_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_x / 2.0,
                                VectorAspectAlign::Max => remaining_x,
                            };
                            let offset_y = match symbol_definition.preserve_aspect_ratio.y_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_y / 2.0,
                                VectorAspectAlign::Max => remaining_y,
                            };
                            (
                                fit_width / view_box_width,
                                fit_height / view_box_height,
                                offset_x,
                                offset_y,
                            )
                        }
                    };
                    let symbol_transform = VectorTransform {
                        a: scale_x,
                        d: scale_y,
                        e: offset_x - view_box_x * scale_x,
                        f: offset_y - view_box_y * scale_y,
                        stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
                        ..identity_transform
                    };
                    outer_transform = compose_transform(
                        symbol_transform,
                        outer_transform,
                        outer_transform.stroke_scale,
                    );
                }
            }
            let transform = compose_transform(
                definition_transform,
                outer_transform,
                outer_transform.stroke_scale,
            );
            let presentation = inherit_presentation(definition_presentation, use_presentation);
            push_embedded_image(
                &mut embedded_images,
                &definition.tag,
                transform,
                presentation,
                definition.image.clone(),
            );
        }
        search_index = use_start + use_end + 1;
    }
    let decode_xml_text = |raw: &str| {
        let mut decoded = String::new();
        let mut remaining = raw;
        while let Some(entity_start) = remaining.find('&') {
            decoded.push_str(&remaining[..entity_start]);
            let entity_tail = &remaining[entity_start + 1..];
            let Some(entity_end) = entity_tail.find(';') else {
                decoded.push_str(&remaining[entity_start..]);
                return decoded;
            };
            let entity = &entity_tail[..entity_end];
            let numeric_char = if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(decimal) = entity.strip_prefix('#') {
                decimal.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            };
            match (entity, numeric_char) {
                ("lt", _) => decoded.push('<'),
                ("gt", _) => decoded.push('>'),
                ("quot", _) => decoded.push('"'),
                ("apos", _) => decoded.push('\''),
                ("amp", _) => decoded.push('&'),
                (_, Some(ch)) => decoded.push(ch),
                _ => {
                    decoded.push('&');
                    decoded.push_str(entity);
                    decoded.push(';');
                }
            }
            remaining = &entity_tail[entity_end + 1..];
        }
        decoded.push_str(remaining);
        decoded
    };
    #[derive(Debug, Clone)]
    struct VectorTextDefinition {
        id: String,
        tag: String,
        start: usize,
        body: String,
        alias_base_presentation: Option<VectorPresentation>,
        alias_transform: Option<VectorTransform>,
        alias_presentation: Option<VectorPresentation>,
    }
    let mut text_definitions = Vec::new();
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<text") {
        let text_start = search_index + relative;
        let text_tail = &svg_content[text_start..];
        if !is_start_tag_named(text_tail, "text") {
            search_index = text_start + "<text".len();
            continue;
        }
        let Some(text_tag_end) = find_simple_xml_tag_end(text_tail) else {
            break;
        };
        if !in_defs(text_start) {
            search_index = text_start + text_tag_end + 1;
            continue;
        }
        let text_tag = &text_tail[..text_tag_end];
        let mut definition_ids = Vec::new();
        if let Some(id) = attr_value(text_tag, "id").filter(|id| !id.trim().is_empty()) {
            definition_ids.push(id);
        }
        for group_definition in &group_definitions {
            if group_definition.content_start <= text_start
                && text_start < group_definition.content_end
                && !definition_ids.iter().any(|id| id == &group_definition.id)
            {
                definition_ids.push(group_definition.id.clone());
            }
        }
        for symbol_definition in &symbol_definitions {
            if symbol_definition.content_start <= text_start
                && text_start < symbol_definition.content_end
                && !definition_ids.iter().any(|id| id == &symbol_definition.id)
            {
                definition_ids.push(symbol_definition.id.clone());
            }
        }
        if definition_ids.is_empty() {
            search_index = text_start + text_tag_end + 1;
            continue;
        }
        let text_body_start = text_start + text_tag_end + 1;
        let Some(text_body_end_relative) = svg_content[text_body_start..].find("</text>") else {
            search_index = text_body_start;
            continue;
        };
        let text_body_end = text_body_start + text_body_end_relative;
        let text_body = &svg_content[text_body_start..text_body_end];
        let preserve_text_space = attr_value(text_tag, "xml:space")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("preserve"));
        let text_body = if preserve_text_space {
            text_body
        } else {
            text_body.trim()
        };
        if !text_body.is_empty() {
            for id in definition_ids {
                text_definitions.push(VectorTextDefinition {
                    id,
                    tag: text_tag.to_string(),
                    start: text_start,
                    body: text_body.to_string(),
                    alias_base_presentation: None,
                    alias_transform: None,
                    alias_presentation: None,
                });
            }
        }
        search_index = text_body_end + "</text>".len();
    }
    let mut texts = Vec::new();
    let push_text = |texts: &mut Vec<VectorText>,
                     transform: VectorTransform,
                     point: (f32, f32),
                     font_size: f32,
                     presentation: VectorPresentation,
                     text: String| {
        if font_size.is_finite() && font_size > 0.0 {
            let matrix_scale = if transform.stroke_scale.is_finite() && transform.stroke_scale > 0.0
            {
                transform.stroke_scale
            } else {
                1.0
            };
            texts.push(VectorText {
                x_ratio: point.0,
                y_ratio: point.1,
                matrix_a: snap_transform_number(transform.a / matrix_scale),
                matrix_b: snap_transform_number(-transform.b / matrix_scale),
                matrix_c: snap_transform_number(-transform.c / matrix_scale),
                matrix_d: snap_transform_number(transform.d / matrix_scale),
                font_size_ratio: font_size / view_box.3,
                letter_spacing_ratio: presentation.letter_spacing.unwrap_or(0.0)
                    * transform.stroke_scale
                    / view_box.2,
                word_spacing_ratio: presentation.word_spacing.unwrap_or(0.0)
                    * transform.stroke_scale
                    / view_box.2,
                anchor: presentation.text_anchor.unwrap_or(VectorTextAnchor::Start),
                font_family: presentation.font_family.unwrap_or(VectorFontFamily::Serif),
                font_series: presentation.font_series.unwrap_or(FontSeries::Regular),
                font_shape: presentation.font_shape.unwrap_or(FontShape::Upright),
                fill: fill_paint(presentation, Some((0.0, 0.0, 0.0))),
                stroke: stroke_paint(presentation),
                stroke_width_ratio: transformed_stroke_width_ratio(presentation, transform),
                stroke_dasharray: transformed_stroke_dasharray_ratio(presentation, transform),
                stroke_style: stroke_style(presentation),
                paint_order: presentation.paint_order.unwrap_or(VectorPaintOrder::Normal),
                decoration: presentation.text_decoration.unwrap_or_default(),
                decoration_paint: text_decoration_paint(presentation),
                decoration_thickness_ratio: presentation
                    .text_decoration_thickness
                    .map(|thickness| thickness * transform.stroke_scale / view_box.2)
                    .filter(|thickness| thickness.is_finite() && *thickness > 0.0),
                decoration_style: presentation
                    .text_decoration_style
                    .unwrap_or(VectorTextDecorationStyle::Solid),
                clip_rect: clip_rect_for(presentation, transform),
                text,
            });
        }
    };
    let estimate_text_advance =
        |text: &str, font_size: f32, letter_spacing: f32, word_spacing: f32| {
            text.chars()
                .map(|ch| {
                    if ch.is_whitespace() || ch.is_ascii_punctuation() {
                        0.33
                    } else {
                        0.5
                    }
                })
                .sum::<f32>()
                * font_size
                + letter_spacing * text.chars().count().saturating_sub(1) as f32
                + word_spacing * text.chars().filter(|ch| *ch == ' ').count() as f32
        };
    let apply_text_length_spacing =
        |tag: &str, mut presentation: VectorPresentation, text: &str, font_size: f32| {
            let length_adjust =
                attr_value(tag, "lengthAdjust").unwrap_or_else(|| "spacing".to_string());
            let length_adjust = length_adjust.trim();
            if !(length_adjust.eq_ignore_ascii_case("spacing")
                || length_adjust.eq_ignore_ascii_case("spacingAndGlyphs"))
            {
                return presentation;
            }
            let Some(text_length) = attr_value(tag, "textLength")
                .as_deref()
                .and_then(parse_x_length)
                .filter(|value| value.is_finite())
            else {
                return presentation;
            };
            let spacing_gaps = text.chars().count().saturating_sub(1);
            if spacing_gaps == 0 {
                return presentation;
            }
            let current_advance = estimate_text_advance(
                text,
                font_size,
                presentation.letter_spacing.unwrap_or(0.0),
                presentation.word_spacing.unwrap_or(0.0),
            );
            let extra_spacing = (text_length - current_advance) / spacing_gaps as f32;
            if extra_spacing.is_finite() {
                presentation.letter_spacing =
                    Some(presentation.letter_spacing.unwrap_or(0.0) + extra_spacing);
            }
            presentation
        };
    let resolve_text_runs = |text_tag: &str,
                             text_body: &str,
                             transform: VectorTransform,
                             presentation: VectorPresentation|
     -> Vec<((f32, f32), f32, VectorPresentation, String)> {
        let preserve_text_space = attr_value(text_tag, "xml:space")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("preserve"));
        let text_body = if preserve_text_space {
            text_body
        } else {
            text_body.trim()
        };
        if text_body.is_empty() {
            return Vec::new();
        }
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
            return Vec::new();
        };
        if !text_body.contains('<') {
            let text = decode_xml_text(text_body);
            let presentation =
                apply_text_length_spacing(text_tag, presentation, &text, local_font_size);
            return vec![(point, font_size, presentation, text)];
        }
        let mut remaining = text_body;
        let mut current_x = text_x;
        let mut current_y = text_raw_y;
        let mut tspan_texts = Vec::new();
        while !remaining.is_empty() {
            let Some(tspan_start) = remaining.find("<tspan") else {
                if !remaining.is_empty() && (preserve_text_space || !remaining.trim().is_empty()) {
                    let literal_text = decode_xml_text(remaining);
                    let literal_baseline_y = current_y
                        + baseline_y_offset(presentation, local_font_size)
                        + baseline_shift_y_offset(presentation, local_font_size);
                    let Some(point) = apply_transform(transform, current_x, literal_baseline_y)
                        .map(normalize_point)
                    else {
                        return Vec::new();
                    };
                    tspan_texts.push((point, font_size, presentation, literal_text));
                }
                break;
            };
            if tspan_start > 0
                && (preserve_text_space || !remaining[..tspan_start].trim().is_empty())
            {
                let literal_text = decode_xml_text(&remaining[..tspan_start]);
                let literal_baseline_y = current_y
                    + baseline_y_offset(presentation, local_font_size)
                    + baseline_shift_y_offset(presentation, local_font_size);
                let Some(point) =
                    apply_transform(transform, current_x, literal_baseline_y).map(normalize_point)
                else {
                    return Vec::new();
                };
                current_x += estimate_text_advance(
                    &literal_text,
                    local_font_size,
                    presentation.letter_spacing.unwrap_or(0.0),
                    presentation.word_spacing.unwrap_or(0.0),
                );
                tspan_texts.push((point, font_size, presentation, literal_text));
            }
            let tspan_tail = &remaining[tspan_start..];
            if !is_start_tag_named(tspan_tail, "tspan") {
                return Vec::new();
            }
            let Some(tspan_tag_end) = find_simple_xml_tag_end(tspan_tail) else {
                return Vec::new();
            };
            let tspan_tag = &tspan_tail[..tspan_tag_end];
            let tspan_body_start = tspan_tag_end + 1;
            let Some(tspan_body_end_relative) = tspan_tail[tspan_body_start..].find("</tspan>")
            else {
                return Vec::new();
            };
            let tspan_body_end = tspan_body_start + tspan_body_end_relative;
            let tspan_body = &tspan_tail[tspan_body_start..tspan_body_end];
            let preserve_tspan_space = attr_value(tspan_tag, "xml:space")
                .map(|value| value.trim().eq_ignore_ascii_case("preserve"))
                .unwrap_or(preserve_text_space);
            if tspan_body.contains('<')
                || (tspan_body.is_empty()
                    || (!preserve_tspan_space && tspan_body.trim().is_empty()))
            {
                return Vec::new();
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
            if !presentation_is_visible(tspan_presentation) {
                current_x = tspan_x + tspan_dx;
                current_y = tspan_y + tspan_dy;
                let tspan_close_end = tspan_body_end + "</tspan>".len();
                remaining = &tspan_tail[tspan_close_end..];
                continue;
            }
            let tspan_text = decode_xml_text(tspan_body);
            let tspan_local_font_size = resolved_font_size(tspan_presentation);
            let tspan_presentation = apply_text_length_spacing(
                tspan_tag,
                tspan_presentation,
                &tspan_text,
                tspan_local_font_size,
            );
            let tspan_font_size = tspan_local_font_size * transform.stroke_scale;
            let tspan_x = tspan_x + tspan_dx;
            let tspan_y = tspan_y + tspan_dy;
            let tspan_baseline_y = tspan_y
                + baseline_y_offset(tspan_presentation, tspan_local_font_size)
                + baseline_shift_y_offset(tspan_presentation, tspan_local_font_size);
            let Some(point) =
                apply_transform(transform, tspan_x, tspan_baseline_y).map(normalize_point)
            else {
                return Vec::new();
            };
            current_x = tspan_x
                + estimate_text_advance(
                    &tspan_text,
                    tspan_local_font_size,
                    tspan_presentation.letter_spacing.unwrap_or(0.0),
                    tspan_presentation.word_spacing.unwrap_or(0.0),
                );
            tspan_texts.push((point, tspan_font_size, tspan_presentation, tspan_text));
            current_y = tspan_y;
            let tspan_close_end = tspan_body_end + "</tspan>".len();
            remaining = &tspan_tail[tspan_close_end..];
        }
        tspan_texts
    };
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<use") {
        let use_start = search_index + relative;
        let use_tail = &svg_content[use_start..];
        if !is_start_tag_named(use_tail, "use") {
            search_index = use_start + "<use".len();
            continue;
        }
        let Some(use_end) = find_simple_xml_tag_end(use_tail) else {
            break;
        };
        if !in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let mut alias_ids = Vec::new();
        if let Some(id) = attr_value(use_tag, "id").filter(|id| !id.trim().is_empty()) {
            alias_ids.push(id);
        }
        for group_definition in &group_definitions {
            if group_definition.content_start <= use_start
                && use_start < group_definition.content_end
                && !alias_ids.iter().any(|id| id == &group_definition.id)
            {
                alias_ids.push(group_definition.id.clone());
            }
        }
        for symbol_definition in &symbol_definitions {
            if symbol_definition.content_start <= use_start
                && use_start < symbol_definition.content_end
                && !alias_ids.iter().any(|id| id == &symbol_definition.id)
            {
                alias_ids.push(symbol_definition.id.clone());
            }
        }
        if alias_ids.is_empty() {
            search_index = use_start + use_end + 1;
            continue;
        }
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.trim().strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
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
            VectorTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        let referenced_definitions = text_definitions
            .iter()
            .filter(|definition| definition.id == reference_id)
            .cloned()
            .collect::<Vec<_>>();
        for definition in referenced_definitions {
            let mut alias_outer_transform = translated_use_transform;
            let mut alias_base_presentation = definition.alias_base_presentation;
            if let Some(symbol_definition) = symbol_definitions
                .iter()
                .rev()
                .find(|symbol_definition| symbol_definition.id == reference_id)
            {
                alias_base_presentation = Some(symbol_definition.presentation);
                if let Some((view_box_x, view_box_y, view_box_width, view_box_height)) =
                    symbol_definition.view_box
                {
                    let use_width = attr_value(use_tag, "width")
                        .as_deref()
                        .and_then(parse_x_length)
                        .unwrap_or(view_box_width);
                    let use_height = attr_value(use_tag, "height")
                        .as_deref()
                        .and_then(parse_y_length)
                        .unwrap_or(view_box_height);
                    if !use_width.is_finite()
                        || !use_height.is_finite()
                        || use_width <= 0.0
                        || use_height <= 0.0
                    {
                        continue;
                    }
                    let (scale_x, scale_y, offset_x, offset_y) = match symbol_definition
                        .preserve_aspect_ratio
                        .scale
                    {
                        VectorAspectScale::None => (
                            use_width / view_box_width,
                            use_height / view_box_height,
                            0.0,
                            0.0,
                        ),
                        VectorAspectScale::Meet | VectorAspectScale::Slice => {
                            let viewport_aspect = use_width / use_height;
                            let view_box_aspect = view_box_width / view_box_height;
                            let fit_width = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_width,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height * view_box_aspect
                                    } else {
                                        use_width
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width
                                    } else {
                                        use_height * view_box_aspect
                                    }
                                }
                            };
                            let fit_height = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_height,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height
                                    } else {
                                        use_width / view_box_aspect
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width / view_box_aspect
                                    } else {
                                        use_height
                                    }
                                }
                            };
                            if !fit_width.is_finite()
                                || !fit_height.is_finite()
                                || fit_width <= 0.0
                                || fit_height <= 0.0
                            {
                                continue;
                            }
                            let remaining_x = use_width - fit_width;
                            let remaining_y = use_height - fit_height;
                            let offset_x = match symbol_definition.preserve_aspect_ratio.x_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_x / 2.0,
                                VectorAspectAlign::Max => remaining_x,
                            };
                            let offset_y = match symbol_definition.preserve_aspect_ratio.y_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_y / 2.0,
                                VectorAspectAlign::Max => remaining_y,
                            };
                            (
                                fit_width / view_box_width,
                                fit_height / view_box_height,
                                offset_x,
                                offset_y,
                            )
                        }
                    };
                    let symbol_transform = VectorTransform {
                        a: scale_x,
                        d: scale_y,
                        e: offset_x - view_box_x * scale_x,
                        f: offset_y - view_box_y * scale_y,
                        stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
                        ..identity_transform
                    };
                    alias_outer_transform = compose_transform(
                        symbol_transform,
                        alias_outer_transform,
                        alias_outer_transform.stroke_scale,
                    );
                }
            }
            let alias_transform = definition
                .alias_transform
                .map(|base_transform| {
                    compose_transform(
                        base_transform,
                        alias_outer_transform,
                        alias_outer_transform.stroke_scale,
                    )
                })
                .unwrap_or(alias_outer_transform);
            let alias_presentation = definition
                .alias_presentation
                .map(|base_presentation| inherit_presentation(base_presentation, use_presentation))
                .unwrap_or(use_presentation);
            for id in &alias_ids {
                text_definitions.push(VectorTextDefinition {
                    id: id.clone(),
                    tag: definition.tag.clone(),
                    start: definition.start,
                    body: definition.body.clone(),
                    alias_base_presentation,
                    alias_transform: Some(alias_transform),
                    alias_presentation: Some(alias_presentation),
                });
            }
        }
        search_index = use_start + use_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<use") {
        let use_start = search_index + relative;
        let use_tail = &svg_content[use_start..];
        if !is_start_tag_named(use_tail, "use") {
            search_index = use_start + "<use".len();
            continue;
        }
        let Some(use_end) = find_simple_xml_tag_end(use_tail) else {
            break;
        };
        if in_defs(use_start) {
            search_index = use_start + use_end + 1;
            continue;
        }
        let use_tag = &use_tail[..use_end];
        let Some(reference_id) = attr_value(use_tag, "href")
            .or_else(|| attr_value(use_tag, "xlink:href"))
            .and_then(|href| href.trim().strip_prefix('#').map(str::to_string))
            .filter(|id| !id.is_empty())
        else {
            search_index = use_start + use_end + 1;
            continue;
        };
        let Some((use_transform, use_presentation)) = parse_element_state(use_tag, use_start)
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
            VectorTransform {
                e: x,
                f: y,
                ..identity_transform
            },
            use_transform,
            use_transform.stroke_scale,
        );
        for definition in text_definitions
            .iter()
            .filter(|definition| definition.id == reference_id)
        {
            let Some((definition_transform, definition_presentation)) =
                parse_element_state(&definition.tag, definition.start)
            else {
                continue;
            };
            let mut definition_transform = definition_transform;
            let mut definition_presentation = definition_presentation;
            if let Some(alias_base_presentation) = definition.alias_base_presentation {
                definition_presentation =
                    inherit_presentation(alias_base_presentation, definition_presentation);
            }
            if let Some(alias_transform) = definition.alias_transform {
                definition_transform = compose_transform(
                    definition_transform,
                    alias_transform,
                    alias_transform.stroke_scale,
                );
            }
            if let Some(alias_presentation) = definition.alias_presentation {
                definition_presentation =
                    inherit_presentation(definition_presentation, alias_presentation);
            }
            let mut outer_transform = translated_use_transform;
            if let Some(symbol_definition) =
                symbol_definitions.iter().rev().find(|symbol_definition| {
                    symbol_definition.id == reference_id
                        && ((symbol_definition.content_start <= definition.start
                            && definition.start < symbol_definition.content_end)
                            || definition.id == symbol_definition.id)
                })
            {
                definition_presentation =
                    inherit_presentation(symbol_definition.presentation, definition_presentation);
                if let Some((view_box_x, view_box_y, view_box_width, view_box_height)) =
                    symbol_definition.view_box
                {
                    let use_width = attr_value(use_tag, "width")
                        .as_deref()
                        .and_then(parse_x_length)
                        .unwrap_or(view_box_width);
                    let use_height = attr_value(use_tag, "height")
                        .as_deref()
                        .and_then(parse_y_length)
                        .unwrap_or(view_box_height);
                    if !use_width.is_finite()
                        || !use_height.is_finite()
                        || use_width <= 0.0
                        || use_height <= 0.0
                    {
                        continue;
                    }
                    let (scale_x, scale_y, offset_x, offset_y) = match symbol_definition
                        .preserve_aspect_ratio
                        .scale
                    {
                        VectorAspectScale::None => (
                            use_width / view_box_width,
                            use_height / view_box_height,
                            0.0,
                            0.0,
                        ),
                        VectorAspectScale::Meet | VectorAspectScale::Slice => {
                            let viewport_aspect = use_width / use_height;
                            let view_box_aspect = view_box_width / view_box_height;
                            let fit_width = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_width,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height * view_box_aspect
                                    } else {
                                        use_width
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width
                                    } else {
                                        use_height * view_box_aspect
                                    }
                                }
                            };
                            let fit_height = match symbol_definition.preserve_aspect_ratio.scale {
                                VectorAspectScale::None => use_height,
                                VectorAspectScale::Meet => {
                                    if viewport_aspect > view_box_aspect {
                                        use_height
                                    } else {
                                        use_width / view_box_aspect
                                    }
                                }
                                VectorAspectScale::Slice => {
                                    if viewport_aspect > view_box_aspect {
                                        use_width / view_box_aspect
                                    } else {
                                        use_height
                                    }
                                }
                            };
                            if !fit_width.is_finite()
                                || !fit_height.is_finite()
                                || fit_width <= 0.0
                                || fit_height <= 0.0
                            {
                                continue;
                            }
                            let remaining_x = use_width - fit_width;
                            let remaining_y = use_height - fit_height;
                            let offset_x = match symbol_definition.preserve_aspect_ratio.x_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_x / 2.0,
                                VectorAspectAlign::Max => remaining_x,
                            };
                            let offset_y = match symbol_definition.preserve_aspect_ratio.y_align {
                                VectorAspectAlign::Min => 0.0,
                                VectorAspectAlign::Mid => remaining_y / 2.0,
                                VectorAspectAlign::Max => remaining_y,
                            };
                            (
                                fit_width / view_box_width,
                                fit_height / view_box_height,
                                offset_x,
                                offset_y,
                            )
                        }
                    };
                    let symbol_transform = VectorTransform {
                        a: scale_x,
                        d: scale_y,
                        e: offset_x - view_box_x * scale_x,
                        f: offset_y - view_box_y * scale_y,
                        stroke_scale: (scale_x.abs() + scale_y.abs()) / 2.0,
                        ..identity_transform
                    };
                    outer_transform = compose_transform(
                        symbol_transform,
                        outer_transform,
                        outer_transform.stroke_scale,
                    );
                }
            }
            let transform = compose_transform(
                definition_transform,
                outer_transform,
                outer_transform.stroke_scale,
            );
            let presentation = inherit_presentation(definition_presentation, use_presentation);
            for (point, font_size, presentation, text) in
                resolve_text_runs(&definition.tag, &definition.body, transform, presentation)
            {
                push_text(&mut texts, transform, point, font_size, presentation, text);
            }
        }
        search_index = use_start + use_end + 1;
    }
    let mut search_index = 0usize;
    while let Some(relative) = svg_content[search_index..].find("<text") {
        let text_start = search_index + relative;
        let text_tail = &svg_content[text_start..];
        if !is_start_tag_named(text_tail, "text") {
            search_index = text_start + "<text".len();
            continue;
        }
        let Some(text_tag_end) = find_simple_xml_tag_end(text_tail) else {
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
        let text_body = &svg_content[text_body_start..text_body_end];
        let preserve_text_space = attr_value(text_tag, "xml:space")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("preserve"));
        let text_body = if preserve_text_space {
            text_body
        } else {
            text_body.trim()
        };
        if text_body.is_empty() {
            search_index = text_body_end + "</text>".len();
            continue;
        }
        let Some((transform, presentation)) = parse_element_state(text_tag, text_start) else {
            search_index = text_body_end + "</text>".len();
            continue;
        };
        for (point, font_size, presentation, text) in
            resolve_text_runs(text_tag, text_body, transform, presentation)
        {
            push_text(&mut texts, transform, point, font_size, presentation, text);
        }
        search_index = text_body_end + "</text>".len();
    }
    Some(VectorScene {
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
        embedded_images,
    })
}

#[cfg(test)]
mod tests {
    use image::ImageEncoder;
    use tex_render_model::{
        GraphicAssetFormat, GraphicAssetRequest, MaterializedGraphicAsset, VectorScene,
        from_pretty_json, to_pretty_json,
    };

    use super::{
        parse_svg, parse_svg_with_embedded_assets, prepare_svg_materialization,
        rewrite_svg_for_embedding, svg_embedded_asset_refs,
    };

    fn tiny_png_bytes() -> Vec<u8> {
        tiny_png_bytes_with_first_red(255)
    }

    fn tiny_png_bytes_with_first_red(first_red: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &[first_red, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
                2,
                2,
                image::ExtendedColorType::Rgb8,
            )
            .expect("encode png");
        bytes
    }

    #[test]
    fn parses_rect_and_text_into_vector_scene() {
        let scene = parse_svg(
            r##"<svg width="100" height="50" viewBox="0 0 100 50">
                <rect x="10" y="5" width="30" height="20" fill="#ff0000"/>
                <text x="25" y="35" font-size="10">hello</text>
            </svg>"##,
        )
        .expect("parse svg");

        assert_eq!(scene.rects.len(), 1);
        assert_eq!(scene.texts.len(), 1);
        assert_eq!(scene.texts[0].text, "hello");
        assert!((scene.rects[0].x_ratio - 0.1).abs() < f32::EPSILON);
        assert_eq!(scene.rects[0].fill.expect("rect fill").rgb, (1.0, 0.0, 0.0));

        let json = to_pretty_json(&scene).expect("serialize parsed vector scene");
        let decoded = from_pretty_json::<VectorScene>(&json).expect("deserialize vector scene");
        assert_eq!(decoded, scene);
        assert!(json.contains("\"text\": \"hello\""));
    }

    #[test]
    fn decodes_embedded_png_data_uri() {
        let mut data_uri = String::from("data:image/png,");
        for byte in tiny_png_bytes() {
            data_uri.push_str(&format!("%{byte:02X}"));
        }
        let svg = format!(
            r#"<svg width="20" height="20" viewBox="0 0 20 20"><image href="{data_uri}" x="2" y="3" width="10" height="12"/></svg>"#
        );

        let scene = parse_svg(&svg).expect("parse svg with data image");

        assert_eq!(scene.embedded_images.len(), 1);
        assert_eq!(scene.embedded_images[0].image.width, 2);
        assert_eq!(scene.embedded_images[0].image.height, 2);
        assert_eq!(scene.embedded_images[0].image.rgb.len(), 12);
    }

    #[test]
    fn resolves_relative_png_and_rewrites_it_for_embedding() {
        let svg = r#"<svg width="20" height="20" viewBox="0 0 20 20"><image href="pixel.png" width="20" height="20"/></svg>"#;
        let png = tiny_png_bytes();
        let mut requested = Vec::new();
        let scene = parse_svg_with_embedded_assets(svg, &mut |href| {
            requested.push(href.to_string());
            (href == "pixel.png").then(|| png.clone())
        })
        .expect("parse svg with resolved image");

        assert_eq!(requested, ["pixel.png"]);
        assert_eq!(scene.embedded_images.len(), 1);

        let rewritten = rewrite_svg_for_embedding(svg, "figures/vector.svg", |asset_ref| {
            (asset_ref == "figures/pixel.png").then(|| png.clone())
        });
        assert!(rewritten.contains("href=\"data:image/png,"));

        let sanitized = rewrite_svg_for_embedding(svg, "figures/vector.svg", |_| None);
        assert!(sanitized.contains("href=\"data:,\""));

        let request = GraphicAssetRequest {
            asset_ref: "figures/vector.svg".to_string(),
            source_format: Some(GraphicAssetFormat::Svg),
            page_selection: None,
            asset_hash: Some("blake3:vector".to_string()),
        };
        let materialized = MaterializedGraphicAsset::from_source(&request, svg.as_bytes().to_vec())
            .expect("materialize svg source");
        let prepared = prepare_svg_materialization(&request, materialized, |asset_ref| {
            (asset_ref == "figures/pixel.png").then(|| png.clone())
        });
        assert_eq!(
            prepared
                .vector_scene
                .as_ref()
                .map(|scene| scene.embedded_images.len()),
            Some(1)
        );
        assert!(
            prepared
                .embeddable_svg
                .as_deref()
                .is_some_and(|svg| svg.contains("href=\"data:image/png,"))
        );

        let changed_png = tiny_png_bytes_with_first_red(64);
        let changed_materialized =
            MaterializedGraphicAsset::from_source(&request, svg.as_bytes().to_vec())
                .expect("materialize changed svg source");
        let changed = prepare_svg_materialization(&request, changed_materialized, |asset_ref| {
            (asset_ref == "figures/pixel.png").then(|| changed_png.clone())
        });
        assert_ne!(changed.content_hash, prepared.content_hash);
    }

    #[test]
    fn collects_unique_normalized_external_svg_asset_refs() {
        let refs = svg_embedded_asset_refs(
            r##"<svg><image href="nested/pixel.png"/><image href="nested/../nested/pixel.png"/><image href="data:image/png,abc"/><use href="#shape"/><image href="https://example.test/remote.png"/></svg>"##,
            "figures/vector.svg",
        );

        assert_eq!(refs, vec!["figures/nested/pixel.png"]);
    }
}
