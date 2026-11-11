//! SDK-owned menu routing. TextBox owns its standard Ctrl+C/X/V/A keys;
//! menu roles call that same native editor, never the application's argument.
use super::*;
use panechord_runtime::{KeyCode, MenuRole, Shortcut};

/// Lower the shared parser's canonical key, not another shortcut grammar.
pub(in super::super) fn shortcut_native(
    shortcut: &Shortcut,
) -> std::result::Result<(n::VirtualKey, n::VirtualKeyModifiers), String> {
    let name = shortcut.key();
    let key = if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
        KeyCode::from_name(&format!("Key{name}"))
    } else if name.len() == 1 && name.as_bytes()[0].is_ascii_digit() {
        KeyCode::from_name(&format!("Digit{name}"))
    } else {
        KeyCode::from_name(match name {
            "Left" => "ArrowLeft",
            "Right" => "ArrowRight",
            "Up" => "ArrowUp",
            "Down" => "ArrowDown",
            other => other,
        })
    };
    let key = virtual_key(key).ok_or("unsupported native menu shortcut key")?;
    let bits = shortcut.modifiers();
    let mut modifiers = n::VirtualKeyModifiers::None;
    if bits & 3 != 0 {
        modifiers |= n::VirtualKeyModifiers::Control;
    }
    if bits & 4 != 0 {
        modifiers |= n::VirtualKeyModifiers::Windows;
    }
    if bits & 8 != 0 {
        modifiers |= n::VirtualKeyModifiers::Menu;
    }
    if bits & 16 != 0 {
        modifiers |= n::VirtualKeyModifiers::Shift;
    }
    Ok((key, modifiers))
}

fn virtual_key(key: KeyCode) -> Option<n::VirtualKey> {
    // This table takes the runtime's typed keys (also produced by native_keys),
    // not raw shortcut text or layout-dependent Win32 MapVirtualKey results.
    macro_rules! keys {
        ($($runtime:ident => $native:ident),+ $(,)?) => {
            match key { $(KeyCode::$runtime => Some(n::VirtualKey::$native),)+ _ => None }
        };
    }
    if key == KeyCode::Comma {
        // VK_OEM_COMMA, absent as a named member of Windows.System.VirtualKey.
        return Some(n::VirtualKey(0xBC));
    }
    keys! {
        KeyA => A, KeyB => B, KeyC => C, KeyD => D, KeyE => E,
        KeyF => F, KeyG => G, KeyH => H, KeyI => I, KeyJ => J,
        KeyK => K, KeyL => L, KeyM => M, KeyN => N, KeyO => O,
        KeyP => P, KeyQ => Q, KeyR => R, KeyS => S, KeyT => T,
        KeyU => U, KeyV => V, KeyW => W, KeyX => X, KeyY => Y, KeyZ => Z,
        Digit0 => Number0, Digit1 => Number1, Digit2 => Number2, Digit3 => Number3,
        Digit4 => Number4, Digit5 => Number5, Digit6 => Number6, Digit7 => Number7,
        Digit8 => Number8, Digit9 => Number9,
        F1 => F1, F2 => F2, F3 => F3, F4 => F4, F5 => F5, F6 => F6,
        F7 => F7, F8 => F8, F9 => F9, F10 => F10, F11 => F11, F12 => F12,
        Enter => Enter, Tab => Tab, Space => Space, Escape => Escape,
        Backspace => Back, Delete => Delete, Insert => Insert, Home => Home, End => End,
        PageUp => PageUp, PageDown => PageDown,
        ArrowLeft => Left, ArrowRight => Right, ArrowUp => Up, ArrowDown => Down,
    }
}

