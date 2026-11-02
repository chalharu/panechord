//! AppKit shortcut lowering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. Lowering that to
//! the Swift bridge encoding is this backend's knowledge, not the shared
//! layer's: previously `Shortcut::to_appkit`/`to_muda*` lived in
//! `panechord-runtime`, so the shared crate depended on backend vocabulary
//! (DIP violation).
//!
//! [`to_bridge`] produces the string the Swift bridge (`w3_shortcut`)
//! consumes. It shares muda's textual shape because the bridge parser was
//! written against it; it is owned here so the runtime never names it.

use panechord_runtime::Shortcut;

/// Canonical key -> bridge token. Backend-owned copy; the runtime only
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

fn resolved(shortcut: &Shortcut) -> u8 {
    let modifiers = shortcut.modifiers();
    (modifiers & !1) | if modifiers & 1 == 0 { 0 } else { 4 }
}

/// Lower a parsed shortcut to the Swift bridge string.
pub fn to_bridge(shortcut: &Shortcut) -> String {
    let mods = resolved(shortcut);
    let mut parts = Vec::new();
    for (bit, name) in [(2, "Ctrl"), (4, "Cmd"), (8, "Alt"), (16, "Shift")] {
        if mods & bit != 0 {
            parts.push(name.to_owned());
        }
    }
    parts.push(
        if let Some((_, token)) = KEYS.iter().find(|(key, _)| *key == shortcut.key()) {
            (*token).into()
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

#[cfg(test)]
mod tests {
    use super::*;

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
