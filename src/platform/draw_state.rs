//! Select the vertex format before a production draw submits its vertices.

use core::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DrawFormat {
    Solid,
    Textured,
}

#[derive(Default)]
pub(super) struct DrawState {
    format: Cell<Option<DrawFormat>>,
}

impl DrawState {
    /// Force configuration after initialization or an external GX state change.
    pub(super) fn configure(&self, format: DrawFormat, configure: impl FnOnce()) {
        configure();
        self.format.set(Some(format));
    }

    /// Configure when required, then return the actual draw result.
    pub(super) fn submit<R>(
        &self,
        format: DrawFormat,
        configure: impl FnOnce(),
        draw: impl FnOnce() -> R,
    ) -> R {
        if self.format.get() != Some(format) {
            self.configure(format, configure);
        }
        draw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn footer_texture_after_solid_draw_changes_format_before_submission() {
        let state = DrawState::default();
        let events = RefCell::new(Vec::new());
        // This explicit sink rejects a four-vertex payload if the selected
        // descriptor consumes a different number of bytes. Dolphin separately
        // checks the real GX command stream; this is a host state regression.
        let stride = Cell::new(0);
        let accepted_bytes = Cell::new(0);
        for (format, configuration, primitive, payload_bytes) in [
            (DrawFormat::Solid, "solid format", "solid quad", 64),
            (DrawFormat::Textured, "textured format", "gecko icon", 96),
            (DrawFormat::Textured, "textured format", "network icon", 96),
            (DrawFormat::Solid, "solid format", "fallback glyph", 64),
            (DrawFormat::Textured, "textured format", "font texture", 96),
        ] {
            let result = state.submit(
                format,
                || {
                    stride.set(match format {
                        DrawFormat::Solid => 16,
                        DrawFormat::Textured => 24,
                    });
                    events.borrow_mut().push(configuration);
                },
                || {
                    if stride.get() * 4 != payload_bytes {
                        return Err("vertex payload does not match the descriptor");
                    }
                    accepted_bytes.set(accepted_bytes.get() + payload_bytes);
                    events.borrow_mut().push(primitive);
                    Ok(())
                },
            );
            assert_eq!(result, Ok(()));
        }
        assert_eq!(accepted_bytes.get(), 416);
        assert_eq!(
            *events.borrow(),
            [
                "solid format",
                "solid quad",
                "textured format",
                "gecko icon",
                "network icon",
                "solid format",
                "fallback glyph",
                "textured format",
                "font texture",
            ]
        );
    }

    #[test]
    fn draw_failure_is_returned_and_does_not_repeat_valid_configuration() {
        let state = DrawState::default();
        let configurations = Cell::new(0);
        let failed: Result<(), &str> = state.submit(
            DrawFormat::Textured,
            || configurations.set(configurations.get() + 1),
            || Err("invalid texture"),
        );
        assert_eq!(failed, Err("invalid texture"));
        let next = state.submit(
            DrawFormat::Textured,
            || configurations.set(configurations.get() + 1),
            || Ok::<_, &str>(7),
        );
        assert_eq!(next, Ok(7));
        assert_eq!(configurations.get(), 1);
    }

    #[test]
    fn explicit_configuration_refreshes_the_same_format() {
        let state = DrawState::default();
        let configurations = Cell::new(0);
        for _ in 0..2 {
            state.configure(DrawFormat::Solid, || {
                configurations.set(configurations.get() + 1);
            });
        }
        assert_eq!(configurations.get(), 2);
    }
}
