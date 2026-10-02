//! Shared shortcut inventory, defaults and validation.
//!
//! Bindings are stored as ordinary `app_settings` values by
//! [`SettingsService`](crate::settings_service::SettingsService). This module
//! owns the stable action IDs and rules so the desktop shell does not keep a
//! second, conflicting shortcut catalog.

use crate::settings::HotkeySpec;

pub const KEYBOARD_SHORTCUTS_CHANGED_EVENT: &str = "clipvault://keyboard-shortcuts-changed";

const STORAGE_PREFIX: &str = "keyboard_shortcut_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyboardShortcutId {
    OpenQuickPaste,
    ToggleClipboardCapture,
    FocusMainSearch,
    FocusQuickSearch,
    PreviewSelected,
    EditSelectedText,
    OpenEntryNote,
    OpenHistory,
    CreateTextCapture,
    CopyPlainText,
}

impl KeyboardShortcutId {
    pub const ALL: [Self; 10] = [
        Self::OpenQuickPaste,
        Self::ToggleClipboardCapture,
        Self::FocusMainSearch,
        Self::FocusQuickSearch,
        Self::PreviewSelected,
        Self::EditSelectedText,
        Self::OpenEntryNote,
        Self::OpenHistory,
        Self::CreateTextCapture,
        Self::CopyPlainText,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenQuickPaste => "open_quick_paste",
            Self::ToggleClipboardCapture => "toggle_clipboard_capture",
            Self::FocusMainSearch => "focus_main_search",
            Self::FocusQuickSearch => "focus_quick_search",
            Self::PreviewSelected => "preview_selected",
            Self::EditSelectedText => "edit_selected_text",
            Self::OpenEntryNote => "open_entry_note",
            Self::OpenHistory => "open_history",
            Self::CreateTextCapture => "create_text_capture",
            Self::CopyPlainText => "copy_plain_text",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }

    pub const fn is_global(self) -> bool {
        matches!(self, Self::OpenQuickPaste | Self::ToggleClipboardCapture)
    }

    pub const fn storage_key(self) -> &'static str {
        match self {
            Self::OpenQuickPaste => "keyboard_shortcut_open_quick_paste",
            Self::ToggleClipboardCapture => "keyboard_shortcut_toggle_clipboard_capture",
            Self::FocusMainSearch => "keyboard_shortcut_focus_main_search",
            Self::FocusQuickSearch => "keyboard_shortcut_focus_quick_search",
            Self::PreviewSelected => "keyboard_shortcut_preview_selected",
            Self::EditSelectedText => "keyboard_shortcut_edit_selected_text",
            Self::OpenEntryNote => "keyboard_shortcut_open_entry_note",
            Self::OpenHistory => "keyboard_shortcut_open_history",
            Self::CreateTextCapture => "keyboard_shortcut_create_text_capture",
            Self::CopyPlainText => "keyboard_shortcut_copy_plain_text",
        }
    }

    fn contexts(self) -> &'static [&'static str] {
        match self {
            Self::OpenQuickPaste | Self::ToggleClipboardCapture => &["global"],
            Self::FocusMainSearch
            | Self::OpenEntryNote
            | Self::OpenHistory
            | Self::CreateTextCapture => &["main"],
            Self::FocusQuickSearch | Self::CopyPlainText => &["quick_paste"],
            Self::PreviewSelected | Self::EditSelectedText => &["main", "quick_paste"],
        }
    }
}

pub fn default_keyboard_shortcuts() -> Vec<HotkeySpec> {
    use KeyboardShortcutId as Id;

    let make = |id: Id, key: &str, shift: bool, alt: bool| HotkeySpec {
        id: id.as_str().to_string(),
        key: key.to_string(),
        // `cmd_or_ctrl` is the normalized primary modifier. The global
        // adapter resolves it to Command on macOS and Control on Linux.
        cmd_or_ctrl: true,
        shift,
        alt,
        meta: false,
    };

    vec![
        make(Id::OpenQuickPaste, "v", true, false),
        make(Id::ToggleClipboardCapture, "b", true, true),
        make(Id::FocusMainSearch, "f", false, false),
        make(Id::FocusQuickSearch, "k", false, false),
        make(Id::PreviewSelected, "enter", false, false),
        make(Id::EditSelectedText, "e", false, false),
        make(Id::OpenEntryNote, "n", true, false),
        make(Id::OpenHistory, "h", true, false),
        make(Id::CreateTextCapture, "n", false, false),
        HotkeySpec {
            id: Id::CopyPlainText.as_str().to_string(),
            key: "enter".to_string(),
            cmd_or_ctrl: false,
            shift: true,
            alt: false,
            meta: false,
        },
    ]
}

