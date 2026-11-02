//! GTK-native menu shortcut rendering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. How that lowers to
//! GTK keyval trigger strings is this backend's knowledge, not the shared
//! layer's: previously `Shortcut::to_gtk*` lived in `panechord-runtime`, so
//! the shared crate depended on backend vocabulary (DIP violation).

use panechord_runtime::{Shortcut, ShortcutTarget};

/// Canonical key -> GTK keyval name. Backend-owned copy; the runtime only
/// validates canonical names during parsing.
const KEYS: &[(&str, &str)] = &[
    ("Comma", "comma"),
    ("Enter", "Return"),
    ("Tab", "Tab"),
    ("Space", "space"),
    ("Escape", "Escape"),
    ("Backspace", "BackSpace"),
    ("Delete", "Delete"),
    ("Insert", "Insert"),
    ("Home", "Home"),
    ("End", "End"),
    ("PageUp", "Page_Up"),
    ("PageDown", "Page_Down"),
    ("Left", "Left"),
    ("Right", "Right"),
    ("Up", "Up"),
    ("Down", "Down"),
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

/// Lower a parsed shortcut to a GTK trigger string for an explicit target.
pub fn to_gtk_for(shortcut: &Shortcut, target: ShortcutTarget) -> String {
    let mods = resolved(shortcut, target);
    let mut out = String::new();
    for (bit, name) in [(2, "Control"), (4, "Meta"), (8, "Alt"), (16, "Shift")] {
        if mods & bit != 0 {
            out.push_str(&format!("<{name}>"));
        }
    }
    if let Some((_, gtk)) = KEYS.iter().find(|(key, _)| *key == shortcut.key()) {
        out.push_str(gtk);
    } else if shortcut.key().len() == 1 {
        out.push_str(&shortcut.key().to_ascii_lowercase());
    } else {
        out.push_str(shortcut.key());
    }
    out
}

/// Lower a parsed shortcut for the current build target.
pub fn to_gtk(shortcut: &Shortcut) -> String {
    to_gtk_for(shortcut, ShortcutTarget::current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_resolves_per_target() {
        for key in ["S", "Q"] {
            let shortcut = Shortcut::parse(&format!("Primary+{key}")).unwrap();
            assert_eq!(
                to_gtk_for(&shortcut, ShortcutTarget::MacOS),
                format!("<Meta>{}", key.to_lowercase())
            );
            assert_eq!(
                to_gtk_for(&shortcut, ShortcutTarget::Other),
                format!("<Control>{}", key.to_lowercase())
            );
        }
    }

    #[test]
    fn comma_resolves_for_both_targets() {
        let shortcut = Shortcut::parse("Primary+Comma").unwrap();
        assert_eq!(to_gtk_for(&shortcut, ShortcutTarget::MacOS), "<Meta>comma");
        assert_eq!(
            to_gtk_for(&shortcut, ShortcutTarget::Other),
            "<Control>comma"
        );
    }

    #[test]
    fn supported_keys_convert() {
        for (name, gtk) in KEYS {
            let shortcut = Shortcut::parse(name).unwrap();
            assert_eq!(to_gtk(&shortcut), *gtk);
        }
        for n in 1..=12 {
            let key = format!("F{n}");
            assert_eq!(to_gtk(&Shortcut::parse(&key).unwrap()), key);
        }
    }

    #[test]
    fn modifiers_normalize_without_implicit_shift() {
        let shortcut = Shortcut::parse(" shift + CTRL + s ").unwrap();
        assert_eq!(to_gtk(&shortcut), "<Control><Shift>s");
        assert_eq!(
            Shortcut::parse("S").unwrap().key(),
            Shortcut::parse("s").unwrap().key()
        );
    }
}
