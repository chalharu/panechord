//! GTK-native menu shortcut rendering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. How that lowers to
//! GTK keyval trigger strings is this backend's knowledge, not the shared
//! layer's: previously `Shortcut::to_gtk*` lived in `panechord-runtime`, so
//! the shared crate depended on backend vocabulary (DIP violation).

use panechord_runtime::{Shortcut, ShortcutKey, ShortcutTarget};

/// Canonical key -> GTK keyval name. Backend-owned copy; the left column
/// must stay exactly [`panechord_runtime::CANONICAL_SPECIAL_KEYS`]
/// (pinned by `special_keys_track_the_canonical_list`), the runtime only
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

/// Lower a parsed shortcut to a GTK trigger string for an explicit target.
pub fn to_gtk_for(shortcut: &Shortcut, target: ShortcutTarget) -> String {
    let mods = shortcut.resolved_modifiers(target);
    let mut out = String::new();
    for (bit, name) in [(2, "Control"), (4, "Meta"), (8, "Alt"), (16, "Shift")] {
        if mods & bit != 0 {
            out.push_str(&format!("<{name}>"));
        }
    }
    match shortcut.key_kind() {
        ShortcutKey::Special(name) => out.push_str(
            KEYS.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, gtk)| *gtk)
                .unwrap_or(name),
        ),
        ShortcutKey::Letter(letter) => out.push(letter.to_ascii_lowercase()),
        ShortcutKey::Digit(digit) => out.push(digit),
        ShortcutKey::Function(function) => out.push_str(function),
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
    fn special_keys_track_the_canonical_list() {
        use panechord_runtime::CANONICAL_SPECIAL_KEYS;
        assert_eq!(KEYS.len(), CANONICAL_SPECIAL_KEYS.len());
        for name in CANONICAL_SPECIAL_KEYS {
            assert!(
                KEYS.iter().any(|(key, _)| key == name),
                "{name} must have a GTK keyval"
            );
        }
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
