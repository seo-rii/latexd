use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    pub chars_per_line: usize,
    pub lines_per_page: usize,
    pub page_width_pt: f32,
    pub page_height_pt: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub start_utf8: u32,
    pub end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    pub page_id: String,
    pub index: usize,
    pub lines: Vec<String>,
    pub content_hash: String,
    pub width_pt: f32,
    pub height_pt: f32,
    pub text_span: TextSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentLayout {
    pub pages: Vec<PageLayout>,
    pub options: LayoutOptions,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            chars_per_line: 72,
            lines_per_page: 48,
            page_width_pt: 612.0,
            page_height_pt: 792.0,
        }
    }
}

pub fn layout_text(text: &str, options: LayoutOptions) -> DocumentLayout {
    let mut lines = Vec::new();
    for raw_line in text.lines() {
        if raw_line.trim().is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let candidate_len = if current.is_empty() {
                word.len()
            } else {
                current.len() + 1 + word.len()
            };
            if candidate_len > options.chars_per_line && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    let mut pages = Vec::new();
    let mut text_offset = 0usize;
    let mut content_occurrences = BTreeMap::<String, usize>::new();
    for (page_index, chunk) in lines.chunks(options.lines_per_page).enumerate() {
        let joined = chunk.join("\n");
        let content_hash = blake3::hash(joined.as_bytes()).to_hex().to_string();
        let occurrence = content_occurrences.entry(content_hash.clone()).or_default();
        let page_id = blake3::hash(
            format!(
                "{}:{}:{}:{}",
                content_hash, options.page_width_pt, options.page_height_pt, *occurrence
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        *occurrence += 1;
        let page_span = TextSpan {
            start_utf8: text_offset as u32,
            end_utf8: (text_offset + joined.len()) as u32,
        };
        text_offset += joined.len();
        if page_index + 1 < lines.chunks(options.lines_per_page).len() {
            text_offset += 1;
        }
        pages.push(PageLayout {
            page_id,
            index: page_index,
            lines: chunk.to_vec(),
            content_hash,
            width_pt: options.page_width_pt,
            height_pt: options.page_height_pt,
            text_span: page_span,
        });
    }

    DocumentLayout { pages, options }
}

#[cfg(test)]
mod tests {
    use super::{LayoutOptions, layout_text};

    #[test]
    fn wraps_lines_by_character_budget() {
        let layout = layout_text(
            "alpha beta gamma delta",
            LayoutOptions {
                chars_per_line: 10,
                lines_per_page: 10,
                ..LayoutOptions::default()
            },
        );

        assert_eq!(
            layout.pages[0].lines,
            vec![
                "alpha beta".to_string(),
                "gamma".to_string(),
                "delta".to_string()
            ]
        );
    }

    #[test]
    fn paginates_after_fixed_number_of_lines() {
        let layout = layout_text(
            "a\nb\nc\nd\ne",
            LayoutOptions {
                chars_per_line: 10,
                lines_per_page: 2,
                ..LayoutOptions::default()
            },
        );

        assert_eq!(layout.pages.len(), 3);
        assert_eq!(
            layout.pages[0].lines,
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(layout.pages[2].lines, vec!["e".to_string()]);
        assert_eq!(layout.pages[0].text_span.start_utf8, 0);
        assert_eq!(layout.pages[0].text_span.end_utf8, 3);
        assert_eq!(layout.pages[1].text_span.start_utf8, 4);
    }

    #[test]
    fn page_ids_and_hashes_are_stable_for_same_content() {
        let left = layout_text("hello world", LayoutOptions::default());
        let right = layout_text("hello world", LayoutOptions::default());

        assert_eq!(left.pages[0].page_id, right.pages[0].page_id);
        assert_eq!(left.pages[0].content_hash, right.pages[0].content_hash);
    }

    #[test]
    fn page_id_changes_when_page_geometry_changes() {
        let narrow = layout_text(
            "hello world",
            LayoutOptions {
                page_width_pt: 500.0,
                ..LayoutOptions::default()
            },
        );
        let wide = layout_text(
            "hello world",
            LayoutOptions {
                page_width_pt: 700.0,
                ..LayoutOptions::default()
            },
        );

        assert_ne!(narrow.pages[0].page_id, wide.pages[0].page_id);
        assert_eq!(narrow.pages[0].content_hash, wide.pages[0].content_hash);
    }

    #[test]
    fn text_spans_are_contiguous_across_pages() {
        let layout = layout_text(
            "one\ntwo\nthree\nfour\nfive\nsix",
            LayoutOptions {
                chars_per_line: 10,
                lines_per_page: 2,
                ..LayoutOptions::default()
            },
        );

        assert_eq!(layout.pages.len(), 3);
        assert_eq!(layout.pages[0].text_span.start_utf8, 0);
        assert_eq!(layout.pages[0].text_span.end_utf8, 7);
        assert_eq!(layout.pages[1].text_span.start_utf8, 8);
        assert_eq!(layout.pages[1].text_span.end_utf8, 18);
        assert_eq!(layout.pages[2].text_span.start_utf8, 19);
        assert_eq!(layout.pages[2].text_span.end_utf8, 27);
    }

    #[test]
    fn unchanged_tail_page_ids_survive_inserted_page() {
        let options = LayoutOptions {
            chars_per_line: 80,
            lines_per_page: 2,
            ..LayoutOptions::default()
        };
        let original = layout_text("a0\na1\nb0\nb1\nc0\nc1", options.clone());
        let shifted = layout_text("a0\na1\nx0\nx1\nb0\nb1\nc0\nc1", options);

        assert_eq!(original.pages.len(), 3);
        assert_eq!(shifted.pages.len(), 4);
        assert_eq!(original.pages[1].page_id, shifted.pages[2].page_id);
        assert_eq!(original.pages[2].page_id, shifted.pages[3].page_id);
    }
}
