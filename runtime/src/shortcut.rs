//! Portable menu shortcut grammar: modifiers first, then one ASCII letter,
//! digit, F1..F12, Comma, or editing/navigation key. Tokens are case-insensitive and
//! trimmed. Modifiers: Primary, Ctrl, Cmd, Alt, Shift (no duplicates).
//! Primary resolves to Cmd on macOS, Ctrl elsewhere. This is not a promise of
//! physical-key/layout identity: GTK uses keyvals, muda's Accelerator uses Code.
//! Only this grammar is accepted; backend renderers lower the parsed form.
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutTarget {
    MacOS,
    Other,
}
impl ShortcutTarget {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOS
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    modifiers: u8,
    key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutError;
impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected [Primary|Ctrl|Cmd|Alt|Shift +] letter/digit/F1..F12/editing key")
    }
}
impl std::error::Error for ShortcutError {}

// Canonical key names only. Native renderings (GTK keyvals, muda Codes,
// AppKit equivalents) are backend-owned; the runtime never names them.
const KEYS: &[&str] = &[
    "Comma",
    "Enter",
    "Tab",
    "Space",
    "Escape",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Left",
    "Right",
    "Up",
    "Down",
];

impl FromStr for Shortcut {
    type Err = ShortcutError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let tokens: Vec<_> = value.split('+').map(str::trim).collect();
        let mut modifiers = 0;
        for token in &tokens[..tokens.len() - 1] {
            let bit = match token.to_ascii_lowercase().as_str() {
                "primary" => 1,
                "ctrl" => 2,
                "cmd" => 4,
                "alt" => 8,
                "shift" => 16,
                _ => return Err(ShortcutError),
            };
            if modifiers & bit != 0 {
                return Err(ShortcutError);
            }
            modifiers |= bit;
        }
        // Avoid target-dependent duplicate modifiers such as Primary+Ctrl.
        if modifiers & 1 != 0 && modifiers & 6 != 0 {
            return Err(ShortcutError);
        }
        let raw = tokens[tokens.len() - 1];
        let key = if raw.len() == 1 && raw.as_bytes()[0].is_ascii_alphanumeric() {
            raw.to_ascii_uppercase()
        } else if let Some(name) = KEYS.iter().find(|name| name.eq_ignore_ascii_case(raw)) {
            (*name).into()
        } else {
            let upper = raw.to_ascii_uppercase();
            let n = upper
                .strip_prefix('F')
                .and_then(|n| n.parse::<u8>().ok())
                .ok_or(ShortcutError)?;
            if !(1..=12).contains(&n) || upper != format!("F{n}") {
                return Err(ShortcutError);
            }
            upper
        };
        Ok(Self { modifiers, key })
    }
}

impl Shortcut {
    pub fn parse(value: &str) -> Result<Self, ShortcutError> {
        value.parse()
    }
    /// Raw modifier bitmap for backend renderers: bit 0 Primary, 1 Ctrl,
    /// 2 Cmd, 3 Alt, 4 Shift. Backends resolve Primary to Cmd on macOS and
    /// Ctrl elsewhere.
    pub fn modifiers(&self) -> u8 {
        self.modifiers
    }
    /// Canonical key name: one ASCII letter/digit, `F1`..`F12`, or an
    /// editing/navigation key such as `Comma`, `Enter`, `Left`.
    pub fn key(&self) -> &str {
        &self.key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_is_canonical_and_target_independent() {
        // Resolution to Cmd/Ctrl is backend rendering, not parsing: the
        // canonical form keeps Primary so each backend lowers it per target.
        for key in ["S", "Q"] {
            let shortcut = Shortcut::parse(&format!("Primary+{key}")).unwrap();
            assert_eq!(shortcut.modifiers() & 1, 1);
            assert_eq!(shortcut.modifiers() & 6, 0);
            assert_eq!(shortcut.key(), key);
        }
    }
    #[test]
    fn canonical_names_normalize_case_and_spacing() {
        let shortcut = Shortcut::parse(" shift + CTRL + s ").unwrap();
        assert_eq!(shortcut.modifiers(), 2 | 16);
        assert_eq!(shortcut.key(), "S");
        assert_eq!(Shortcut::parse("S"), Shortcut::parse("s"));
        assert_eq!(Shortcut::parse("Primary+Comma").unwrap().key(), "Comma");
    }
    #[test]
    fn supported_keys_parse() {
        for name in KEYS {
            assert!(Shortcut::parse(name).is_ok(), "{name}");
        }
        for n in 1..=12 {
            assert!(Shortcut::parse(&format!("F{n}")).is_ok());
        }
        for key in '0'..='9' {
            assert!(Shortcut::parse(&key.to_string()).is_ok());
        }
        for key in 'A'..='Z' {
            assert!(Shortcut::parse(&key.to_string()).is_ok());
        }
    }
    #[test]
    fn malformed_inputs_are_not_common_grammar() {
        for input in [
            "",
            "+",
            "Ctrl+",
            "Ctrl++S",
            "Ctrl+Ctrl+S",
            "Primary+Cmd+S",
            "Primary+Ctrl+S",
            "S+Ctrl",
            "Ctrl+S+Q",
            "F0",
            "F13",
            "F01",
            "é",
            "Ctrl+KeyS",
            "<Control>s",
        ] {
            assert!(Shortcut::parse(input).is_err(), "{input}");
        }
    }
}
