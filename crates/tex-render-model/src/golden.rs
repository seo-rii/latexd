use serde::{Serialize, de::DeserializeOwned};

pub fn to_pretty_json<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut json = serde_json::to_string_pretty(value)?;
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
        from_pretty_json, to_pretty_json,
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
}
