use tex_render_model::{
    DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape, MathAtomKind,
    MathLargeOperator, MathNode, MathScriptPlacement, Point, PositionedTextRun, Rect,
    SourceProvenance,
};

use crate::font_metrics::{approximate_text_clusters, text_advance_pt};

pub(crate) struct MathLayoutBox {
    pub(crate) width_pt: f32,
    pub(crate) ascent_pt: f32,
    pub(crate) descent_pt: f32,
    pub(crate) ops: Vec<DrawOp>,
}

pub(crate) fn layout_math_node(
    node: &MathNode,
    size_pt: f32,
    source: &SourceProvenance,
    math_font: &FontRequest,
) -> MathLayoutBox {
    match node {
        MathNode::Atom { text, atom_kind } => {
            let mut font = math_font.clone();
            font.size_pt = size_pt;
            font.shape = if *atom_kind == MathAtomKind::Identifier {
                FontShape::Italic
            } else {
                FontShape::Upright
            };
            let width_pt = text_advance_pt(text, &font, size_pt);
            MathLayoutBox {
                width_pt,
                ascent_pt: size_pt * 0.75,
                descent_pt: size_pt * 0.2,
                ops: vec![DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 0.0, y: 0.0 },
                    text: text.clone(),
                    font,
                    size_pt,
                    approximate_advance_pt: width_pt,
                    glyphs: None,
                    clusters: approximate_text_clusters(text),
                    source: source.clone(),
                })],
            }
        }
        MathNode::LargeOperator { operator } => {
            let text = match operator {
                MathLargeOperator::Sum => "∑",
                MathLargeOperator::Product => "∏",
                MathLargeOperator::Integral => "∫",
            };
            let operator_size_pt = size_pt * 1.45;
            let font = FontRequest {
                family: FontFamilyRequest::Symbol,
                series: FontSeries::Regular,
                shape: FontShape::Upright,
                size_pt: operator_size_pt,
                role: FontRole::Math,
            };
            let width_pt = text_advance_pt(text, &font, operator_size_pt);
            MathLayoutBox {
                width_pt,
                ascent_pt: operator_size_pt * 0.75,
                descent_pt: operator_size_pt * 0.2,
                ops: vec![DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 0.0, y: 0.0 },
                    text: text.to_string(),
                    font,
                    size_pt: operator_size_pt,
                    approximate_advance_pt: width_pt,
                    glyphs: None,
                    clusters: approximate_text_clusters(text),
                    source: source.clone(),
                })],
            }
        }
        MathNode::Row { children } => {
            let mut width_pt = 0.0_f32;
            let mut ascent_pt = 0.0_f32;
            let mut descent_pt = 0.0_f32;
            let mut ops = Vec::new();
            for (index, child) in children.iter().enumerate() {
                let spacing_pt = match child {
                    MathNode::Atom {
                        atom_kind: MathAtomKind::Relation,
                        ..
                    } => size_pt * 0.28,
                    MathNode::Atom {
                        atom_kind: MathAtomKind::Operator,
                        ..
                    } => size_pt * 0.2,
                    _ => 0.0,
                };
                if index > 0 {
                    width_pt += spacing_pt;
                }
                let mut child_box = layout_math_node(child, size_pt, source, math_font);
                for op in &mut child_box.ops {
                    op.translate(width_pt, 0.0);
                }
                width_pt += child_box.width_pt;
                if index + 1 < children.len() {
                    width_pt += spacing_pt;
                }
                ascent_pt = ascent_pt.max(child_box.ascent_pt);
                descent_pt = descent_pt.max(child_box.descent_pt);
                ops.extend(child_box.ops);
            }
            MathLayoutBox {
                width_pt,
                ascent_pt,
                descent_pt,
                ops,
            }
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => {
            let mut numerator = layout_math_node(numerator, size_pt, source, math_font);
            let mut denominator = layout_math_node(denominator, size_pt, source, math_font);
            let width_pt = numerator.width_pt.max(denominator.width_pt) + size_pt * 0.1;
            let numerator_x = (width_pt - numerator.width_pt) / 2.0;
            let denominator_x = (width_pt - denominator.width_pt) / 2.0;
            let rule_axis_y = -size_pt * 0.25;
            let rule_gap_pt = size_pt * 0.2;
            let numerator_y = rule_axis_y - rule_gap_pt - numerator.descent_pt;
            let denominator_y = rule_axis_y + rule_gap_pt + denominator.ascent_pt;
            for op in &mut numerator.ops {
                op.translate(numerator_x, numerator_y);
            }
            for op in &mut denominator.ops {
                op.translate(denominator_x, denominator_y);
            }
            let rule_height_pt = (size_pt * 0.05).max(0.4);
            let mut ops = numerator.ops;
            ops.push(DrawOp::Rule(Rect {
                x: 0.0,
                y: rule_axis_y - rule_height_pt / 2.0,
                width: width_pt,
                height: rule_height_pt,
            }));
            ops.extend(denominator.ops);
            MathLayoutBox {
                width_pt,
                ascent_pt: -numerator_y + numerator.ascent_pt,
                descent_pt: denominator_y + denominator.descent_pt,
                ops,
            }
        }
        MathNode::Scripts {
            base,
            subscript,
            superscript,
            placement,
        } => {
            let mut base = layout_math_node(base, size_pt, source, math_font);
            let script_size_pt = size_pt * 0.7;
            let mut subscript = subscript
                .as_deref()
                .map(|node| layout_math_node(node, script_size_pt, source, math_font));
            let mut superscript = superscript
                .as_deref()
                .map(|node| layout_math_node(node, script_size_pt, source, math_font));
            if *placement == MathScriptPlacement::Limits {
                let width_pt = subscript
                    .as_ref()
                    .map(|layout| layout.width_pt)
                    .into_iter()
                    .chain(superscript.as_ref().map(|layout| layout.width_pt))
                    .fold(base.width_pt, f32::max);
                let base_x = (width_pt - base.width_pt) / 2.0;
                for op in &mut base.ops {
                    op.translate(base_x, 0.0);
                }
                let mut ascent_pt = base.ascent_pt;
                let mut descent_pt = base.descent_pt;
                if let Some(layout) = &mut superscript {
                    let baseline_y = -(base.ascent_pt + layout.descent_pt + size_pt * 0.1);
                    let x = (width_pt - layout.width_pt) / 2.0;
                    for op in &mut layout.ops {
                        op.translate(x, baseline_y);
                    }
                    ascent_pt = ascent_pt.max(-baseline_y + layout.ascent_pt);
                }
                if let Some(layout) = &mut subscript {
                    let baseline_y = base.descent_pt + layout.ascent_pt + size_pt * 0.1;
                    let x = (width_pt - layout.width_pt) / 2.0;
                    for op in &mut layout.ops {
                        op.translate(x, baseline_y);
                    }
                    descent_pt = descent_pt.max(baseline_y + layout.descent_pt);
                }
                let mut ops = base.ops;
                if let Some(layout) = superscript {
                    ops.extend(layout.ops);
                }
                if let Some(layout) = subscript {
                    ops.extend(layout.ops);
                }
                MathLayoutBox {
                    width_pt,
                    ascent_pt,
                    descent_pt,
                    ops,
                }
            } else {
                let script_width_pt = subscript
                    .as_ref()
                    .map(|layout| layout.width_pt)
                    .into_iter()
                    .chain(superscript.as_ref().map(|layout| layout.width_pt))
                    .fold(0.0_f32, f32::max);
                let script_x = base.width_pt + size_pt * 0.05;
                let mut ascent_pt = base.ascent_pt;
                let mut descent_pt = base.descent_pt;
                if let Some(layout) = &mut superscript {
                    let baseline_y = -size_pt * 0.55;
                    for op in &mut layout.ops {
                        op.translate(script_x, baseline_y);
                    }
                    ascent_pt = ascent_pt.max(-baseline_y + layout.ascent_pt);
                }
                if let Some(layout) = &mut subscript {
                    let baseline_y = size_pt * 0.35;
                    for op in &mut layout.ops {
                        op.translate(script_x, baseline_y);
                    }
                    descent_pt = descent_pt.max(baseline_y + layout.descent_pt);
                }
                let mut ops = base.ops;
                if let Some(layout) = superscript {
                    ops.extend(layout.ops);
                }
                if let Some(layout) = subscript {
                    ops.extend(layout.ops);
                }
                MathLayoutBox {
                    width_pt: base.width_pt + size_pt * 0.05 + script_width_pt,
                    ascent_pt,
                    descent_pt,
                    ops,
                }
            }
        }
    }
}
