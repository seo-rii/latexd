use serde::{Serialize, de::DeserializeOwned};

pub fn to_pretty_json<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    Ok(json)
}

pub fn to_semantic_pretty_json<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(value)?;
    let mut stack = vec![&mut value];
    while let Some(current) = stack.pop() {
        match current {
            serde_json::Value::Array(items) => {
                for item in items {
                    stack.push(item);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    if matches!(
                        key.as_str(),
                        "source"
                            | "source_spans"
                            | "source_hash"
                            | "title_source"
                            | "author_sources"
                            | "date_source"
                            | "caption_source"
                            | "full_source_artifact"
                    ) {
                        *child = serde_json::Value::String("<present>".to_string());
                    } else {
                        stack.push(child);
                    }
                }
            }
            _ => {}
        }
    }
    let mut json = serde_json::to_string_pretty(&value)?;
    json.push('\n');
    Ok(json)
}

pub fn from_pretty_json<T: DeserializeOwned>(json: &str) -> serde_json::Result<T> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use crate::{
        RenderEvent, RenderEventEnvelope, RenderEventStream, SourceProvenance, TextEvent,
        from_pretty_json, to_pretty_json, to_semantic_pretty_json,
    };

    #[test]
    fn pretty_json_helper_roundtrips_stream() {
        let stream = RenderEventStream::new(
            Some("text".to_string()),
            vec![RenderEventEnvelope::new(
                1,
                RenderEvent::Text(TextEvent {
                    text: "hello".to_string(),
                }),
                SourceProvenance::file("main.tex", 0, 5),
            )],
        );

        let encoded = to_pretty_json(&stream).expect("encode stream");
        let decoded: RenderEventStream = from_pretty_json(&encoded).expect("decode stream");

        assert_eq!(decoded, stream);
        assert!(encoded.ends_with('\n'));
    }

    #[test]
    fn semantic_json_helper_elides_noisy_source_fields() {
        let value = serde_json::json!({
            "kind": "display_math",
            "raw_source": "x^2",
            "source": {
                "primary": {
                    "kind": "file",
                    "path": "main.tex",
                    "start_utf8": 1,
                    "end_utf8": 4
                }
            },
            "source_hash": "blake3:abc",
            "content": [
                {
                    "kind": "text",
                    "title_source": {
                        "primary": {
                            "kind": "file",
                            "path": "main.tex",
                            "start_utf8": 10,
                            "end_utf8": 14
                        }
                    }
                }
            ]
        });

        let encoded = to_semantic_pretty_json(&value).expect("semantic json");

        assert!(encoded.contains("\"source\": \"<present>\""));
        assert!(encoded.contains("\"title_source\": \"<present>\""));
        assert!(encoded.contains("\"source_hash\": \"<present>\""));
        assert!(encoded.contains("\"raw_source\": \"x^2\""));
        assert!(!encoded.contains("start_utf8"));
    }
}
