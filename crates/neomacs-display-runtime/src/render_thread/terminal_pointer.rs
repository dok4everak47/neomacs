//! Pointer routing into Window-target neo-term terminals.
//!
//! A terminal child that enables mouse reporting (DEC 1000/1002/1003) expects
//! the pointer events that land on its grid. The terminal is painted as glyphs
//! the evaluator knows nothing about, so the render thread — which owns the
//! presented geometry and the PTY writer — is the only place that can resolve
//! a pixel to a cell.
//!
//! Reporting is *additive*: the event still reaches Emacs, so selecting the
//! window and the rest of the editor's pointer behaviour are unchanged. Only
//! the child gains the protocol bytes it asked for.

use super::RenderApp;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::terminal::{TerminalDisplayTarget, TerminalId, TerminalManager, mouse};

/// The modifiers a pointer event carries, already reduced to the three bits
/// the mouse protocol defines.
pub(super) fn mouse_modifiers(modifiers: u32) -> mouse::MouseModifiers {
    use crate::backend::wgpu::{
        NEOMACS_ALT_MASK, NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SHIFT_MASK,
    };
    let mut bits = 0u8;
    if modifiers & NEOMACS_SHIFT_MASK != 0 {
        bits |= mouse::MouseModifiers::SHIFT;
    }
    // xterm has a single "meta" modifier; both NS Option and Command map to it.
    if modifiers & (NEOMACS_ALT_MASK | NEOMACS_META_MASK) != 0 {
        bits |= mouse::MouseModifiers::META;
    }
    if modifiers & NEOMACS_CTRL_MASK != 0 {
        bits |= mouse::MouseModifiers::CTRL;
    }
    mouse::MouseModifiers::from_bits(bits)
}

/// Map a winit button onto the mouse protocol's button set.
pub(super) fn mouse_button(button: winit::event::MouseButton) -> Option<mouse::MouseButton> {
    use winit::event::MouseButton;
    Some(match button {
        MouseButton::Left => mouse::MouseButton::Left,
        MouseButton::Middle => mouse::MouseButton::Middle,
        MouseButton::Right => mouse::MouseButton::Right,
        MouseButton::Back => mouse::MouseButton::Back,
        MouseButton::Forward => mouse::MouseButton::Forward,
        _ => return None,
    })
}

/// Resolve a frame-space point to the Window-target terminal cell under it.
///
/// Only `Window` targets are hit: inline and floating terminals paint outside
/// any window's text body, so there is no buffer whose geometry could route a
/// cell. The first match in terminal-id order wins, which keeps two windows
/// showing the same buffer deterministic.
pub(super) fn terminal_cell_at(
    terminals: &TerminalManager,
    frame: &FrameGlyphBuffer,
    x: f32,
    y: f32,
) -> Option<(TerminalId, usize, usize)> {
    let cell_w = frame.char_width;
    let cell_h = frame.char_height;
    if cell_w <= 0.0 || cell_h <= 0.0 || cell_w.is_nan() || cell_h.is_nan() {
        return None;
    }
    for id in terminals.ids() {
        let Some(view) = terminals.get(id) else {
            continue;
        };
        let TerminalDisplayTarget::Window { buffer } = view.target else {
            continue;
        };
        for info in frame
            .window_infos
            .iter()
            .filter(|info| info.buffer_id == buffer.0 && !info.is_minibuffer)
        {
            let body = RenderApp::window_text_body(info);
            if let Some(cell) = crate::terminal::selection::cell_at(body, cell_w, cell_h, x, y) {
                return Some((id, cell.0, cell.1));
            }
        }
    }
    None
}

/// Whether a motion report repeats the cell already on the wire.
///
/// xterm re-sends a motion report only when the character cell changed
/// (`src/button.c:5591-5600`); press and release are discrete and always go
/// out.
fn motion_repeats_last_cell(
    action: mouse::MouseAction,
    cell: (TerminalId, usize, usize),
    last_cell: Option<(TerminalId, usize, usize)>,
) -> bool {
    action == mouse::MouseAction::Motion && Some(cell) == last_cell
}

