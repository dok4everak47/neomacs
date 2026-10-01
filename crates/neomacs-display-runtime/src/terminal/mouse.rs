//! Mouse reporting for neo-term terminal instances.
//!
//! A full-screen child process (vim, htop, tmux, ...) asks its terminal to
//! forward pointer input by enabling one of the DEC private modes 1000, 1002
//! or 1003; the wire encoding is then chosen by mode 1006 (SGR) or 1005
//! (UTF-8), and otherwise by the original `CSI M` byte form.
//!
//! Ensure the mode snapshot and the encoder stay separable from the PTY: the
//! whole protocol is decided by [`MouseProtocol`] plus a cell coordinate, so
//! it can be exercised without a terminal or a display.

use rio_vt::crosswords::Mode;

/// Mouse reporting a child process has requested.
///
/// Each field mirrors one DEC private mode. They are not mutually exclusive
/// as *encoding* choices, but the three reporting modes are: `rio-vt` keeps
/// only the last one enabled (see its `report_private_mode` handling), so at
/// most one of `report_click`/`report_drag`/`report_motion` is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseProtocol {
    /// Mode 1000: report button press and release.
    pub report_click: bool,
    /// Mode 1002: report button press/release and motion while a button is held.
    pub report_drag: bool,
    /// Mode 1003: report all motion, button held or not.
    pub report_motion: bool,
    /// Mode 1006: SGR extended coordinates.
    pub sgr: bool,
    /// Mode 1005: UTF-8 extended coordinates in the legacy byte form.
    pub utf8: bool,
}

impl MouseProtocol {
    /// Snapshot the reporting modes tracked by `rio-vt`.
    pub fn from_mode(mode: Mode) -> Self {
        Self {
            report_click: mode.contains(Mode::MOUSE_REPORT_CLICK),
            report_drag: mode.contains(Mode::MOUSE_DRAG),
            report_motion: mode.contains(Mode::MOUSE_MOTION),
            sgr: mode.contains(Mode::SGR_MOUSE),
            utf8: mode.contains(Mode::UTF8_MOUSE),
        }
    }

    /// Whether the child wants any pointer reporting at all.
    pub fn reports(&self) -> bool {
        self.report_click || self.report_drag || self.report_motion
    }

    /// Whether motion with a button held is reported.
    pub fn reports_drag(&self) -> bool {
        self.report_drag || self.report_motion
    }

    /// Whether motion with no button held is reported.
    pub fn reports_motion(&self) -> bool {
        self.report_motion
    }
}

/// A pointer button, including the wheel, which xterm reports as buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
    Back,
    Forward,
}

impl MouseButton {
    /// The xterm button code without modifier or motion bits.
    ///
    /// xterm encodes the button as a bitfield: after X buttons 4 and 5 are
    /// shifted up to DEC numbers 6 and 7, `BtnCode` is `button & 3`, plus 64
    /// when bit 2 is set and plus 128 when bit 3 is set (src/button.c:
    /// 5389-5404). Back/Forward are X buttons 8/9, so they reach the wire as
    /// 128/129 (ctlseqs: "by adding 128 (for buttons 8 through 11)").
    fn code(self) -> u16 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
            Self::Back => 128,
            Self::Forward => 129,
            Self::WheelUp => 64,
            Self::WheelDown => 65,
            Self::WheelLeft => 66,
            Self::WheelRight => 67,
        }
    }

    /// Wheel events are presses with no matching release on the wire.
    pub fn is_wheel(self) -> bool {
        matches!(
            self,
            Self::WheelUp | Self::WheelDown | Self::WheelLeft | Self::WheelRight
        )
    }
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// Keyboard modifiers, as the mouse protocol's modifier bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseModifiers(u8);

impl MouseModifiers {
    pub const SHIFT: u8 = 4;
    pub const META: u8 = 8;
    pub const CTRL: u8 = 16;

    pub fn from_bits(bits: u8) -> Self {
        Self(bits & (Self::SHIFT | Self::META | Self::CTRL))
    }

