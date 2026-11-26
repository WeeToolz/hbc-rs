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
        let mut line_start = 0;
        let mut last_space_next = None;
        let mut offset = 0;
        while offset < paragraph.len() {
            let character = paragraph[offset..]
                .chars()
                .next()
                .expect("offset is before the end of the paragraph");
            let next_offset = offset + character.len_utf8();
            if character == '\r' {
                offset = next_offset;
                continue;
            }

            line.push(character);
            if character == ' ' {
                last_space_next = Some((next_offset, line.len()));
            }
            let width = measure(&line);
            if !width.is_finite() || width < 0.0 {
                return Err(crate::font::FontError::InvalidFont);
            }
            if width >= spec.width {
                if character == ' ' {
                    // Reborn ends this line after an ASCII space that reaches
                    // the edge.
                    lines.push(core::mem::take(&mut line));
                    offset = next_offset;
                    line_start = offset;
                    last_space_next = None;
                    continue;
                }
                if let Some((next, byte_end)) =
                    last_space_next.filter(|(next, _)| *next > line_start)
                {
                    // Reborn keeps the space and retries all later glyphs on
                    // the next line.
                    line.truncate(byte_end);
                    lines.push(core::mem::take(&mut line));
                    offset = next;
                    line_start = next;
                    last_space_next = None;
                    continue;
                }
                if line.chars().count() > 1 {
                    line.pop();
                    lines.push(core::mem::take(&mut line));
                    line_start = offset;
                    last_space_next = None;
                    continue;
                }
                // Keep one full-width glyph, as Reborn does when it cannot
                // break before the first glyph on a line.
                lines.push(core::mem::take(&mut line));
                offset = next_offset;
                line_start = offset;
                last_space_next = None;
                continue;
            }
            offset = next_offset;
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
        assert_eq!(memo.lines, ["WW ", "ii "]);
        assert_eq!(memo.max_scroll, 4);
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
    fn measured_wrap_moves_text_that_reaches_the_widget_edge() {
        let memo = super::layout_memo(
            "ab cd",
            super::MemoSpec {
                width: 10.0,
                height: 100.0,
                scroll: 0,
            },
            10.0,
            |text| text.chars().count() as f32 * 5.0,
        )
        .unwrap();

        assert_eq!(memo.lines, ["a", "b ", "c", "d"]);
        assert_eq!(memo.max_scroll, 0);

        // A single glyph that exactly fills the widget remains drawable.
        let glyph = super::layout_memo(
            "a",
            super::MemoSpec {
                width: 5.0,
                height: 10.0,
                scroll: 0,
            },
            10.0,
            |_| 5.0,
        )
        .unwrap();
        assert_eq!(glyph.lines, ["a"]);
    }

    #[test]
    fn measured_wrap_preserves_c_spaces_tabs_and_carriage_returns() {
        let spec = super::MemoSpec {
            width: 100.0,
            height: 100.0,
            scroll: 0,
        };
        let measure = |text: &str| text.chars().count() as f32;
        let whitespace = super::layout_memo("A  B\tC", spec, 10.0, measure).unwrap();
        assert_eq!(whitespace.lines, ["A  B\tC"]);

        let carriage_return = super::layout_memo("A\rB", spec, 10.0, measure).unwrap();
        assert_eq!(carriage_return.lines, ["AB"]);

        let wrapped = super::layout_memo(
            "ab  cd",
            super::MemoSpec { width: 5.0, ..spec },
            10.0,
            measure,
        )
        .unwrap();
        assert_eq!(wrapped.lines, ["ab  ", "cd"]);
    }

    #[test]
    fn memo_wrapping_keeps_paragraph_gaps() {
        assert_eq!(
            wrap_memo_lines("First paragraph\n\nSecond paragraph", 40),
            ["First paragraph", "", "Second paragraph",]
        );
    }

    #[test]
    fn measured_layout_rejects_invalid_bounds_and_keeps_wide_glyphs() {
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
        let wide = super::layout_memo(
            "W",
            super::MemoSpec {
                height: 10.0,
                ..spec
            },
            10.0,
            |_| 11.0,
        )
        .unwrap();
        assert_eq!(wide.lines, ["W"]);
        assert_eq!(wide.max_scroll, 1);
    }
}
