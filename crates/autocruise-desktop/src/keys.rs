//! Key names and characters to X11 keysyms (layout-independent).

use crate::DesktopError;

pub const SHIFT_L: u32 = 0xffe1;
pub const CONTROL_L: u32 = 0xffe3;
pub const ALT_L: u32 = 0xffe9;
pub const SUPER_L: u32 = 0xffeb;

/// A parsed key combination: modifiers held while `key` is pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    pub modifiers: Vec<u32>,
    pub key: u32,
}

/// Parses `"Return"`, `"ctrl+a"`, `"shift+Tab"`... Names are
/// case-insensitive; single characters stand for themselves.
pub fn parse_combo(combo: &str) -> Result<Combo, DesktopError> {
    let unsupported = || DesktopError::UnsupportedKey(combo.to_owned());
    let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last().ok_or_else(unsupported)?;
    let modifiers = mods
        .iter()
        .map(|m| match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Ok(CONTROL_L),
            "shift" => Ok(SHIFT_L),
            "alt" => Ok(ALT_L),
            "super" | "meta" | "cmd" => Ok(SUPER_L),
            _ => Err(unsupported()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let key = named_key(key)
        .or_else(|| {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => char_keysym(c.to_ascii_lowercase()),
                _ => None,
            }
        })
        .ok_or_else(unsupported)?;
    Ok(Combo { modifiers, key })
}

fn named_key(name: &str) -> Option<u32> {
    let keysym = match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => 0xff0d,
        "escape" | "esc" => 0xff1b,
        "tab" => 0xff09,
        "backspace" => 0xff08,
        "delete" | "del" => 0xffff,
        "home" => 0xff50,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "page_up" | "pageup" => 0xff55,
        "page_down" | "pagedown" => 0xff56,
        "end" => 0xff57,
        "space" => 0x20,
        "f5" => 0xffc2,
        _ => return None,
    };
    Some(keysym)
}

/// Keysym producing a character: Latin-1 keysyms equal the code point,
/// others use the Unicode keysym range.
pub fn char_keysym(c: char) -> Option<u32> {
    let cp = c as u32;
    match c {
        '\n' | '\r' => Some(0xff0d),
        '\t' => Some(0xff09),
        _ if c.is_control() => None,
        _ if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) => Some(cp),
        _ => Some(0x0100_0000 + cp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos() {
        assert_eq!(
            parse_combo("ctrl+a").unwrap(),
            Combo {
                modifiers: vec![CONTROL_L],
                key: 'a' as u32
            }
        );
        assert_eq!(parse_combo("Return").unwrap().key, 0xff0d);
        assert_eq!(parse_combo("ctrl+L").unwrap().key, 'l' as u32);
        assert!(parse_combo("hyper+x").is_err());
        assert!(parse_combo("Nope").is_err());
    }

    #[test]
    fn characters() {
        assert_eq!(char_keysym('a'), Some(0x61));
        assert_eq!(char_keysym('é'), Some(0xe9));
        assert_eq!(char_keysym('€'), Some(0x0100_20ac));
        assert_eq!(char_keysym('\u{7}'), None);
    }
}
