//! Locks the dispatch order of a primary `WindowEvent::PointerButton`.
//!
//! The ordering the terminal protocol needs is that a release clears the held
//! button before the move at the release position is processed. The
//! render-thread state collapses to the same value either way after a release,
//! so the only observable is the bytes that reach the child PTY — this test
//! drives the real parser and encoder and reads that wire.

#![cfg(feature = "neo-term")]

use crate::backend::wgpu::NEOMACS_SHIFT_MASK;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::render_thread::ImageRenderState;
use crate::render_thread::RenderApp;
use crate::render_thread::frame_windows::FrameLifecycle;
use crate::terminal::{TerminalDisplayTarget, TerminalGridSize, TerminalId, TerminalView, mouse};
use crate::thread_comm::ThreadComms;
use neomacs_display_protocol::presentation_origin::BufferModiff;
use neomacs_display_protocol::{DeviceScale, DisplayWindowId, SurfaceState};
use std::sync::{Arc, Mutex};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton};
use winit::window::WindowId;

/// Build a minimal `RenderApp`, mirroring the builder the input tests use.
fn make_test_app_with_comms(
    width: u32,
    height: u32,
    scale_factor: f64,
) -> (RenderApp, crate::thread_comm::EmacsComms) {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let image_metadata = Arc::new(ImageRenderState::default());
    let shared_monitors = Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));
    let mut app = RenderApp::new(
        render,
        width,
        height,
        "test".to_string(),
        image_metadata,
        shared_monitors,
        true,
        crate::terminal::new_shared_terminals(),
    );
    if let Some(primary) = app.frame_windows.primary_window_mut()
        && let FrameLifecycle::Pending {
            scale_factor: sf, ..
        } = &mut primary.lifecycle
    {
        *sf = scale_factor;
    }
    (app, emacs)
}

fn make_test_app(width: u32, height: u32, scale_factor: f64) -> RenderApp {
    make_test_app_with_comms(width, height, scale_factor).0
}

