use serde::{Deserialize, Serialize};
use tex_render_gs::{PageRenderInput, Rect, Viewport};

const DEFAULT_TILE_SIZE: u32 = 256;
const MAX_TILE_SIZE: u32 = 4096;
const MAX_RASTER_DIMENSION: u32 = 32_768;
// Current backends may rasterize the whole page even for a single tile.
const MAX_RASTER_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_MANIFEST_TILES: u64 = 4096;

/// Coordinates use the top-left origin of the canonical page raster. The API's
/// scale already represents the requested pixel density; it has no separate DPR.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct RasterSpec {
    pub zoom_bucket: u16,
    pub tile_size: u32,
    pub width_px: u32,
    pub height_px: u32,
}

impl RasterSpec {
    pub fn from_page_scale(
        page: &PageRenderInput,
        scale: f32,
        tile_size: Option<u32>,
    ) -> Result<Self, &'static str> {
        Self::from_size_scale(page.size_pt(), scale, tile_size)
    }

    pub fn from_page_zoom_bucket(
        page: &PageRenderInput,
        zoom_bucket: u16,
        tile_size: Option<u32>,
    ) -> Result<Self, &'static str> {
        Self::from_size_zoom_bucket(page.size_pt(), zoom_bucket, tile_size)
    }

    #[cfg(test)]
    pub fn from_scale(
        width: u32,
        height: u32,
        scale: f32,
        tile_size: Option<u32>,
    ) -> Result<Self, &'static str> {
        Self::from_size_scale([f64::from(width), f64::from(height)], scale, tile_size)
    }

    pub fn from_size_scale(
        size_pt: [f64; 2],
        scale: f32,
        tile_size: Option<u32>,
    ) -> Result<Self, &'static str> {
        if !scale.is_finite() || !(0.1..=u16::MAX as f32 / 100.0).contains(&scale) {
            return Err("raster scale must be finite and between 0.1 and 655.35");
        }
        Self::from_size_zoom_bucket(size_pt, (scale * 100.0).round() as u16, tile_size)
    }

    fn from_size_zoom_bucket(
        [width, height]: [f64; 2],
        zoom_bucket: u16,
        tile_size: Option<u32>,
    ) -> Result<Self, &'static str> {
        if zoom_bucket < 10 {
            return Err("raster zoom bucket must be at least 10");
        }
        let tile_size = tile_size.unwrap_or(DEFAULT_TILE_SIZE);
        if !(1..=MAX_TILE_SIZE).contains(&tile_size) {
            return Err("raster tile size must be between 1 and 4096");
        }
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err("page dimensions must be positive and finite");
        }
        let scale = f64::from(zoom_bucket) / 100.0;
        let width_px = ((width * scale).round() as u32).max(1);
        let height_px = ((height * scale).round() as u32).max(1);
        if width_px > MAX_RASTER_DIMENSION
            || height_px > MAX_RASTER_DIMENSION
            || u64::from(width_px) * u64::from(height_px) > MAX_RASTER_PIXELS
        {
            return Err("page raster exceeds the dimension or pixel budget");
        }
        Ok(Self {
            zoom_bucket,
            tile_size,
            width_px,
            height_px,
        })
    }

    pub fn scale(self) -> f32 {
        self.zoom_bucket as f32 / 100.0
    }

    pub fn tile_rect(self, tile_x: u32, tile_y: u32) -> Option<Rect> {
        let x = tile_x.checked_mul(self.tile_size)?;
        let y = tile_y.checked_mul(self.tile_size)?;
        if x >= self.width_px || y >= self.height_px {
            return None;
        }
        Some(Rect {
            x,
            y,
            width: (self.width_px - x).min(self.tile_size),
            height: (self.height_px - y).min(self.tile_size),
        })
    }

    pub fn required_tiles(self, viewport: &Viewport) -> Result<Vec<(u32, u32)>, &'static str> {
        let left = viewport.left.min(self.width_px);
        let top = viewport.top.min(self.height_px);
        let right = viewport
            .left
            .saturating_add(viewport.width)
            .min(self.width_px);
        let bottom = viewport
            .top
            .saturating_add(viewport.height)
            .min(self.height_px);
        if right <= left || bottom <= top {
            return Ok(Vec::new());
        }
        let start_x = left / self.tile_size;
        let end_x = (right - 1) / self.tile_size;
        let start_y = top / self.tile_size;
        let end_y = (bottom - 1) / self.tile_size;
        let count = u64::from(end_x - start_x + 1) * u64::from(end_y - start_y + 1);
        if count > MAX_MANIFEST_TILES {
            return Err("viewport exceeds the tile manifest budget");
        }
        let mut tiles = Vec::with_capacity(count as usize);
        for tile_y in start_y..=end_y {
            for tile_x in start_x..=end_x {
                tiles.push((tile_x, tile_y));
            }
        }
        Ok(tiles)
    }
}
