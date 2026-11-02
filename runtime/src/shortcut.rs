//! Portable menu shortcut grammar: modifiers first, then one ASCII letter,
//! digit, F1..F12, Comma, or editing/navigation key. Tokens are case-insensitive and
//! trimmed. Modifiers: Primary, Ctrl, Cmd, Alt, Shift (no duplicates).
//! Primary resolves to Cmd on macOS, Ctrl elsewhere. This is not a promise of
//! physical-key/layout identity: GTK uses keyvals, muda's Accelerator uses Code.
//! Backend-specific legacy syntax is deliberately NOT parsed here; use
//! `canonical_or_raw` to sequence the canonical grammar ahead of a
//! backend-native fallback in one shared place.
use std::{borrow::Cow, fmt, str::FromStr};

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
    /// Memoized parse for menu rebuild loops: the same shortcut strings recur
    /// on every items emission, so cache the portable parse per raw string.
    /// Backends still own rendering (`to_gtk`/`to_muda`/`to_bridge`); this
    /// only skips re-splitting and re-validating the shared grammar.
    pub fn parse_cached(value: &str) -> Result<Self, ShortcutError> {
        use std::{cell::RefCell, collections::HashMap};
        thread_local! {
            static PARSED: RefCell<HashMap<String, Result<Shortcut, ShortcutError>>> =
                RefCell::new(HashMap::new());
        }
        PARSED.with(|cache| {
            if let Some(hit) = cache.borrow().get(value) {
                return hit.clone();
            }
            let parsed = Self::parse(value);
            cache.borrow_mut().insert(value.to_owned(), parsed.clone());
            parsed
        })
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

/// Canonical grammar first, raw value as the backend-native fallback: renders
/// the canonical form when `value` parses, otherwise borrows `value` so the
/// backend parser can try its own legacy syntax. Each backend passes its own
/// renderer (`panechord_gtk::shortcut::to_gtk`,
/// `panechord_tao::shortcut::to_muda`), so the fallback is sequenced in one
/// shared place without the runtime naming backend vocabulary.
pub fn canonical_or_raw(value: &str, render: impl FnOnce(&Shortcut) -> String) -> Cow<'_, str> {
    match Shortcut::parse(value) {
        Ok(shortcut) => Cow::Owned(render(&shortcut)),
        Err(_) => Cow::Borrowed(value),
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
    fn malformed_and_legacy_are_not_common_grammar() {
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
