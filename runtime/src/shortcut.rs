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
    /// Resolve Primary to the target's concrete modifier bit, keeping the
    /// other bits. Portable core of the backend renderers: bit 1 Ctrl on
    /// other targets, bit 2 Cmd on macOS.
    pub fn resolved_modifiers(&self, target: ShortcutTarget) -> u8 {
        (self.modifiers & !1)
            | if self.modifiers & 1 == 0 {
                0
            } else if target == ShortcutTarget::MacOS {
                4
            } else {
                2
            }
    }
    /// Portable classification of the canonical key. Backends match on
    /// this instead of re-implementing the single-char/`F<n>` branches;
    /// the `Special` payload is one of [`CANONICAL_SPECIAL_KEYS`].
    pub fn key_kind(&self) -> ShortcutKey<'_> {
        if let Some(name) = CANONICAL_SPECIAL_KEYS
            .iter()
            .find(|name| **name == self.key)
        {
            ShortcutKey::Special(name)
        } else if self.key.len() == 1 {
            let byte = self.key.as_bytes()[0];
            if byte.is_ascii_alphabetic() {
                ShortcutKey::Letter(byte as char)
            } else {
                ShortcutKey::Digit(byte as char)
            }
        } else {
            ShortcutKey::Function(&self.key)
        }
    }
}

/// Portable classification of a canonical shortcut key for backend
/// renderers. `Special` names come from [`CANONICAL_SPECIAL_KEYS`];
/// letters are already uppercased by parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey<'a> {
    Special(&'a str),
    Letter(char),
    Digit(char),
    Function(&'a str),
}

/// Canonical editing/navigation key names. This is the single source of
/// truth: backend value maps (`KEYS` in the GTK/muda renderers) key off
/// these names, and parsing accepts exactly these.
pub const CANONICAL_SPECIAL_KEYS: &[&str] = KEYS;

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
    fn resolution_and_key_kinds_are_portable() {
        use crate::ShortcutKey;
        let shortcut = Shortcut::parse("Primary+S").unwrap();
        assert_eq!(
            shortcut.resolved_modifiers(ShortcutTarget::MacOS),
            4,
            "Primary resolves to Cmd on macOS"
        );
        assert_eq!(
            shortcut.resolved_modifiers(ShortcutTarget::Other),
            2,
            "Primary resolves to Ctrl elsewhere"
        );
        assert_eq!(shortcut.key_kind(), ShortcutKey::Letter('S'));
        assert_eq!(
            Shortcut::parse("Cmd+Alt+9").unwrap().key_kind(),
            ShortcutKey::Digit('9')
        );
        assert_eq!(
            Shortcut::parse("F11").unwrap().key_kind(),
            ShortcutKey::Function("F11")
        );
        assert_eq!(
            Shortcut::parse("Primary+Comma").unwrap().key_kind(),
            ShortcutKey::Special(&"Comma")
        );
        for name in CANONICAL_SPECIAL_KEYS {
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
