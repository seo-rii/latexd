//! Reusable decoded rasters for the full-page-and-crop backends. This cache is
//! owned by the renderer actor; it does not keep revision artifacts alive.
use std::{collections::BTreeMap, collections::VecDeque, sync::Arc};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tex_render_gs::{
    PageRenderInput, RasterImage, Rect, Renderer, TileImage, crop_tiles_from_full,
};

use crate::raster_spec::RasterSpec;

pub(super) const FULL_PAGE_CACHE_ENTRY_BUDGET: usize = 8;
pub(super) const FULL_PAGE_CACHE_BYTE_BUDGET: usize = 64 * 1024 * 1024;

/// Renderer mode is fixed for the owning actor. Tile size and source/revision
/// identity do not affect pixels in a full raster and are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PageRasterKey {
    paint_hash: String,
    width: u32,
    height: u32,
    zoom_bucket: u16,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct FullPageRasterMetrics {
    pub entries: usize,
    pub bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub oversized_bypasses: u64,
    /// Actual backend calls, including failed calls; excludes cached crops.
    pub renderer_invocations: u64,
    /// Successful full-page raster pixels, rather than returned tile pixels.
    pub pixels_rasterized: u64,
    pub tiles_sliced: u64,
}

pub(super) struct CachedPageRenderer<R> {
    renderer: R,
    entries: BTreeMap<PageRasterKey, Arc<RasterImage>>,
    order: VecDeque<PageRasterKey>,
    entry_budget: usize,
    byte_budget: usize,
    metrics: FullPageRasterMetrics,
}

impl<R: Renderer> CachedPageRenderer<R> {
    pub fn new(renderer: R, entry_budget: usize, byte_budget: usize) -> Self {
        Self {
            renderer,
            entries: BTreeMap::new(),
            order: VecDeque::new(),
            entry_budget,
            byte_budget,
            metrics: FullPageRasterMetrics::default(),
        }
    }

    pub fn metrics(&self) -> FullPageRasterMetrics {
        self.metrics.clone()
    }

    fn raster(&mut self, page: &PageRenderInput, scale: f32) -> Result<Arc<RasterImage>> {
        let spec =
            RasterSpec::from_page_scale(page, scale, None).map_err(|error| anyhow!(error))?;
        let key = PageRasterKey {
            paint_hash: page.content_hash.clone(),
            width: spec.width_px,
            height: spec.height_px,
            zoom_bucket: spec.zoom_bucket,
        };
        if let Some(image) = self.entries.get(&key).cloned() {
            self.metrics.hits += 1;
            self.order.retain(|candidate| candidate != &key);
            self.order.push_back(key);
            return Ok(image);
        }
        self.metrics.misses += 1;
        self.metrics.renderer_invocations += 1;
        let image = self.renderer.render_full_page(page, spec.scale())?;
        let pixel_count = u64::from(image.width) * u64::from(image.height);
        if pixel_count.checked_mul(4) != Some(image.rgba.len() as u64) {
            return Err(anyhow!("renderer returned an invalid RGBA page buffer"));
        }
        self.metrics.pixels_rasterized += pixel_count;
        let image = Arc::new(image);
        if image.rgba.len() > self.byte_budget || self.entry_budget == 0 {
            self.metrics.oversized_bypasses += 1;
            return Ok(image);
        }
        while self.entries.len() >= self.entry_budget
            || self.metrics.bytes > self.byte_budget - image.rgba.len()
        {
            let oldest = self.order.pop_front().expect("occupied raster cache");
            let removed = self.entries.remove(&oldest).expect("cached raster");
            self.metrics.bytes -= removed.rgba.len();
            self.metrics.evictions += 1;
        }
        self.metrics.bytes += image.rgba.len();
        self.entries.insert(key.clone(), image.clone());
        self.order.push_back(key);
        self.metrics.entries = self.entries.len();
        Ok(image)
    }

    pub fn render_full_page(&mut self, page: &PageRenderInput, scale: f32) -> Result<RasterImage> {
        Ok((*self.raster(page, scale)?).clone())
    }