    fn bits(self) -> u16 {
        u16::from(self.0)
    }
}

/// Encode one pointer event for the child process.
///
/// `col` and `row` are zero-based grid cells; the wire form is one-based.
/// Returns `None` when the child has not asked for this kind of event, or
/// when a legacy coordinate cannot be represented in the byte form.
pub fn encode(
    protocol: MouseProtocol,
    action: MouseAction,
    button: Option<MouseButton>,
    col: usize,
    row: usize,
    modifiers: MouseModifiers,
) -> Option<Vec<u8>> {
    if !protocol.reports() {
        return None;
    }
    let button = match action {
        MouseAction::Press | MouseAction::Release => {
            let button = button?;
            if button.is_wheel() && action == MouseAction::Release {
                // xterm only puts a wheel press on the wire.
                return None;
            }
            Some(button)
        }
        MouseAction::Motion => match button {
            Some(button) if protocol.reports_drag() => Some(button),
            None if protocol.reports_motion() => None,
            _ => return None,
        },
    };

    let col = col as u64 + 1;
    let row = row as u64 + 1;

    let mut code = match button {
        Some(button) => button.code(),
        // Bare motion carries the "no button" code.
        None => 3,
    };
    if action == MouseAction::Motion {
        code += 32;
    }
    code += modifiers.bits();
    // A legacy release is code 3 regardless of which button came up.
    if action == MouseAction::Release && !protocol.sgr {
        code = 3 + modifiers.bits();
    }

    if protocol.sgr {
        let final_byte = if action == MouseAction::Release {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{code};{col};{row}{final_byte}").into_bytes());
    }

    // Legacy `CSI M` form: three single bytes. Coordinates are offset by 32,
    // and in UTF-8 mode (1005) each offset coordinate is one UTF-8 codepoint.
    let mut out = Vec::with_capacity(6);
    out.extend_from_slice(b"\x1b[M");
    out.push(u8::try_from(code + 32).ok()?);
    if protocol.utf8 {
        push_utf8(&mut out, col + 32);
        push_utf8(&mut out, row + 32);
    } else {
        out.push(u8::try_from(col + 32).ok()?);
        out.push(u8::try_from(row + 32).ok()?);
    }
    Some(out)
}

fn push_utf8(out: &mut Vec<u8>, codepoint: u64) {
    let character = u32::try_from(codepoint)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buffer = [0u8; 4];
    out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sgr() -> MouseProtocol {
        MouseProtocol {
            report_click: true,
            sgr: true,
            ..MouseProtocol::default()
        }
    }

    #[test]
    fn from_mode_reads_each_tracked_flag() {
        let mode = Mode::MOUSE_REPORT_CLICK | Mode::SGR_MOUSE;
        let protocol = MouseProtocol::from_mode(mode);
        assert!(protocol.report_click);
        assert!(!protocol.report_drag);
        assert!(!protocol.report_motion);
        assert!(protocol.sgr);
        assert!(!protocol.utf8);
        assert!(protocol.reports());
    }

    #[test]
    fn disabled_protocol_encodes_nothing() {
        assert_eq!(
            encode(
                MouseProtocol::default(),
                MouseAction::Press,
                Some(MouseButton::Left),
                0,
                0,
                MouseModifiers::default(),
            ),
            None
        );
    }

    #[test]
    fn sgr_press_and_release_are_one_based() {
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Press,
                Some(MouseButton::Left),
                3,
                7,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<0;4;8M".to_vec()
        );
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Release,
                Some(MouseButton::Left),
                3,
                7,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<0;4;8m".to_vec()
        );
    }

    #[test]
    fn sgr_motion_sets_the_motion_bit() {
        let protocol = MouseProtocol {
            report_motion: true,
            sgr: true,
            ..MouseProtocol::default()
        };
        // Bare motion: button-less code 3 + 32.
        assert_eq!(
            encode(
                protocol,
                MouseAction::Motion,
                None,
                0,
                0,
                MouseModifiers::default()
            )
            .unwrap(),
            b"\x1b[<35;1;1M".to_vec()
        );
        // Drag: left button 0 + 32.
        assert_eq!(
            encode(
                protocol,
                MouseAction::Motion,
                Some(MouseButton::Left),
                1,
                1,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<32;2;2M".to_vec()
        );
    }

    #[test]
    fn click_only_protocol_drops_motion() {
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Motion,
                Some(MouseButton::Left),
                0,
                0,
                MouseModifiers::default(),
            ),
            None
        );
    }

    #[test]
    fn drag_protocol_reports_a_held_button_but_not_bare_motion() {
        let protocol = MouseProtocol {
            report_drag: true,
            sgr: true,
            ..MouseProtocol::default()
        };
        assert_eq!(
            encode(
                protocol,
                MouseAction::Motion,
                Some(MouseButton::Left),
                1,
                1,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<32;2;2M".to_vec()
        );
        assert_eq!(
            encode(
                protocol,
                MouseAction::Motion,
                None,
                1,
                1,
                MouseModifiers::default()
            ),
            None
        );
    }

    #[test]
    fn wheel_is_a_press_and_its_release_is_dropped() {
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Press,
                Some(MouseButton::WheelUp),
                0,
                0,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<64;1;1M".to_vec()
        );
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Release,
                Some(MouseButton::WheelUp),
                0,
                0,
                MouseModifiers::default(),
            ),
            None
        );
    }

    #[test]
    fn extra_buttons_use_the_high_bitfield() {
        // Back/Forward are X buttons 8/9, which xterm puts on the wire as the
        // bitfield codes 128/129 in SGR and as code + 32 in the legacy form.
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Press,
                Some(MouseButton::Back),
                0,
                0,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<128;1;1M".to_vec()
        );
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Press,
                Some(MouseButton::Forward),
                0,
                0,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[<129;1;1M".to_vec()
        );
        let legacy = MouseProtocol {
            report_click: true,
            ..MouseProtocol::default()
        };
        assert_eq!(
            encode(
                legacy,
                MouseAction::Press,
                Some(MouseButton::Back),
                0,
                0,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[M\xa0!!".to_vec()
        );
        assert_eq!(
            encode(
                legacy,
                MouseAction::Press,
                Some(MouseButton::Forward),
                0,
                0,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[M\xa1!!".to_vec()
        );
    }

    #[test]
    fn modifiers_are_added_to_the_code() {
        assert_eq!(
            encode(
                sgr(),
                MouseAction::Press,
                Some(MouseButton::Right),
                0,
                0,
                MouseModifiers::from_bits(MouseModifiers::CTRL | MouseModifiers::SHIFT),
            )
            .unwrap(),
            b"\x1b[<22;1;1M".to_vec()
        );
    }

    #[test]
    fn legacy_release_uses_code_three() {
        let protocol = MouseProtocol {
            report_click: true,
            ..MouseProtocol::default()
        };
        assert_eq!(
            encode(
                protocol,
                MouseAction::Release,
                Some(MouseButton::Left),
                0,
                0,
                MouseModifiers::default(),
            )
            .unwrap(),
            b"\x1b[M\x23\x21\x21".to_vec()
        );
    }

    #[test]
    fn legacy_utf8_encodes_wide_coordinates() {
        let protocol = MouseProtocol {
            report_click: true,
            utf8: true,
            ..MouseProtocol::default()
        };
        // Column 200 + 1 + 32 = 233, which is not a single byte.
        let encoded = encode(
            protocol,
            MouseAction::Press,
            Some(MouseButton::Left),
            200,
            0,
            MouseModifiers::default(),
        )
        .unwrap();
        assert_eq!(&encoded[..3], b"\x1b[M");
        assert_eq!(encoded[3], 32); // code 0 + 32
        assert_eq!(std::str::from_utf8(&encoded[4..]).unwrap(), "\u{e9}\u{21}");
    }
}
