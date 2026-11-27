//! muda-native menu shortcut rendering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. How that lowers to
//! muda `Accelerator` strings is this backend's knowledge, not the shared
//! layer's: previously `Shortcut::to_muda*` lived in `panechord-runtime`, so
//! the shared crate depended on backend vocabulary (DIP violation).

use panechord_runtime::{Shortcut, ShortcutKey, ShortcutTarget};

/// Canonical key -> muda `Code` name. Backend-owned copy; the left column
/// must stay exactly [`panechord_runtime::CANONICAL_SPECIAL_KEYS`]
/// (pinned by `special_keys_track_the_canonical_list`), the runtime only
/// validates canonical names during parsing.

/// Canonical key -> muda `Code` name. Backend-owned copy; the runtime only
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

/// Lower a parsed shortcut to a muda accelerator string for an explicit target.
pub fn to_muda_for(shortcut: &Shortcut, target: ShortcutTarget) -> String {
    let mods = shortcut.resolved_modifiers(target);
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
            .map(|(_, muda)| (*muda).to_owned())
            .unwrap_or_else(|| name.to_owned()),
        ShortcutKey::Letter(letter) => format!("Key{letter}"),
        ShortcutKey::Digit(digit) => format!("Digit{digit}"),
        ShortcutKey::Function(function) => function.to_owned(),
    });
    parts.join("+")
}

/// Lower a parsed shortcut for the current build target.
pub fn to_muda(shortcut: &Shortcut) -> String {
    to_muda_for(shortcut, ShortcutTarget::current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_resolves_per_target() {
        for key in ["S", "Q"] {
            let shortcut = Shortcut::parse(&format!("Primary+{key}")).unwrap();
            assert_eq!(
                to_muda_for(&shortcut, ShortcutTarget::MacOS),
                format!("Cmd+Key{key}")
            );
            assert_eq!(
                to_muda_for(&shortcut, ShortcutTarget::Other),
                format!("Ctrl+Key{key}")
            );
        }
    }

    #[test]
    fn comma_resolves_for_both_targets() {
        let shortcut = Shortcut::parse("Primary+Comma").unwrap();
        assert_eq!(to_muda_for(&shortcut, ShortcutTarget::MacOS), "Cmd+Comma");
        assert_eq!(to_muda_for(&shortcut, ShortcutTarget::Other), "Ctrl+Comma");
    }

    #[test]
    fn modifiers_normalize_without_implicit_shift() {
        let shortcut = Shortcut::parse(" shift + CTRL + s ").unwrap();
        assert_eq!(to_muda(&shortcut), "Ctrl+Shift+KeyS");
        assert_eq!(
            Shortcut::parse("Cmd+Alt+9").unwrap().key(),
            "9",
            "canonical key is preserved for digit rendering"
        );
        assert_eq!(
            to_muda(&Shortcut::parse("Cmd+Alt+9").unwrap()),
            "Cmd+Alt+Digit9"
        );
    }

    #[test]
    fn special_keys_track_the_canonical_list() {
        use panechord_runtime::CANONICAL_SPECIAL_KEYS;
        assert_eq!(KEYS.len(), CANONICAL_SPECIAL_KEYS.len());
        for name in CANONICAL_SPECIAL_KEYS {
            assert!(
                KEYS.iter().any(|(key, _)| key == name),
                "{name} must have a muda Code"
            );
        }
        for (name, muda) in KEYS {
            let shortcut = Shortcut::parse(name).unwrap();
            assert_eq!(to_muda(&shortcut), *muda);
        }
    }
}