pub(super) fn shortcut_caption(metadata: &super::super::MenuMetadata) -> String {
    if let Some(chord) = &metadata.chord {
        let bits = chord.modifiers();
        let mut caption = String::new();
        for (present, name) in [
            (bits & 3 != 0, "Ctrl+"),
            (bits & 4 != 0, "Win+"),
            (bits & 8 != 0, "Alt+"),
            (bits & 16 != 0, "Shift+"),
        ] {
            if present {
                caption.push_str(name);
            }
        }
        caption.push_str(chord.key());
        caption
    } else {
        match metadata.role {
            MenuRole::Copy => "Ctrl+C",
            MenuRole::Cut => "Ctrl+X",
            MenuRole::Paste => "Ctrl+V",
            MenuRole::SelectAll => "Ctrl+A",
            _ => "",
        }
        .into()
    }
}

impl Renderer {
    pub(super) fn menu_metadata(
        &self,
        node: &Node,
        key: &windows_reactor::Key,
    ) -> Result<(String, super::super::MenuMetadata)> {
        let Kind::Menu(menu) = &node.kind else {
            return Err(sdk_error("not a menu"));
        };
        // Key is publicly Clone/Eq, but has no public string accessor. Compare
        // opaque keys constructed by the public From<String>, never parse Debug.
        let id = menu
            .native_ids
            .borrow()
            .get(key)
            .cloned()
            .ok_or_else(|| sdk_error("native menu key has no retained ID"))?;
        let metadata = menu
            .metadata
            .borrow()
            .get(&id)
            .cloned()
            .ok_or_else(|| sdk_error("native menu ID has no retained metadata"))?;
        Ok((id, metadata))
    }

    pub(super) fn focus_entry(&self, token: u64) {
        let accepted = !self.retired.get()
            && !self.applying.get()
            && self.objects.borrow().values().any(|o| {
                o.token == token
                    && matches!(o.node.kind, Kind::Entry(_))
                    && self.view.live(&o.node)
                    && self.view.actionable(&o.node)
            });
        if accepted {
            self.last_focused_entry.set(Some(token));
        }
        eprintln!(
            "NATIVE_GOT_FOCUS generation={} token={token} accepted={accepted} actual={:?} retired={} applying={}",
            self.generation,
            self.last_focused_entry.get(),
            self.retired.get(),
            self.applying.get(),
        );
    }

    /// Request only: GotFocus may be queued until a later UI turn.
    pub(crate) fn request_entry_focus(&self, id: &str) -> Result<()> {
        let entry = self.find(id, "control")?;
        if !matches!(entry.node.kind, Kind::Entry(_))
            || !self.view.live(&entry.node)
            || !self.view.actionable(&entry.node)
            || self.retired.get()
            || self.applying.get()
        {
            return Err(sdk_error("native focus target unavailable"));
        }
        let focused = entry.element.Focus(n::FocusState::Programmatic)?;
        eprintln!(
            "NATIVE_FOCUS_REQUEST generation={} node={id} expected={} accepted={focused}",
            self.generation, entry.token
        );
        if !focused {
            return Err(sdk_error("native TextBox Focus rejected"));
        }
        Ok(())
    }

    pub(crate) fn entry_focus_delivered(&self, id: &str) -> Result<bool> {
        let entry = self.find(id, "control")?;
        if self.last_focused_entry.get() != Some(entry.token) {
            return Ok(false);
        }
        self.assert_entry_focus(id)?;
        Ok(true)
    }

