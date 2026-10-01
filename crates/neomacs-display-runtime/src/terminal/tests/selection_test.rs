//! Unit tests for the pure pointer-to-cell mapping and the selection
//! constructors, plus an end-to-end `selected_text` check through a test
//! terminal.

use super::selection::{cell_at, pos_of, simple_selection};
use super::{TerminalDisplayTarget, TerminalGridSize, TerminalId, TerminalView};
use crate::core::types::Rect;
use rio_vt::crosswords::pos::{Column, Line, Pos, Side};
use rio_vt::selection::SelectionType;
use std::sync::Arc;

fn body() -> Rect {
    Rect::new(10.0, 20.0, 80.0, 40.0)
}

#[test]
fn cell_is_zero_based_from_the_text_body_origin() {
    assert_eq!(cell_at(body(), 8.0, 16.0, 10.0, 20.0), Some((0, 0)));
    assert_eq!(cell_at(body(), 8.0, 16.0, 17.9, 35.9), Some((0, 0)));
    assert_eq!(cell_at(body(), 8.0, 16.0, 18.0, 36.0), Some((1, 1)));
}

#[test]
fn cell_rejects_points_outside_the_body() {
    assert_eq!(cell_at(body(), 8.0, 16.0, 9.9, 20.0), None);
    assert_eq!(cell_at(body(), 8.0, 16.0, 10.0, 19.9), None);
    // The body's right/bottom edge is exclusive.
    assert_eq!(cell_at(body(), 8.0, 16.0, 90.0, 20.0), None);
    assert_eq!(cell_at(body(), 8.0, 16.0, 10.0, 60.0), None);
}

#[test]
fn cell_rejects_a_degenerate_cell() {
    assert_eq!(cell_at(body(), 0.0, 16.0, 10.0, 20.0), None);
    assert_eq!(cell_at(body(), 8.0, f32::NAN, 10.0, 20.0), None);
}

#[test]
fn pos_of_maps_rows_and_columns() {
    assert_eq!(pos_of(0, 0), Pos::new(Line(0), Column(0)));
    assert_eq!(pos_of(3, 7), Pos::new(Line(3), Column(7)));
}

#[test]
fn simple_selection_uses_the_requested_side() {
    let left = simple_selection(1, 2, Side::Left);
    assert_eq!(left.ty, SelectionType::Simple);
    let right = simple_selection(1, 2, Side::Right);
    assert_eq!(right.ty, SelectionType::Simple);
    // Different sides are distinct anchors even at the same point.
    assert_ne!(left, right);
}

#[test]
fn selected_text_extracts_the_dragged_range_and_clears() {
    let id = TerminalId::new(11).unwrap();
    let mut view = TerminalView::for_test(
        id,
        TerminalGridSize::new(20, 4).unwrap(),
        TerminalDisplayTarget::Window {
            buffer: neovm_core::buffer::BufferId(1),
        },
        Arc::new(parking_lot::Mutex::new(Vec::new())),
    );
    view.feed_for_test(b"hello");

    // A drag from the first cell rightwards must include the cell under the
    // pointer (anchor side `Left`, moving side `Right`).
    view.begin_selection(0, 0);
    view.update_selection(0, 2);
    assert_eq!(view.selected_text().as_deref(), Some("hel"));

    view.clear_selection();
    assert_eq!(view.selected_text(), None);
}
