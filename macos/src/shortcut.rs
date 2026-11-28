//! AppKit shortcut lowering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. Lowering that
//! to the Swift bridge encoding is this backend's knowledge, not the
//! shared layer's (DIP: the runtime never names bridge vocabulary).
//!
//! [`to_bridge`] produces the string the Swift bridge (`w3_shortcut`)
//! consumes. It shares muda's textual shape because the bridge parser was
//! written against it; it is owned here so the runtime never names it.

use panechord_runtime::{Shortcut, ShortcutKey, ShortcutTarget};

/// Canonical key -> bridge token. Backend-owned copy; the left column
/// must stay exactly [`panechord_runtime::CANONICAL_SPECIAL_KEYS`]
/// (pinned by `special_keys_track_the_canonical_list`), the runtime only
/// validates canonical names during parsing.
const KEYS: &[(&str, &str)] = &[
    ("Comma", "Comma"),
    ("Enter", "Enter"),
    ("Tab", "Tab"),
    ("Space", "Space"),
    ("Escape", "Escape"),
    ("Backspace", "Backspace"),
    ("Delete", "Delete"),
    ("Insert", "Insert"),
    ("Home", "Home"),
    ("End", "End"),
    ("PageUp", "PageUp"),
    ("PageDown", "PageDown"),
    ("Left", "ArrowLeft"),
    ("Right", "ArrowRight"),
    ("Up", "ArrowUp"),
    ("Down", "ArrowDown"),
];

/// Lower a parsed shortcut to the Swift bridge string.
pub fn to_bridge(shortcut: &Shortcut) -> String {
    let mods = shortcut.resolved_modifiers(ShortcutTarget::MacOS);
    let mut parts = Vec::new();
    for (bit, name) in [(2, "Ctrl"), (4, "Cmd"), (8, "Alt"), (16, "Shift")] {
        if mods & bit != 0 {
            parts.push(name.to_owned());
        }
    }
    parts.push(match shortcut.key_kind() {
        ShortcutKey::Special(name) => KEYS
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, token)| (*token).to_owned())
            .unwrap_or_else(|| name.to_owned()),
        ShortcutKey::Letter(letter) => format!("Key{letter}"),
        ShortcutKey::Digit(digit) => format!("Digit{digit}"),
        ShortcutKey::Function(function) => function.to_owned(),
    });
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_keys_track_the_canonical_list() {
        use panechord_runtime::CANONICAL_SPECIAL_KEYS;
        assert_eq!(KEYS.len(), CANONICAL_SPECIAL_KEYS.len());
        for name in CANONICAL_SPECIAL_KEYS {
            assert!(
                KEYS.iter().any(|(key, _)| key == name),
                "{name} must have a bridge token"
            );
        }
        for (name, token) in KEYS {
            let shortcut = Shortcut::parse(name).unwrap();
            assert_eq!(to_bridge(&shortcut), *token);
        }
    }

    #[test]
    fn bridge_resolves_primary_to_command() {
        assert_eq!(
            to_bridge(&Shortcut::parse("Primary+O").unwrap()),
            "Cmd+KeyO"
        );
        assert_eq!(
            to_bridge(&Shortcut::parse("Primary+Shift+S").unwrap()),
            "Cmd+Shift+KeyS"
        );
    }
}
