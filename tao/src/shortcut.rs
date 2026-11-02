//! muda-native menu shortcut rendering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. How that lowers to
//! muda `Accelerator` strings is this backend's knowledge, not the shared
//! layer's: previously `Shortcut::to_muda*` lived in `panechord-runtime`, so
//! the shared crate depended on backend vocabulary (DIP violation).

use panechord_runtime::{Shortcut, ShortcutTarget};

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

fn resolved(shortcut: &Shortcut, target: ShortcutTarget) -> u8 {
    let modifiers = shortcut.modifiers();
    (modifiers & !1)
        | if modifiers & 1 == 0 {
            0
        } else if target == ShortcutTarget::MacOS {
            4
        } else {
            2
        }
}

/// Lower a parsed shortcut to a muda accelerator string for an explicit target.
pub fn to_muda_for(shortcut: &Shortcut, target: ShortcutTarget) -> String {
    let mods = resolved(shortcut, target);
    let mut parts = Vec::new();
    for (bit, name) in [(2, "Ctrl"), (4, "Cmd"), (8, "Alt"), (16, "Shift")] {
        if mods & bit != 0 {
            parts.push(name.to_owned());
        }
    }
    parts.push(
        if let Some((_, muda)) = KEYS.iter().find(|(key, _)| *key == shortcut.key()) {
            (*muda).into()
        } else if shortcut.key().len() == 1 && shortcut.key().as_bytes()[0].is_ascii_alphabetic() {
            format!("Key{}", shortcut.key())
        } else if shortcut.key().len() == 1 {
            format!("Digit{}", shortcut.key())
        } else {
            shortcut.key().to_owned()
        },
    );
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
    fn supported_keys_convert() {
        for (name, muda) in KEYS {
            let shortcut = Shortcut::parse(name).unwrap();
            assert_eq!(to_muda(&shortcut), *muda);
        }
    }
}