/// Forward one pointer event to the terminal under the point, if any.
///
/// Returns the `(terminal, col, row)` the child actually received, so the
/// caller can record it and skip a later motion in the same cell. `last_cell`
/// is compared only for [`mouse::MouseAction::Motion`]: xterm emits a motion
/// report only when the character cell changed (`src/button.c:5591-5600`),
/// while press and release are discrete events that always go on the wire.
/// `None` means the child did not consume the event through a reporting mode.
///
/// Failures are logged, never surfaced: a broken PTY must not swallow the
/// editor's own pointer handling.
pub(super) fn report_terminal_mouse(
    terminals: &TerminalManager,
    frame: &FrameGlyphBuffer,
    x: f32,
    y: f32,
    action: mouse::MouseAction,
    button: Option<mouse::MouseButton>,
    modifiers: mouse::MouseModifiers,
    last_cell: Option<(TerminalId, usize, usize)>,
) -> Option<(TerminalId, usize, usize)> {
    let (id, col, row) = terminal_cell_at(terminals, frame, x, y)?;
    if motion_repeats_last_cell(action, (id, col, row), last_cell) {
        return None;
    }
    let view = terminals.get(id)?;
    match view.report_mouse(action, button, col, row, modifiers) {
        Ok(true) => Some((id, col, row)),
        Ok(false) => None,
        Err(error) => {
            tracing::warn!("Terminal {id} mouse report failed: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::wgpu::{
        NEOMACS_ALT_MASK, NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SHIFT_MASK,
    };

    #[test]
    fn mouse_modifiers_combine_shift_ctrl_and_meta() {
        let modifiers = mouse_modifiers(NEOMACS_SHIFT_MASK | NEOMACS_CTRL_MASK | NEOMACS_ALT_MASK);
        assert_eq!(
            modifiers,
            mouse::MouseModifiers::from_bits(
                mouse::MouseModifiers::SHIFT
                    | mouse::MouseModifiers::CTRL
                    | mouse::MouseModifiers::META
            )
        );
        // Command and Option both mean "meta" to xterm's mouse protocol.
        assert_eq!(
            mouse_modifiers(NEOMACS_META_MASK),
            mouse_modifiers(NEOMACS_ALT_MASK)
        );
    }

    #[test]
    fn mouse_button_maps_every_pointer_button() {
        use winit::event::MouseButton;
        assert_eq!(
            mouse_button(MouseButton::Left),
            Some(mouse::MouseButton::Left)
        );
        assert_eq!(
            mouse_button(MouseButton::Middle),
            Some(mouse::MouseButton::Middle)
        );
        assert_eq!(
            mouse_button(MouseButton::Right),
            Some(mouse::MouseButton::Right)
        );
        assert_eq!(
            mouse_button(MouseButton::Back),
            Some(mouse::MouseButton::Back)
        );
        assert_eq!(
            mouse_button(MouseButton::Forward),
            Some(mouse::MouseButton::Forward)
        );
        assert_eq!(mouse_button(MouseButton::Button6), None);
    }

    #[test]
    fn motion_repeats_only_the_same_cell_of_the_same_terminal() {
        let a = TerminalId::new(1).unwrap();
        let b = TerminalId::new(2).unwrap();
        // Same terminal, same cell: suppressed.
        assert!(motion_repeats_last_cell(
            mouse::MouseAction::Motion,
            (a, 3, 4),
            Some((a, 3, 4))
        ));
        // Same terminal, different cell: reported.
        assert!(!motion_repeats_last_cell(
            mouse::MouseAction::Motion,
            (a, 3, 5),
            Some((a, 3, 4))
        ));
        // Different terminal, same coordinates: reported (no aliasing).
        assert!(!motion_repeats_last_cell(
            mouse::MouseAction::Motion,
            (b, 3, 4),
            Some((a, 3, 4))
        ));
        // Press and release are never suppressed, even for the same cell.
        assert!(!motion_repeats_last_cell(
            mouse::MouseAction::Press,
            (a, 3, 4),
            Some((a, 3, 4))
        ));
        assert!(!motion_repeats_last_cell(
            mouse::MouseAction::Release,
            (a, 3, 4),
            Some((a, 3, 4))
        ));
        // No previous report: everything goes out.
        assert!(!motion_repeats_last_cell(
            mouse::MouseAction::Motion,
            (a, 0, 0),
            None
        ));
    }
}
