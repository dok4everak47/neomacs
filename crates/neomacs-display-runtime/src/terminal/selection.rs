//! Pure mapping from presented pixels to terminal grid cells, plus the
//! minimal rio-vt selection constructors used by drag-select.
//!
//! The render thread owns the presented geometry, so it is the only layer
//! that can turn a pointer position into a `(row, col)` cell. This module
//! keeps that arithmetic and the selection construction free of any terminal
//! or render state so they are directly testable.

use rio_vt::crosswords::pos::{Column, Line, Pos, Side};
use rio_vt::selection::{Selection, SelectionType};

/// Resolve a point to a zero-based `(col, row)` cell inside a text body, or
/// `None` outside it.
///
/// The body's right and bottom edges are exclusive, so a point exactly on the
/// far edge names no cell. A degenerate cell size cannot produce a cell.
pub fn cell_at(
    body: crate::core::types::Rect,
    cell_w: f32,
    cell_h: f32,
    x: f32,
    y: f32,
) -> Option<(usize, usize)> {
    if cell_w <= 0.0 || cell_h <= 0.0 || cell_w.is_nan() || cell_h.is_nan() {
        return None;
    }
    if x < body.x || x >= body.x + body.width || y < body.y || y >= body.y + body.height {
        return None;
    }
    Some((
        ((x - body.x) / cell_w) as usize,
        ((y - body.y) / cell_h) as usize,
    ))
}

/// Clamp a body-relative point to the nearest cell, or `None` when there is
/// no grid to clamp into.
///
/// Unlike [`cell_at`] this never rejects an out-of-body point: a drag that
/// leaves the grid keeps extending to the nearest edge cell, which is what
/// xterm does. `cols`/`rows` are the grid size, so the result is always a
/// real cell and never past the last row/column.
pub fn clamp_cell_at(
    body: crate::core::types::Rect,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
    x: f32,
    y: f32,
) -> Option<(usize, usize)> {
    if cell_w <= 0.0
        || cell_h <= 0.0
        || cell_w.is_nan()
        || cell_h.is_nan()
        || cols == 0
        || rows == 0
    {
        return None;
    }
    let col = (((x - body.x) / cell_w).floor().max(0.0) as usize).min(cols - 1);
    let row = (((y - body.y) / cell_h).floor().max(0.0) as usize).min(rows - 1);
    Some((col, row))
}

/// Map a visible grid cell to a rio-vt selection position.
pub fn pos_of(row: usize, col: usize) -> Pos {
    Pos::new(Line(row as i32), Column(col))
}

/// Build a fresh simple selection anchored at a cell.
///
/// The dragging anchor is the left/top end of the selection, so it takes
/// [`Side::Left`]; the moving end takes [`Side::Right`] (see
/// `TerminalView::update_selection`). `Selection::range_simple` drops the
/// last cell of an end whose side is `Left`, so a same-side pair would trim
/// the cell under the pointer away from the selection.
pub fn simple_selection(row: usize, col: usize, side: Side) -> Selection {
    Selection::new(SelectionType::Simple, pos_of(row, col), side)
}