pub fn validate_shortcut(spec: &HotkeySpec) -> Result<KeyboardShortcutId, &'static str> {
    let Some(id) = KeyboardShortcutId::parse(spec.id.trim()) else {
        return Err("unknown_shortcut");
    };
    let key = spec.key.trim().to_ascii_lowercase();
    let supported = key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric()
        || matches!(key.as_str(), "enter" | "escape" | "space");
    if !supported {
        return Err("unsupported_key");
    }
    if !spec.cmd_or_ctrl && !spec.shift && !spec.alt && !spec.meta {
        return Err("modifier_required");
    }
    Ok(id)
}

pub fn same_shortcut(left: &HotkeySpec, right: &HotkeySpec) -> bool {
    left.key.trim().eq_ignore_ascii_case(right.key.trim())
        && left.cmd_or_ctrl == right.cmd_or_ctrl
        && left.shift == right.shift
        && left.alt == right.alt
        && left.meta == right.meta
}

/// Return the ID of an action that would receive the same chord in a
/// simultaneously active context. Global shortcuts conflict with every local
/// action because the platform can consume their key event first.
pub fn shortcut_conflict<'a>(
    proposed: &HotkeySpec,
    current: &'a [HotkeySpec],
) -> Option<KeyboardShortcutId> {
    let proposed_id = KeyboardShortcutId::parse(proposed.id.trim())?;
    current.iter().find_map(|existing| {
        let existing_id = KeyboardShortcutId::parse(existing.id.trim())?;
        if existing_id == proposed_id || !same_shortcut(proposed, existing) {
            return None;
        }
        let contexts_overlap = proposed_id.is_global()
            || existing_id.is_global()
            || proposed_id
                .contexts()
                .iter()
                .any(|context| existing_id.contexts().contains(context));
        contexts_overlap.then_some(existing_id)
    })
}

pub const fn shortcut_storage_prefix() -> &'static str {
    STORAGE_PREFIX
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyboardShortcutId as Id;

    #[test]
    fn defaults_cover_all_actions_and_keep_platform_primary_modifier() {
        let defaults = default_keyboard_shortcuts();
        let mac = &defaults;
        let linux = &defaults;
        assert_eq!(mac.len(), Id::ALL.len());
        assert!(
            mac.iter()
                .find(|s| s.id == Id::OpenQuickPaste.as_str())
                .unwrap()
                .cmd_or_ctrl
        );
        assert!(
            linux
                .iter()
                .find(|s| s.id == Id::OpenQuickPaste.as_str())
                .unwrap()
                .cmd_or_ctrl
        );
        assert!(
            mac.iter()
                .find(|s| s.id == Id::CopyPlainText.as_str())
                .unwrap()
                .shift
        );
        assert_eq!(Id::ALL.map(Id::as_str).len(), 10);
        for (id, key, shift) in [
            (Id::OpenEntryNote, "n", true),
            (Id::OpenHistory, "h", true),
            (Id::CreateTextCapture, "n", false),
        ] {
            let binding = defaults
                .iter()
                .find(|binding| binding.id == id.as_str())
                .expect("default action binding");
            assert_eq!(binding.key, key);
            assert_eq!(binding.shift, shift);
            assert!(binding.cmd_or_ctrl);
        }
        assert!(defaults
            .iter()
            .all(|binding| shortcut_conflict(binding, &defaults).is_none()));
    }

    #[test]
    fn validation_requires_supported_key_and_a_modifier() {
        let mut binding = default_keyboard_shortcuts().remove(0);
        assert!(validate_shortcut(&binding).is_ok());
        binding.key = "f13".into();
        assert_eq!(validate_shortcut(&binding), Err("unsupported_key"));
        binding.key = "x".into();
        binding.cmd_or_ctrl = false;
        binding.shift = false;
        assert_eq!(validate_shortcut(&binding), Err("modifier_required"));

        binding.cmd_or_ctrl = true;
        binding.meta = true;
        assert!(validate_shortcut(&binding).is_ok());
    }

    #[test]
    fn duplicate_chords_are_allowed_only_in_disjoint_contexts() {
        let defaults = default_keyboard_shortcuts();
        let preview = defaults
            .iter()
            .find(|s| s.id == Id::PreviewSelected.as_str())
            .unwrap();
        assert_eq!(shortcut_conflict(preview, &defaults), None);

        let mut proposed = preview.clone();
        proposed.id = Id::FocusQuickSearch.as_str().to_string();
        assert_eq!(
            shortcut_conflict(&proposed, &defaults),
            Some(Id::PreviewSelected)
        );

        let mut proposed = preview.clone();
        proposed.id = Id::ToggleClipboardCapture.as_str().to_string();
        assert_eq!(
            shortcut_conflict(&proposed, &defaults),
            Some(Id::PreviewSelected)
        );
    }
}