    pub fn render_tiles(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> Result<Vec<TileImage>> {
        if rects.is_empty() {
            return Ok(Vec::new());
        }
        let spec =
            RasterSpec::from_page_scale(page, scale, None).map_err(|error| anyhow!(error))?;
        let full = self.raster(page, spec.scale())?;
        let tiles = crop_tiles_from_full(page, spec.scale(), rects, &full)?;
        self.metrics.tiles_sliced += tiles.len() as u64;
        Ok(tiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tex_render_gs::MockRenderer;

    fn page() -> PageRenderInput {
        PageRenderInput {
            page_id: "first".into(),
            revision: 1,
            content_hash: "paint-a".into(),
            width_px: 8,
            height_px: 8,
            natural_size_pt: None,
            pdf_path: "first.pdf".into(),
        }
    }

    fn rect(x: u32) -> Rect {
        Rect {
            x,
            y: 0,
            width: 4,
            height: 4,
        }
    }

    #[test]
    fn distinct_tile_misses_share_raster_across_revision_and_page_identity() {
        let mut cache = CachedPageRenderer::new(MockRenderer, 8, 4096);
        let page = page();
        let first = cache.render_tiles(&page, 1.234, &[rect(0)]).unwrap();
        let mut moved_page = page.clone();
        moved_page.revision = 2;
        moved_page.page_id = "moved".into();
        moved_page.pdf_path = "second.pdf".into();
        let second = cache.render_tiles(&moved_page, 1.23, &[rect(4)]).unwrap();
        let clean = MockRenderer.render_full_page(&moved_page, 1.23).unwrap();
        assert_eq!(
            second,
            crop_tiles_from_full(&moved_page, 1.23, &[rect(4)], &clean).unwrap()
        );
        assert_eq!(first[0].key.zoom_bucket, 123);
        assert_eq!(second[0].key.page_id, "moved");
        assert_eq!(cache.metrics.renderer_invocations, 1);
        assert_eq!(cache.metrics.pixels_rasterized, 100);
        assert_eq!(cache.metrics.hits, 1);
        assert_eq!(cache.metrics.tiles_sliced, 2);
    }

    #[test]
    fn fractional_geometry_cache_keys_use_actual_raster_dimensions() {
        let mut cache = CachedPageRenderer::new(MockRenderer, 8, 4096);
        let mut page = page();
        page.natural_size_pt = Some([8.2, 8.2]);
        let first = cache.render_full_page(&page, 2.0).unwrap();
        page.natural_size_pt = Some([8.4, 8.4]);
        let second = cache.render_full_page(&page, 2.0).unwrap();
        assert_eq!((first.width, first.height), (16, 16));
        assert_eq!((second.width, second.height), (17, 17));
        assert_eq!(cache.metrics.renderer_invocations, 2);
    }

    #[test]
    fn changed_paint_geometry_or_scale_rasterizes_again() {
        let mut cache = CachedPageRenderer::new(MockRenderer, 8, 4096);
        let mut page = page();
        cache.render_tiles(&page, 1.0, &[rect(0)]).unwrap();
        page.content_hash = "paint-b".into();
        cache.render_tiles(&page, 1.0, &[rect(0)]).unwrap();
        page.width_px = 9;
        cache.render_tiles(&page, 1.0, &[rect(0)]).unwrap();
        cache.render_tiles(&page, 1.01, &[rect(0)]).unwrap();
        assert_eq!(cache.metrics.renderer_invocations, 4);
    }

    #[test]
    fn cache_evicts_by_bytes_and_entry_count_with_lru_hits() {
        for (entries, bytes) in [(2, 4096), (8, 512)] {
            let mut cache = CachedPageRenderer::new(MockRenderer, entries, bytes);
            let a = page();
            let mut b = a.clone();
            b.content_hash = "b".into();
            let mut c = a.clone();
            c.content_hash = "c".into();
            for page in [&a, &b, &a, &c, &a] {
                cache.render_tiles(page, 1.0, &[rect(0)]).unwrap();
            }
            assert_eq!(cache.metrics.entries, 2);
            assert_eq!(cache.metrics.bytes, 512);
            assert_eq!(cache.metrics.evictions, 1);
            assert_eq!(cache.metrics.renderer_invocations, 3);
            cache.render_tiles(&b, 1.0, &[rect(0)]).unwrap();
            assert_eq!(cache.metrics.renderer_invocations, 4);
        }
    }

    #[test]
    fn oversized_pages_and_disabled_caches_do_not_evict_useful_rasters() {
        let mut cache = CachedPageRenderer::new(MockRenderer, 8, 256);
        let small = page();
        cache.render_tiles(&small, 1.0, &[rect(0)]).unwrap();
        let mut large = small.clone();
        large.width_px = 9;
        cache.render_tiles(&large, 1.0, &[rect(0)]).unwrap();
        cache.render_tiles(&small, 1.0, &[rect(0)]).unwrap();
        assert_eq!(cache.metrics.entries, 1);
        assert_eq!(cache.metrics.bytes, 256);
        assert_eq!(cache.metrics.oversized_bypasses, 1);
        assert_eq!(cache.metrics.hits, 1);
        let mut disabled = CachedPageRenderer::new(MockRenderer, 0, 4096);
        disabled.render_full_page(&small, 1.0).unwrap();
        assert_eq!(disabled.metrics.entries, 0);
    }

    #[test]
    fn invalid_scale_and_empty_batches_do_not_invoke_the_renderer() {
        let mut cache = CachedPageRenderer::new(MockRenderer, 8, 4096);
        assert!(cache.render_tiles(&page(), f32::NAN, &[rect(0)]).is_err());
        assert!(cache.render_tiles(&page(), 1.0, &[]).unwrap().is_empty());
        assert_eq!(cache.metrics.renderer_invocations, 0);
    }

    #[test]
    fn failed_raster_is_retried_and_never_enters_the_cache() {
        struct FailsOnce(bool);
        impl Renderer for FailsOnce {
            fn render_full_page(
                &mut self,
                page: &PageRenderInput,
                scale: f32,
            ) -> Result<RasterImage> {
                if !self.0 {
                    self.0 = true;
                    return Err(anyhow!("simulated renderer failure"));
                }
                MockRenderer.render_full_page(page, scale)
            }

            fn render_tiles(
                &mut self,
                _: &PageRenderInput,
                _: f32,
                _: &[Rect],
            ) -> Result<Vec<TileImage>> {
                unreachable!("the cache must count full-page work honestly")
            }
        }
        let mut cache = CachedPageRenderer::new(FailsOnce(false), 8, 4096);
        assert!(cache.render_tiles(&page(), 1.0, &[rect(0)]).is_err());
        assert_eq!(cache.metrics.entries, 0);
        assert_eq!(cache.metrics.pixels_rasterized, 0);
        cache.render_tiles(&page(), 1.0, &[rect(4)]).unwrap();
        assert_eq!(cache.metrics.renderer_invocations, 2);
        assert_eq!(cache.metrics.pixels_rasterized, 64);
        assert_eq!(cache.metrics.entries, 1);
    }
}
