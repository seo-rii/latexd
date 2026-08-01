use serde::{Deserialize, Serialize};

use crate::{DrawOp, GraphicAssetFormat, PageDisplayList, PageId};

pub const BROWSER_PAGES_SCHEMA_VERSION: u32 = 1;
pub const BROWSER_BUILD_METADATA_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserAssetManifestEntry {
    pub asset_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<GraphicAssetFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserPagesArtifact {
    pub schema_version: u32,
    pub revision: u64,
    pub pages: Vec<PageDisplayList>,
    pub changed_page_ids: Vec<PageId>,
    pub removed_page_ids: Vec<PageId>,
    pub assets: Vec<BrowserAssetManifestEntry>,
}

impl BrowserPagesArtifact {
    pub fn one_shot(revision: u64, pages: Vec<PageDisplayList>) -> Self {
        let changed_page_ids = pages.iter().map(|page| page.page_id.clone()).collect();
        let mut assets = Vec::new();
        for page in &pages {
            for op in &page.ops {
                let DrawOp::Image(image) = op else {
                    continue;
                };
                let entry = BrowserAssetManifestEntry {
                    asset_ref: image.asset_ref.clone(),
                    format: image.asset_format,
                    content_hash: image.asset_hash.clone(),
                };
                if !assets.contains(&entry) {
                    assets.push(entry);
                }
            }
        }
        Self {
            schema_version: BROWSER_PAGES_SCHEMA_VERSION,
            revision,
            pages,
            changed_page_ids,
            removed_page_ids: Vec::new(),
            assets,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserCompileMode {
    OneShot,
    Incremental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPageStats {
    pub total: u64,
    pub changed: u64,
    pub reused: u64,
    pub removed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserBuildMetadata {
    pub schema_version: u32,
    pub revision: u64,
    pub compile_mode: BrowserCompileMode,
    pub event_count: u64,
    pub diagnostic_count: u64,
    pub pages: BrowserPageStats,
}

impl BrowserBuildMetadata {
    pub fn one_shot(
        revision: u64,
        event_count: u64,
        diagnostic_count: u64,
        pages: &BrowserPagesArtifact,
    ) -> Self {
        Self {
            schema_version: BROWSER_BUILD_METADATA_SCHEMA_VERSION,
            revision,
            compile_mode: BrowserCompileMode::OneShot,
            event_count,
            diagnostic_count,
            pages: BrowserPageStats {
                total: pages.pages.len() as u64,
                changed: pages.changed_page_ids.len() as u64,
                reused: 0,
                removed: pages.removed_page_ids.len() as u64,
            },
        }
    }
}