/// Install a Window-target terminal whose text body maps pixel `(x, y)` to
/// cell `(x / 8, y / 16)` and make that frame current. `mouse_mode`, when
/// present, is fed to the child so it asks for mouse reporting.
fn install_window_terminal(app: &mut RenderApp, id: TerminalId, mouse_mode: Option<&[u8]>) {
    let mut view = TerminalView::for_test(
        id,
        TerminalGridSize::new(20, 4).unwrap(),
        TerminalDisplayTarget::Window {
            buffer: neovm_core::buffer::BufferId(2),
        },
        Arc::new(parking_lot::Mutex::new(Vec::new())),
    );
    view.feed_for_test(b"hello");
    if let Some(mode) = mouse_mode {
        view.feed_for_test(mode);
    }
    app.terminal_manager.terminals.insert(id, view);

    let mut frame = FrameGlyphBuffer::with_size(200.0, 100.0);
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    frame.add_window_info(
        DisplayWindowId::new(1),
        2,
        0,
        0,
        0,
        BufferModiff::new(0),
        0.0,
        0.0,
        200.0,
        100.0,
        0.0,
        0.0,
        0.0,
        true,
        false,
        16.0,
        "t".into(),
        String::new(),
        false,
    );
    frame.add_terminal(id.get(), 0.0, 0.0, 200.0, 100.0);

    let render = &mut app.frame_windows.primary_window_mut().unwrap().render;
    render.set_emacs_frame_id(0x42);
    render.set_current_frame(Some(frame), None, Default::default(), Default::default());
    render.set_surface_state(
        SurfaceState::from_device_size(200, 100, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
}

/// Drain every queued display input, unwrapping the transport wrappers.
fn drain_input_events(
    emacs: &crate::thread_comm::EmacsComms,
) -> Vec<crate::thread_comm::InputEvent> {
    let mut events = Vec::new();
    while let Ok(event) = emacs.input_rx.try_recv() {
        let event = match event {
            crate::thread_comm::InputEvent::Tracked { event, .. } => *event,
            event => event,
        };
        let event = match event {
            crate::thread_comm::InputEvent::Observed { event, .. } => *event,
            event => event,
        };
        events.push(event);
    }
    events
}

/// The one `TerminalSelection` in `events`, if any.
fn terminal_selection(events: Vec<crate::thread_comm::InputEvent>) -> Option<(TerminalId, String)> {
    events.into_iter().find_map(|event| match event {
        crate::thread_comm::InputEvent::TerminalSelection { id, text } => Some((id, text)),
        _ => None,
    })
}

#[test]
fn a_release_reports_only_the_release_not_a_trailing_drag() {
    let mut app = make_test_app(200, 100, 1.0);
    let window_id = WindowId::from_raw(1);
    app.frame_windows.primary_winit_id = Some(window_id);

    let id = TerminalId::new(7).unwrap();
    let mut view = TerminalView::for_test(
        id,
        TerminalGridSize::new(80, 24).unwrap(),
        TerminalDisplayTarget::Window {
            buffer: neovm_core::buffer::BufferId(2),
        },
        Arc::new(parking_lot::Mutex::new(Vec::new())),
    );
    // Mode 1002 (drag, not click-only) is what makes the ordering observable:
    // with mode 1000 a stray motion encodes nothing at all.
    view.feed_for_test(b"\x1b[?1002h\x1b[?1006h");
    assert!(
        view.reports_for_test(),
        "the terminal rejected the mouse-reporting mode; the wire assertions \
         below would pass vacuously"
    );
    app.terminal_manager.terminals.insert(id, view);

    let mut frame = FrameGlyphBuffer::with_size(200.0, 100.0);
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    frame.add_window_info(
        DisplayWindowId::new(1),
        2,
        0,
        0,
        0,
        BufferModiff::new(0),
        0.0,
        0.0,
        200.0,
        100.0,
        0.0,
        0.0,
        0.0,
        true,
        false,
        16.0,
        "t".into(),
        String::new(),
        false,
    );
    frame.add_terminal(7, 0.0, 0.0, 200.0, 100.0);

    // The surface-to-frame mapping is derived from the frame that is current
    // when the surface state is installed, so the frame must be current first.
    let render = &mut app.frame_windows.primary_window_mut().unwrap().render;
    render.set_emacs_frame_id(0x42);
    render.set_current_frame(Some(frame), None, Default::default(), Default::default());
    render.set_surface_state(
        SurfaceState::from_device_size(200, 100, DeviceScale::new(1.0).unwrap()).unwrap(),
    );

    let protocol = app.terminal_manager.get(id).unwrap().mouse_protocol();
    assert!(
        protocol.reports(),
        "mouse mode must be live before dispatch"
    );

    // Press at cell (1, 1); release one cell to the right. Releasing at the
    // pressed cell would be suppressed by the same-cell motion rule and hide
    // the very ordering this test exists to pin.
    let press = PhysicalPosition::new(10.0, 20.0);
    let release = PhysicalPosition::new(18.0, 20.0);

    app.dispatch_primary_pointer_button(
        window_id,
        ElementState::Pressed,
        MouseButton::Left.into(),
        press,
    );
    let after_press = app
        .terminal_manager
        .get(id)
        .unwrap()
        .take_written_for_test();

    app.dispatch_primary_pointer_button(
        window_id,
        ElementState::Released,
        MouseButton::Left.into(),
        release,
    );
    let after_release = app
        .terminal_manager
        .get(id)
        .unwrap()
        .take_written_for_test();

    let mods = mouse::MouseModifiers::default();
    assert_eq!(
        after_press,
        mouse::encode(
            protocol,
            mouse::MouseAction::Press,
            Some(mouse::MouseButton::Left),
            1,
            1,
            mods,
        )
        .expect("the press is reportable under mode 1002"),
        "the press must be a single report for the pressed cell"
    );

    let report_count = after_release
        .windows(3)
        .filter(|window| *window == b"\x1b[<")
        .count();
    assert_eq!(
        report_count,
        1,
        "the release must be the only report, got {:?}",
        String::from_utf8_lossy(&after_release)
    );
    assert!(
        !after_release.contains(&b'M'),
        "a press or drag report leaked ahead of the release: {:?}",
        String::from_utf8_lossy(&after_release)
    );
    assert_eq!(
        after_release,
        mouse::encode(
            protocol,
            mouse::MouseAction::Release,
            Some(mouse::MouseButton::Left),
            2,
            1,
            mods,
        )
        .expect("the release is reportable under mode 1002"),
        "the release must encode the release cell and the SGR release final byte"
    );
}

#[test]
fn a_drag_over_a_non_reporting_terminal_pushes_its_text() {
    let (mut app, emacs) = make_test_app_with_comms(200, 100, 1.0);
    let window_id = WindowId::from_raw(1);
    app.frame_windows.primary_winit_id = Some(window_id);

    let id = TerminalId::new(9).unwrap();
    install_window_terminal(&mut app, id, None);
    assert!(
        !app.terminal_manager.get(id).unwrap().reports_for_test(),
        "the fixture must not report mouse events, or this test proves nothing"
    );

    // Press the left edge of "hello", drag two cells, release.
    app.dispatch_primary_pointer_button(
        window_id,
        ElementState::Pressed,
        MouseButton::Left.into(),
        PhysicalPosition::new(4.0, 8.0),
    );
    app.handle_cursor_moved(window_id, PhysicalPosition::new(20.0, 8.0));
    app.dispatch_primary_pointer_button(
        window_id,
        ElementState::Released,
        MouseButton::Left.into(),
        PhysicalPosition::new(20.0, 8.0),
    );

    assert_eq!(
        terminal_selection(drain_input_events(&emacs)),
        Some((id, "hel".to_string())),
        "the dragged range must reach Emacs as TerminalSelection"
    );
}

#[test]
fn a_shiftless_press_over_a_reporting_terminal_still_reports() {
    let mut app = make_test_app(200, 100, 1.0);
    let window_id = WindowId::from_raw(1);
    app.frame_windows.primary_winit_id = Some(window_id);

    let id = TerminalId::new(10).unwrap();
    install_window_terminal(&mut app, id, Some(b"\x1b[?1002h\x1b[?1006h"));
    let protocol = app.terminal_manager.get(id).unwrap().mouse_protocol();
    assert!(protocol.reports());

    app.dispatch_primary_pointer_button(
        window_id,
        ElementState::Pressed,
        MouseButton::Left.into(),
        PhysicalPosition::new(10.0, 20.0),
    );

    assert_eq!(
        app.terminal_manager
            .get(id)
            .unwrap()
            .take_written_for_test(),
        mouse::encode(
            protocol,
            mouse::MouseAction::Press,
            Some(mouse::MouseButton::Left),
            1,
            1,
            mouse::MouseModifiers::default(),
        )
        .expect("the press is reportable under mode 1002"),
        "a reporting child keeps receiving its press when Shift is not held"
    );
    assert!(
        app.terminal_drag.is_none(),
        "a forwarded press must not start a local selection"
    );
}

#[test]
fn shift_over_a_reporting_terminal_selects_instead_of_reporting() {
    let mut app = make_test_app(200, 100, 1.0);
    let window_id = WindowId::from_raw(1);
    app.frame_windows.primary_winit_id = Some(window_id);

    let id = TerminalId::new(11).unwrap();
    install_window_terminal(&mut app, id, Some(b"\x1b[?1002h\x1b[?1006h"));
    assert!(app.terminal_manager.get(id).unwrap().reports_for_test());

    app.modifiers |= NEOMACS_SHIFT_MASK;
    app.dispatch_primary_pointer_button(
        window_id,
        ElementState::Pressed,
        MouseButton::Left.into(),
        PhysicalPosition::new(4.0, 8.0),
    );

    assert!(
        app.terminal_manager
            .get(id)
            .unwrap()
            .take_written_for_test()
            .is_empty(),
        "Shift must suppress the child's mouse report"
    );
    assert_eq!(
        app.terminal_drag,
        Some((id, 0, 0)),
        "Shift starts a local selection drag at the pressed cell"
    );
}
