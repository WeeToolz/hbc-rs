//! Bounded custom TrueType rasterization and per-theme glyph caching.

use alloc::{string::String, vec::Vec};
use fontdue::{Font, FontSettings, Metrics};

pub const DEFAULT_REGULAR_FONT_KEY: &str = "@builtin:droid.ttf";
pub const DEFAULT_BOLD_FONT_KEY: &str = "@builtin:droidbold.ttf";

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphBitmap {
    pub character: char,
    pub pixel_size: u16,
    pub xmin: i32,
    pub ymin: i32,
    pub width: u16,
    pub height: u16,
    pub advance_width: f32,
    pub coverage: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontError {
    InvalidFont,
    InvalidSize,
    GlyphTooLarge,
    AllocationFailed,
}

struct CachedGlyph(GlyphBitmap);

/// A parsed font face. Rasterized glyphs stay cached for its lifetime.
pub struct ThemeFontFace {
    pub file: String,
    font: Font,
    cache: Vec<CachedGlyph>,
}

impl ThemeFontFace {
    pub fn parse(file: String, bytes: Vec<u8>) -> Result<Self, FontError> {
        let font =
            Font::from_bytes(bytes, FontSettings::default()).map_err(|_| FontError::InvalidFont)?;
        Ok(Self {
            file,
            font,
            cache: Vec::new(),
        })
    }

    pub fn rasterize(
        &mut self,
        character: char,
        pixel_size: u16,
    ) -> Result<&GlyphBitmap, FontError> {
        if pixel_size == 0 || pixel_size > 128 {
            return Err(FontError::InvalidSize);
        }
        if let Some(index) = self
            .cache
            .iter()
            .position(|item| item.0.character == character && item.0.pixel_size == pixel_size)
        {
            return Ok(&self.cache[index].0);
        }
        let (metrics, coverage) = self.font.rasterize(character, f32::from(pixel_size));
        let glyph = make_bitmap(character, pixel_size, metrics, coverage)?;
        self.cache
            .try_reserve(1)
            .map_err(|_| FontError::AllocationFailed)?;
        self.cache.push(CachedGlyph(glyph));
        Ok(&self.cache.last().expect("just inserted").0)
    }

    pub fn cached_glyph_count(&self) -> usize {
        self.cache.len()
    }
    pub fn horizontal_line_metrics(&self, pixel_size: u16) -> Option<(f32, f32, f32)> {
        self.font
            .horizontal_line_metrics(f32::from(pixel_size))
            .map(|m| (m.ascent, m.descent, m.line_gap))
    }

    pub fn horizontal_kern(&self, left: char, right: char, pixel_size: u16) -> Option<f32> {
        self.font
            .horizontal_kern(left, right, f32::from(pixel_size))
    }

    pub fn measure_width(&self, value: &str, pixel_size: u16) -> f32 {
        let mut cursor = 0.0;
        let mut right: f32 = 0.0;
        let mut previous = None;
        for character in value.chars() {
            if let Some(left) = previous {
                cursor += self
                    .horizontal_kern(left, character, pixel_size)
                    .unwrap_or(0.0);
            }
            let glyph = self.font.metrics(character, f32::from(pixel_size));
            right = right.max(cursor + glyph.xmin as f32 + glyph.width as f32);
            cursor += glyph.advance_width;
            previous = Some(character);
        }
        right.max(cursor)
    }

    pub fn layout_memo(
        &self,
        value: &str,
        spec: crate::text_layout::MemoSpec,
        pixel_size: u16,
        scale: f32,
    ) -> Result<crate::text_layout::MemoLayout, FontError> {
        if pixel_size == 0
            || pixel_size > 128
            || !scale.is_finite()
            || scale <= 0.0
            || !spec.width.is_finite()
            || spec.width <= 0.0
            || !spec.height.is_finite()
            || spec.height <= 0.0
        {
            return Err(FontError::InvalidSize);
        }
        let (ascent, descent, gap) = self
            .horizontal_line_metrics(pixel_size)
            .ok_or(FontError::InvalidFont)?;
        let line_height = libm::ceilf((ascent - descent + gap) * scale).max(1.0);
        crate::text_layout::layout_memo(value, spec, line_height, |line| {
            self.measure_width(line, pixel_size) * scale
        })
    }
}

fn make_bitmap(
    character: char,
    pixel_size: u16,
    metrics: Metrics,
    coverage: Vec<u8>,
) -> Result<GlyphBitmap, FontError> {
    let width = u16::try_from(metrics.width).map_err(|_| FontError::GlyphTooLarge)?;
    let height = u16::try_from(metrics.height).map_err(|_| FontError::GlyphTooLarge)?;
    let advance_width = metrics.advance_width.clamp(0.0, f32::from(u16::MAX));
    if coverage.len() != usize::from(width) * usize::from(height) {
        return Err(FontError::GlyphTooLarge);
    }
    Ok(GlyphBitmap {
        character,
        pixel_size,
        xmin: metrics.xmin,
        ymin: metrics.ymin,
        width,
        height,
        advance_width,
        coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_layout_fits_the_dialog_and_reaches_the_final_line() {
        let face = ThemeFontFace::parse("droid.ttf".into(), DROID_REGULAR.to_vec()).unwrap();
        assert!(face.measure_width("WWW", 16) > face.measure_width("iii", 16) * 2.0);
        let value = alloc::format!("{}\nTheme: Water", crate::app::ABOUT_CREDITS);
        for size in [16, 24] {
            let mut scroll = 0;
            loop {
                let memo = face
                    .layout_memo(
                        &value,
                        crate::text_layout::MemoSpec {
                            width: 456.0,
                            height: 180.0,
                            scroll,
                        },
                        size,
                        1.0,
                    )
                    .unwrap();
                assert!(memo.max_scroll > 0);
                assert!(memo.lines.len() as f32 * memo.line_height <= 180.0);
                for line in &memo.lines {
                    assert!(face.measure_width(line, size) <= 456.0);
                }
                if scroll == memo.max_scroll {
                    assert_eq!(memo.lines.last().map(String::as_str), Some("Theme: Water"));
                    break;
                }
                scroll = memo.max_scroll;
            }
        }
    }

    const DROID_REGULAR: &[u8] = include_bytes!("../assets/droid.ttf");

    #[test]
    fn true_type_metrics_raster_data_and_glyph_cache_are_stable() {
        let mut face = ThemeFontFace::parse("droid.ttf".into(), DROID_REGULAR.to_vec()).unwrap();
        let line = face.horizontal_line_metrics(18).unwrap();
        assert!(line.0 > 0.0);
        assert!(line.1 < 0.0);
        let glyph = face.rasterize('A', 18).unwrap().clone();
        assert!(glyph.width > 0 && glyph.height > 0);
        assert_eq!(
            glyph.coverage.len(),
            usize::from(glyph.width) * usize::from(glyph.height)
        );
        assert!(glyph.coverage.iter().any(|coverage| *coverage > 0));
        assert_eq!(face.cached_glyph_count(), 1);
        assert_eq!(face.rasterize('A', 18).unwrap(), &glyph);
        assert_eq!(face.cached_glyph_count(), 1);
        assert_ne!(
            face.rasterize('A', 22).unwrap().pixel_size,
            glyph.pixel_size
        );
        assert_eq!(face.cached_glyph_count(), 2);
        assert!(
            face.horizontal_kern('A', 'V', 18)
                .is_none_or(|adjustment| adjustment.is_finite())
        );
    }

    #[test]
    fn invalid_font_and_unsupported_size_return_errors() {
        assert_eq!(
            ThemeFontFace::parse("bad.ttf".into(), vec![0; 8]).err(),
            Some(FontError::InvalidFont)
        );
        let mut face = ThemeFontFace::parse("droid.ttf".into(), DROID_REGULAR.to_vec()).unwrap();
        assert_eq!(face.rasterize('A', 0).err(), Some(FontError::InvalidSize));
        assert_eq!(face.rasterize('A', 129).err(), Some(FontError::InvalidSize));
    }
}
