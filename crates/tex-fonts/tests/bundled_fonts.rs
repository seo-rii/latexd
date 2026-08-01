use sha2::{Digest, Sha256};
use tex_fonts::{BundledFontResolver, FontResolver, TexFontFace, resolve_font_with};

#[test]
fn bundled_classic_faces_resolve_without_runtime_tex_discovery() {
    let resolver = BundledFontResolver;
    let faces = [
        TexFontFace::Roman10,
        TexFontFace::Roman7,
        TexFontFace::Roman5,
        TexFontFace::MathItalic10,
        TexFontFace::MathItalic7,
        TexFontFace::MathItalic5,
        TexFontFace::MathSymbol10,
        TexFontFace::MathSymbol7,
        TexFontFace::MathSymbol5,
        TexFontFace::MathExtension10,
    ];

    for face in faces {
        let font = resolve_font_with(face, &resolver)
            .unwrap_or_else(|| panic!("bundled {} should resolve", face.stem()));

        assert_eq!(font.face, face);
        assert!(font.content_hash.starts_with("blake3:"));
        assert_eq!(font.content_hash.len(), "blake3:".len() + 64);
        assert!(font.metrics.pdf_widths().iter().any(|width| *width > 0.0));
        assert!(font.type1.length1 > 0);
        assert!(font.type1.length2 > 0);
        assert!(
            font.type1
                .bytes
                .windows(face.postscript_name().len())
                .any(|window| window == face.postscript_name().as_bytes()),
            "{} Type 1 payload should retain its PostScript name",
            face.stem()
        );
    }
}

#[test]
fn bundled_files_match_the_audited_manifest() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../assets/classic/manifest.json"))
            .expect("classic font manifest should parse");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(
        manifest["bundle_id"],
        "amsfonts-classic-3.04+cm-tfm-2022-12-23"
    );
    let sources = manifest["sources"].as_array().expect("manifest sources");
    assert_eq!(sources[0]["license"], "OFL-1.1");
    assert_eq!(sources[1]["license"], "Knuth");

    let resolver = BundledFontResolver;
    let faces = manifest["faces"].as_array().expect("manifest faces");
    assert_eq!(faces.len(), 10);
    for face in faces {
        let stem = face["face_id"].as_str().expect("face id");
        let tfm = resolver.resolve_tfm(stem).expect("bundled TFM");
        let type1 = resolver.resolve_type1(stem).expect("bundled Type 1");

        assert_eq!(sha256(tfm.as_bytes()), face["tfm"]["sha256"]);
        assert_eq!(sha256(type1.as_bytes()), face["type1"]["sha256"]);
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