    /// Read-only check AFTER yielding: never manufacture the event's token.
    pub(super) fn assert_entry_focus(&self, id: &str) -> Result<()> {
        let entry = self.find(id, "control")?;
        let actual = self.last_focused_entry.get();
        let state = entry.element.FocusState()?;
        let facts = format!(
            "generation={} node={id} expected={} actual={actual:?} state={state:?} loaded={:?} native-enabled={:?} native-visible={:?} node-enabled={} node-visible={} live={} actionable={} retired={} applying={} revokers={} context=[{}]",
            self.generation,
            entry.token,
            entry.element.cast::<n::IFrameworkElement>()?.IsLoaded(),
            entry.element.cast::<n::IControl>()?.IsEnabled(),
            entry.element.Visibility(),
            entry.node.enabled.get(),
            entry.node.visible.get(),
            self.view.live(&entry.node),
            self.view.actionable(&entry.node),
            self.retired.get(),
            self.applying.get(),
            entry.revokers.borrow().len(),
            crate::driver::native_focus_context(self.generation).unwrap_or_else(|e| e.to_string()),
        );
        if actual != Some(entry.token)
            // Focusing an already focused editor may retain Keyboard/Pointer.
            || state == n::FocusState::Unfocused
            || !self.view.live(&entry.node)
            || !self.view.actionable(&entry.node)
            || !entry.element.cast::<n::IFrameworkElement>()?.IsLoaded()?
            || entry.element.XamlRoot()? != self.root()?.XamlRoot()?
        {
            return Err(sdk_error(format!(
                "native TextBox GotFocus token routing: {facts}"
            )));
        }
        eprintln!("NATIVE_FOCUS_AFTER_TURN {facts}");
        Ok(())
    }

    pub(super) fn edit_menu(&self, role: MenuRole) -> std::result::Result<(), String> {
        if self.retired.get() || self.applying.get() || self.view.is_closed() {
            return Err("native editor is unavailable".into());
        }
        let token = self
            .last_focused_entry
            .get()
            .ok_or("no focused native editor")?;
        let object = self
            .objects
            .borrow()
            .values()
            .find(|o| o.token == token)
            .cloned()
            .ok_or("focused native editor was removed")?;
        if !self.view.live(&object.node) || !self.view.actionable(&object.node) {
            return Err("focused native editor or ancestor is hidden/disabled/retired".into());
        }
        let Kind::Entry(binding) = &object.node.kind else {
            return Err("focused target is not an editor".into());
        };
        let work = || -> Result<()> {
            let editor = object.element.cast::<n::ITextBox>()?;
            if !object.element.cast::<n::IControl>()?.IsEnabled()? {
                return Err(sdk_error("native editor is disabled"));
            }
            if matches!(role, MenuRole::Cut | MenuRole::Paste)
                && (!binding.is_writable() || editor.IsReadOnly()?)
            {
                return Err(sdk_error("focused native editor is read-only"));
            }
            if !object.element.cast::<n::IFrameworkElement>()?.IsLoaded()?
                || object.element.XamlRoot()? != self.root()?.XamlRoot()?
            {
                return Err(sdk_error("focused editor is detached or foreign"));
            }
            match role {
                MenuRole::Copy => editor.CopySelectionToClipboard(),
                MenuRole::Cut => editor.CutSelectionToClipboard(),
                MenuRole::Paste => {
                    if !editor.CanPasteClipboardContent()? {
                        return Err(sdk_error("clipboard has no pasteable text"));
                    }
                    editor.PasteFromClipboard()
                }
                MenuRole::SelectAll => editor.SelectAll(),
                _ => Err(sdk_error("unsupported native edit selector")),
            }
        };
        work().map_err(|e| e.to_string())
    }

    pub(super) fn invoke_menu(&self, token: u64, revision: u64) -> Result<bool> {
        if self.retired.get() || self.applying.get() {
            return Ok(false);
        }
        let object = self
            .objects
            .borrow()
            .values()
            .find(|o| o.token == token)
            .cloned();
        let Some(object) = object else {
            return Ok(false);
        };
        if !self.view.live(&object.node)
            || object.node.snapshot_revision.get() != revision
            || object.revision.get() != revision
        {
            return Ok(false);
        }
        let id = object
            .menu_id
            .borrow()
            .clone()
            .ok_or_else(|| sdk_error("menu item ID missing"))?;
        // Expected rejections (disabled command, no editor, readonly writer)
        // are not driver faults. Application panics still reach callback().
        Ok(self.view.activate_menu_node(&object.node, &id).is_ok())
    }

