//! Shared text wrapping for layout and scroll bounds.

use alloc::{string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoSpec {
    pub width: f32,
    pub height: f32,
    pub scroll: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoLayout {
    pub lines: Vec<String>,
    pub max_scroll: usize,
    pub line_height: f32,
}

/// Wrap using the same font measurements as drawing, then select complete visible lines.
pub fn layout_memo(
    value: &str,
    spec: MemoSpec,
    line_height: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Result<MemoLayout, crate::font::FontError> {
    if !spec.width.is_finite()
        || spec.width <= 0.0
        || !spec.height.is_finite()
        || spec.height <= 0.0
        || !line_height.is_finite()
        || line_height <= 0.0
        || line_height > spec.height
    {
        return Err(crate::font::FontError::InvalidSize);
    }
    let mut lines = Vec::new();
    for paragraph in value.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                String::from(word)
            } else {
                alloc::format!("{line} {word}")
            };
            let width = measure(&candidate);
            if !width.is_finite() || width < 0.0 {
                return Err(crate::font::FontError::InvalidFont);
            }
            if width <= spec.width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(core::mem::take(&mut line));
            }
            for character in word.chars() {
                let glyph_width = measure(&String::from(character));
                if !glyph_width.is_finite() || glyph_width < 0.0 {
                    return Err(crate::font::FontError::InvalidFont);
                }
                if glyph_width > spec.width {
                    return Err(crate::font::FontError::GlyphTooLarge);
                }
                let mut candidate = line.clone();
                candidate.push(character);
                let width = measure(&candidate);
                if !width.is_finite() || width < 0.0 {
                    return Err(crate::font::FontError::InvalidFont);
                }
                if !line.is_empty() && width > spec.width {
                    lines.push(core::mem::take(&mut line));
                }
                line.push(character);
            }
        }
        lines.push(line);
    }
    let visible = (spec.height / line_height).max(1.0) as usize;
    let max_scroll = lines.len().saturating_sub(visible);
    let start = spec.scroll.min(max_scroll);
    let end = lines.len().min(start.saturating_add(visible));
    Ok(MemoLayout {
        lines: lines[start..end].to_vec(),
        max_scroll,
        line_height,
    })
}

/// Wrap words into lines no longer than `max_chars` characters.
pub fn wrap_lines(value: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        let mut chars = word.chars();
        while chars.clone().count() > max_chars {
            if !line.is_empty() {
                lines.push(core::mem::take(&mut line));
            }
            lines.push(chars.by_ref().take(max_chars).collect());
        }
        let remainder: String = chars.collect();
        if remainder.is_empty() {
            continue;
        }
        if !line.is_empty() && line.chars().count() + 1 + remainder.chars().count() > max_chars {
            lines.push(core::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&remainder);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Wrap text while preserving explicit paragraph breaks, including blank lines.
pub fn wrap_memo_lines(value: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for paragraph in value.split('\n') {
        if paragraph.trim().is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrap_lines(paragraph, max_chars));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::wrap_memo_lines;

    #[test]
    fn measured_wrap_uses_proportional_width_and_complete_visible_rows() {
        let measure = |s: &str| {
            s.chars()
                .map(|c| match c {
                    'W' => 10.0,
                    'i' => 2.0,
                    _ => 3.0,
                })
                .sum()
        };
        let spec = super::MemoSpec {
            width: 27.0,
            height: 20.0,
            scroll: 0,
        };
        let memo = super::layout_memo("WW ii WW\n\nWWWW", spec, 10.0, measure).unwrap();
        assert_eq!(memo.lines, ["WW ii", "WW"]);
        assert_eq!(memo.max_scroll, 3);
        let end = super::layout_memo(
            "WW ii WW\n\nWWWW",
            super::MemoSpec {
                scroll: 999,
                ..spec
            },
            10.0,
            measure,
        )
        .unwrap();
        assert_eq!(end.lines, ["WW", "WW"]);
    }

    #[test]
    fn memo_wrapping_keeps_paragraph_gaps() {
        assert_eq!(
            wrap_memo_lines("First paragraph\n\nSecond paragraph", 40),
            ["First paragraph", "", "Second paragraph",]
        );
    }

    #[test]
    fn measured_layout_rejects_invalid_bounds_and_oversized_glyphs() {
        let spec = super::MemoSpec {
            width: 10.0,
            height: 20.0,
            scroll: 0,
        };
        assert_eq!(
            super::layout_memo("W", spec, 0.0, |_| 10.0),
            Err(crate::font::FontError::InvalidSize)
        );
        assert_eq!(
            super::layout_memo("W", spec, 10.0, |_| f32::NAN),
            Err(crate::font::FontError::InvalidFont)
        );
        assert_eq!(
            super::layout_memo("W", spec, 10.0, |_| 11.0),
            Err(crate::font::FontError::GlyphTooLarge)
        );
    }
}
