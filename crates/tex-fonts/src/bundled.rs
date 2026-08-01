use super::{FontData, FontResolver};

#[derive(Debug, Clone, Copy, Default)]
pub struct BundledFontResolver;

impl FontResolver for BundledFontResolver {
    fn resolve_tfm(&self, stem: &str) -> Option<FontData> {
        bundled_tfm(stem).map(FontData::borrowed)
    }

    fn resolve_type1(&self, stem: &str) -> Option<FontData> {
        bundled_type1(stem).map(FontData::borrowed)
    }
}

fn bundled_tfm(stem: &str) -> Option<&'static [u8]> {
    match stem {
        "cmr10" => Some(include_bytes!("../assets/classic/tfm/cmr10.tfm")),
        "cmr7" => Some(include_bytes!("../assets/classic/tfm/cmr7.tfm")),
        "cmr5" => Some(include_bytes!("../assets/classic/tfm/cmr5.tfm")),
        "cmmi10" => Some(include_bytes!("../assets/classic/tfm/cmmi10.tfm")),
        "cmmi7" => Some(include_bytes!("../assets/classic/tfm/cmmi7.tfm")),
        "cmmi5" => Some(include_bytes!("../assets/classic/tfm/cmmi5.tfm")),
        "cmsy10" => Some(include_bytes!("../assets/classic/tfm/cmsy10.tfm")),
        "cmsy7" => Some(include_bytes!("../assets/classic/tfm/cmsy7.tfm")),
        "cmsy5" => Some(include_bytes!("../assets/classic/tfm/cmsy5.tfm")),
        "cmex10" => Some(include_bytes!("../assets/classic/tfm/cmex10.tfm")),
        _ => None,
    }
}

fn bundled_type1(stem: &str) -> Option<&'static [u8]> {
    match stem {
        "cmr10" => Some(include_bytes!("../assets/classic/type1/cmr10.pfb")),
        "cmr7" => Some(include_bytes!("../assets/classic/type1/cmr7.pfb")),
        "cmr5" => Some(include_bytes!("../assets/classic/type1/cmr5.pfb")),
        "cmmi10" => Some(include_bytes!("../assets/classic/type1/cmmi10.pfb")),
        "cmmi7" => Some(include_bytes!("../assets/classic/type1/cmmi7.pfb")),
        "cmmi5" => Some(include_bytes!("../assets/classic/type1/cmmi5.pfb")),
        "cmsy10" => Some(include_bytes!("../assets/classic/type1/cmsy10.pfb")),
        "cmsy7" => Some(include_bytes!("../assets/classic/type1/cmsy7.pfb")),
        "cmsy5" => Some(include_bytes!("../assets/classic/type1/cmsy5.pfb")),
        "cmex10" => Some(include_bytes!("../assets/classic/type1/cmex10.pfb")),
        _ => None,
    }
}
