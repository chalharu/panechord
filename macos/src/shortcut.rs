//! AppKit shortcut lowering (backend-owned).
//!
//! The runtime only parses the portable grammar (`Shortcut::parse`) and
//! exposes the canonical key plus the raw modifier bitmap. Both lowerings
//! here are this backend's knowledge, not the shared layer's: previously
//! `Shortcut::to_appkit`/`to_muda*` lived in `panechord-runtime`, so the
//! shared crate depended on backend vocabulary (DIP violation).
//!
//! Two encodings exist for historical reasons:
//!
//! * [`to_appkit`] produces `(modifierFlags, key)` for `NSMenuItem`
//!   key equivalents in the in-process global-menu mirror.
//! * [`to_bridge`] produces the string the Swift bridge (`w3_shortcut`)
//!   consumes. It shares muda's textual shape because the bridge parser was
//!   written against it; it is owned here so the runtime never names it.

use panechord_runtime::{Shortcut, ShortcutTarget};

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

/// AppKit key equivalent for `NSMenuItem`: `(modifierFlags, key)`. Returns
/// `None` for keys AppKit cannot express as an equivalent (navigation/editing
/// keys); those items still work, they just show no shortcut.
/// NSEventModifierFlags values: Shift 1<<17, Control 1<<18, Option 1<<19,
/// Command 1<<20.
pub fn to_appkit(shortcut: &Shortcut) -> Option<(u64, String)> {
    // Bits match the parse bitmap: 1 Primary, 2 Ctrl, 4 Cmd, 8 Alt, 16 Shift.
    // Primary resolves to Command (rejected mixed with Ctrl/Cmd at parse time).
    let mut flags = 0u64;
    if shortcut.modifiers() & 1 != 0 {
        flags |= 1 << 20;
    }
    if shortcut.modifiers() & 2 != 0 {
        flags |= 1 << 18;
    }
    if shortcut.modifiers() & 4 != 0 {
        flags |= 1 << 20;
    }
    if shortcut.modifiers() & 8 != 0 {
        flags |= 1 << 19;
    }
    if shortcut.modifiers() & 16 != 0 {
        flags |= 1 << 17;
    }
    if shortcut.modifiers() & !31 != 0 {
        return None;
    }
    let key = if shortcut.key().len() == 1 {
        shortcut.key().to_ascii_lowercase()
    } else if let Some(n) = shortcut
        .key()
        .strip_prefix('F')
        .and_then(|n| n.parse::<u32>().ok())
        && (1..=12).contains(&n)
    {
        // NSF1FunctionKey..NSF12FunctionKey (U+F704..U+F70F).
        char::from_u32(0xF704 + n - 1)?.to_string()
    } else {
        match shortcut.key() {
            "Comma" => ",".into(),
            "Enter" => "\r".into(),
            "Tab" => "\t".into(),
            "Space" => " ".into(),
            "Escape" => "\u{1b}".into(),
            "Backspace" => "\u{7f}".into(),
            "Delete" => "\u{F728}".into(),
            "Left" => "\u{F702}".into(),
            "Right" => "\u{F703}".into(),
            "Up" => "\u{F700}".into(),
            "Down" => "\u{F701}".into(),
            _ => return None,
        }
    };
    Some((flags, key))
}

/// Current-target alias for tests and documentation probes.
pub fn to_bridge_for_target(shortcut: &Shortcut, _target: ShortcutTarget) -> String {
    // The bridge always resolves Primary to Command; the target parameter only
    // exists so call sites read uniformly with the other backends.
    let _ = _target;
    to_bridge(shortcut)
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

    #[test]
    fn appkit_equivalent_maps_primary_to_command() {
        let shortcut = Shortcut::parse("Primary+O").unwrap();
        assert_eq!(to_appkit(&shortcut), Some(((1 << 20), "o".into())));
        let shortcut = Shortcut::parse("Primary+Shift+S").unwrap();
        assert_eq!(
            to_appkit(&shortcut),
            Some(((1 << 20) | (1 << 17), "s".into()))
        );
        let shortcut = Shortcut::parse("Primary+Comma").unwrap();
        assert_eq!(to_appkit(&shortcut), Some(((1 << 20), ",".into())));
        let shortcut = Shortcut::parse("Primary+F5").unwrap();
        assert_eq!(to_appkit(&shortcut).map(|(flags, _)| flags), Some(1 << 20));
        assert!(to_appkit(&shortcut).is_some_and(|(_, key)| key.chars().count() == 1));
        assert_eq!(to_appkit(&Shortcut::parse("Primary+Home").unwrap()), None);
    }
}