    pub(super) fn sync_accelerators(&self) -> Result<()> {
        let root = self.root()?;
        let mut previous = self.accelerators.take();
        let objects: Vec<_> = self
            .objects
            .borrow()
            .values()
            .filter(|o| o.menu_id.borrow().is_some())
            .cloned()
            .collect();
        let mut next = BTreeMap::new();
        for object in objects {
            let Kind::Menu(menu) = &object.node.kind else {
                continue;
            };
            let id = object.menu_id.borrow().clone().expect("menu item ID");
            let metadata = menu
                .metadata
                .borrow()
                .get(&id)
                .cloned()
                .ok_or_else(|| sdk_error("menu accelerator metadata missing"))?;
            let Some(chord) = metadata.chord else {
                continue;
            };
            let revision = object.node.snapshot_revision.get();
            let old = previous.remove(&object.token);
            let accelerator = if old.as_ref().is_some_and(|a| a.revision == revision) {
                old.expect("matching accelerator")
            } else {
                if let Some(old) = old {
                    self.revoke_accelerator(object.token, old)?;
                }
                let (key, modifiers) = shortcut_native(&chord).map_err(sdk_error)?;
                let native = n::KeyboardAccelerator::new()?;
                native.SetKey(key)?;
                native.SetModifiers(modifiers)?;
                native.SetScopeOwner(&root.cast::<n::DependencyObject>()?)?;
                let generation = self.generation;
                let epoch = self.menu_epoch;
                let token = object.token;
                let revoker = native.Invoked(move |sender, args| {
                    callback(|| {
                        with_renderer(generation, |r| {
                            let current = r.menu_epoch == epoch
                                && r.accelerators.borrow().get(&token).is_some_and(|a| {
                                    a.revision == revision && sender.as_ref() == Some(&a.native)
                                });
                            if current && r.invoke_menu(token, revision)? {
                                args.ok()?.SetHandled(true)?;
                            }
                            Ok(())
                        })
                    })
                })?;
                Accelerator {
                    native,
                    revision,
                    _revoker: revoker,
                }
            };
            accelerator
                .native
                .SetIsEnabled(metadata.enabled && self.view.actionable(&object.node))?;
            next.insert(object.token, accelerator);
        }
        // Disable and revoke old handlers before withdrawing native ownership.
        for (token, old) in previous {
            self.revoke_accelerator(token, old)?;
        }
        let list = root.KeyboardAccelerators()?;
        let mut changed = list.Size()? as usize != next.len();
        if !changed {
            for (index, accelerator) in next.values().enumerate() {
                if list.GetAt(index as u32)? != accelerator.native {
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            list.Clear()?;
            for accelerator in next.values() {
                list.Append(&accelerator.native)?;
            }
        }
        self.accelerators.replace(next);
        Ok(())
    }

    fn revoke_accelerator(&self, token: u64, old: Accelerator) -> Result<()> {
        old.native.SetIsEnabled(false)?;
        if old.native.IsEnabled()? {
            return Err(sdk_error("retired accelerator still enabled"));
        }
        self.last_revoked_accelerator
            .replace(Some((token, old.revision, old.native.clone())));
        drop(old); // EventRevoker removes Invoked before native owner withdrawal.
        self.accelerators_revoked.set(
            self.accelerators_revoked
                .get()
                .checked_add(1)
                .ok_or_else(|| sdk_error("accelerator revocation count exhausted"))?,
        );
        Ok(())
    }

    pub(super) fn menu_assertions(&self) -> Result<()> {
        let root = self.root()?;
        let list = root.KeyboardAccelerators()?;
        let objects: Vec<_> = self
            .objects
            .borrow()
            .values()
            .filter(|o| o.menu_id.borrow().is_some())
            .cloned()
            .collect();
        let accelerators = self.accelerators.borrow();
        let mut count = 0;
        for object in &objects {
            let Kind::Menu(menu) = &object.node.kind else {
                return Err(sdk_error("menu object type"));
            };
            let id = object.menu_id.borrow().clone().expect("menu item ID");
            let metadata = menu
                .metadata
                .borrow()
                .get(&id)
                .cloned()
                .ok_or_else(|| sdk_error("menu metadata absent"))?;
            let enabled = metadata.enabled && self.view.actionable(&object.node);
            if object.element.cast::<n::IControl>()?.IsEnabled()? != enabled
                || object.menu_event.borrow().is_none()
                || object.revision.get() != object.node.snapshot_revision.get()
                || object
                    .element
                    .cast::<n::IMenuFlyoutItem>()?
                    .KeyboardAcceleratorTextOverride()?
                    != shortcut_caption(&metadata)
            {
                return Err(sdk_error(
                    "native menu enabled/caption/revision/revoker readback",
                ));
            }
            if let Some(chord) = &metadata.chord {
                let expected = shortcut_native(chord).map_err(sdk_error)?;
                let accelerator = accelerators
                    .get(&object.token)
                    .ok_or_else(|| sdk_error("accelerator missing"))?;
                if (accelerator.native.Key()?, accelerator.native.Modifiers()?) != expected
                    || accelerator.native.IsEnabled()? != enabled
                    || accelerator.native.ScopeOwner()? != root.cast::<n::DependencyObject>()?
                    || accelerator.revision != object.node.snapshot_revision.get()
                {
                    return Err(sdk_error(
                        "native accelerator key/modifiers/enabled/scope/revision readback",
                    ));
                }
                count += 1;
            } else if accelerators.contains_key(&object.token) {
                return Err(sdk_error(
                    "predefined role stole TextBox keyboard accelerator",
                ));
            }
        }
        if count != accelerators.len() || list.Size()? as usize != count {
            return Err(sdk_error("native root accelerator count/leak"));
        }
        for (index, accelerator) in accelerators.values().enumerate() {
            if list.GetAt(index as u32)? != accelerator.native {
                return Err(sdk_error("native root accelerator ownership"));
            }
        }
        drop(accelerators);
        let revoked = self.last_revoked_accelerator.borrow().clone();
        if let Some((token, revision, native)) = revoked {
            if native.IsEnabled()? || self.invoke_menu(token, revision)? {
                return Err(sdk_error(
                    "revoked native accelerator/stale revision accepted",
                ));
            }
            for index in 0..list.Size()? {
                if list.GetAt(index)? == native {
                    return Err(sdk_error("revoked accelerator retained by root"));
                }
            }
        }
        println!(
            "NATIVE_MENU_ACCELERATORS_PASS generation={} count={count} key-modifiers-enabled-scope-readback=true revoked={} no-physical-input-claim",
            self.generation,
            self.accelerators_revoked.get()
        );
        Ok(())
    }

    /// Explicit asynchronous fixture stage; structural assertions never focus.
    pub(crate) fn menu_edit_assertions(&self, id: &str) -> Result<()> {
        self.assert_entry_focus(id)?;
        if id == "entry" {
            let entry = self.find(id, "control")?;
            let editor = entry.element.cast::<n::ITextBox>()?;
            let selection = (editor.SelectionStart()?, editor.SelectionLength()?);
            self.view.activate_menu("edit-select").map_err(sdk_error)?;
            if editor.SelectedText()? != editor.Text()? {
                return Err(sdk_error("native menu SelectAll selection readback"));
            }
            editor.Select(selection.0, selection.1)?;
        } else if id == "readonly" {
            let readonly = self.find(id, "control")?;
            let editor = readonly.element.cast::<n::ITextBox>()?;
            let original = editor.Text()?;
            self.view.activate_menu("edit-select").map_err(sdk_error)?;
            if editor.SelectedText()? != original {
                return Err(sdk_error("readonly native SelectAll selection readback"));
            }
            if self.view.activate_menu("edit-cut").is_ok()
                || self.view.activate_menu("edit-paste").is_ok()
            {
                return Err(sdk_error("readonly native edit role accepted write"));
            }
            if editor.Text()? != original || editor.SelectedText()? != original {
                return Err(sdk_error("readonly native edit changed text/selection"));
            }
            let old = readonly.node.enabled.replace(false);
            let rejected = self.view.activate_menu("edit-select").is_err();
            readonly.node.enabled.set(old);
            if !rejected {
                return Err(sdk_error("disabled native editor accepted role"));
            }
            let old = readonly.node.visible.replace(false);
            let rejected = self.view.activate_menu("edit-select").is_err();
            readonly.node.visible.set(old);
            if !rejected {
                return Err(sdk_error("hidden native editor accepted role"));
            }
            let parent = self
                .view
                .find(
                    readonly
                        .node
                        .parent
                        .as_deref()
                        .ok_or_else(|| sdk_error("editor parent missing"))?,
                )
                .map_err(sdk_error)?;
            let old = parent.enabled.replace(false);
            let rejected = self.view.activate_menu("edit-select").is_err();
            parent.enabled.set(old);
            if !rejected {
                return Err(sdk_error("disabled editor ancestor accepted role"));
            }
            let old = parent.visible.replace(false);
            let rejected = self.view.activate_menu("edit-select").is_err();
            parent.visible.set(old);
            if !rejected {
                return Err(sdk_error("hidden editor ancestor accepted role"));
            }
            let saved = self.last_focused_entry.replace(Some(u64::MAX));
            let rejected = self.view.activate_menu("edit-select").is_err();
            self.last_focused_entry.set(saved);
            if !rejected {
                return Err(sdk_error("missing editor token accepted role"));
            }
            println!(
                "NATIVE_MENU_EDIT_PASS generation={} actual-GotFocus=true SelectAll-readback=true readonly-write-rejected=true hidden-disabled-missing-rejected=true no-physical-input-claim",
                self.generation
            );
        } else {
            return Err(sdk_error("unknown menu edit fixture stage"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_shortcuts_lower_to_native_keys_and_modifiers() {
        for (raw, key, modifiers) in [
            (
                "Primary+S",
                n::VirtualKey::S,
                n::VirtualKeyModifiers::Control,
            ),
            ("ctrl+q", n::VirtualKey::Q, n::VirtualKeyModifiers::Control),
            (
                "Shift+Alt+F12",
                n::VirtualKey::F12,
                n::VirtualKeyModifiers::Shift | n::VirtualKeyModifiers::Menu,
            ),
            (
                "Cmd+9",
                n::VirtualKey::Number9,
                n::VirtualKeyModifiers::Windows,
            ),
            (
                "Primary+Comma",
                n::VirtualKey(0xBC),
                n::VirtualKeyModifiers::Control,
            ),
            ("Left", n::VirtualKey::Left, n::VirtualKeyModifiers::None),
        ] {
            assert_eq!(
                shortcut_native(&Shortcut::parse(raw).unwrap()).unwrap(),
                (key, modifiers)
            );
        }
        for key in [
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
            "0",
            "9",
            "F1",
            "F12",
        ] {
            assert!(
                shortcut_native(&Shortcut::parse(key).unwrap()).is_ok(),
                "{key}"
            );
        }
        for (scan, key) in [
            (31, n::VirtualKey::S),
            (16, n::VirtualKey::Q),
            (0xe04b, n::VirtualKey::Left),
            (88, n::VirtualKey::F12),
        ] {
            assert_eq!(
                virtual_key(panechord_runtime::native_keys::windows_set1(scan)),
                Some(key)
            );
        }
        assert_eq!(virtual_key(KeyCode::Unidentified), None);
        for invalid in ["", "Primary+Ctrl+S", "Ctrl++S", "Ctrl+KeyS", "F13"] {
            assert!(Shortcut::parse(invalid).is_err());
        }
    }
}
