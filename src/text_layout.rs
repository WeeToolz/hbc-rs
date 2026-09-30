//! Shared text wrapping for layout and scroll bounds.

use alloc::{string::String, vec::Vec};

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
