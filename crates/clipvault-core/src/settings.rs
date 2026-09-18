//! Local settings aggregate and validation logic.
//!
//! [`Settings`] is the in-memory representation of everything the
//! `privacy-settings` capability persists through
//! `app_settings`. The structure is intentionally flat and
//! serialisable so the Tauri command surface can clone it across
//! threads without locking.
//!
//! [`SettingsUpdate`] is the value the frontend submits; it goes
//! through [`SettingsUpdate::validate`] which returns a typed
//! [`ValidationError`] so the shell can show a specific error to the
//! user without leaking the rejected value.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::management::RetentionPolicy;

const HOTKEY_SETTING_KEY: &str = "quick_paste_hotkey";

pub const MAX_IDENTIFIER_LENGTH: usize = 128;
/// Maximum length of the validated local peer display name. The
/// limit is intentionally generous (64 Unicode scalar values) so
/// the user can write in their own language without surprises but
/// not so large that the value could break the LAN advertisement
/// records the future `local-peer-discovery` change will own.
pub const MAX_PEER_DISPLAY_NAME_LENGTH: usize = 64;

pub const LOCAL_PEER_DISPLAY_NAME_KEY: &str = "local_peer_display_name";

/// Persistence key for the opt-in `local_peer_sharing_enabled`
/// toggle the `local-peer-discovery` change introduces. Stored as
/// the literal strings `"true"` / `"false"` in `app_settings` so
/// the value round-trips through the same `set`/`get` path every
/// other key uses.
pub const LOCAL_PEER_SHARING_ENABLED_KEY: &str = "local_peer_sharing_enabled";

/// Parse the persisted value into a boolean. Missing / empty
/// strings collapse to the documented default (`false`); any other
/// non-`"true"` value is treated as `false` so a manually edited
/// `app_settings` row can never accidentally enable sharing.
pub fn parse_local_peer_sharing_enabled(raw: Option<&str>) -> bool {
    matches!(raw, Some(value) if value.trim().eq_ignore_ascii_case("true"))
}

/// Render a boolean as the persisted value. Always emits the
/// literal `"true"` or `"false"` string so the round-trip is
/// deterministic and the value can be inspected with `sqlite3`.
pub fn local_peer_sharing_enabled_value(enabled: bool) -> &'static str {
    if enabled {
        "true"
    } else {
        "false"
    }
}

/// Aggregate of every setting persisted for the MVP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Settings {
    pub retention: RetentionPolicy,
    pub ignored_apps: Vec<String>,
    pub quick_paste_hotkey: Option<HotkeySpec>,
    /// Validated visible device name for the local peer identity
    /// foundation. Persisted in `app_settings`; the cryptographic
    /// identity itself is derived from the public key and lives in
    /// the platform secure store.
    pub local_peer_display_name: Option<String>,
    /// Opt-in `Compartir en red local` toggle introduced by the
    /// `local-peer-discovery` change. Defaults to `false` so a
    /// fresh install never announces itself on the LAN; turning it
    /// on requires a reachable peer identity (the secure store
    /// must report `Ok`).
    pub local_peer_sharing_enabled: bool,
}

impl Settings {
    /// Defaults applied when no persisted value exists.
    pub fn defaults() -> Self {
        Self {
            retention: RetentionPolicy::Days30,
            ignored_apps: Vec::new(),
            quick_paste_hotkey: None,
            local_peer_display_name: None,
            // Sharing stays disabled by default — the
            // `local-peer-discovery` change ships behind an opt-in
            // toggle so the very first launch never announces
            // itself on the LAN.
            local_peer_sharing_enabled: false,
        }
    }
}

/// Hotkey specification stored in `app_settings`. Mirrors
/// [`clipvault_platform::HotkeyBinding`] but stays in the core layer so
/// `clipvault-db` and the `app_settings` table do not depend on
/// `clipvault-platform`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HotkeySpec {
    pub id: String,
    pub key: String,
    pub cmd_or_ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl HotkeySpec {
    /// Stable representation persisted in `app_settings`.
    pub fn as_setting_value(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        if self.cmd_or_ctrl {
            parts.push("cmd_or_ctrl");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.meta {
            parts.push("meta");
        }
        if !self.key.is_empty() {
            parts.push(self.key.as_str());
        }
        if parts.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                out.push('+');
            }
            out.push_str(part);
        }
        out
    }

    /// Parse a stored value back into a `HotkeySpec`. Returns `None`
    /// when the value is empty (`Some(None)` becomes `None` in the
    /// aggregate). Returns `Some(Err(message))` when the value cannot
    /// be parsed.
    pub fn parse(raw: Option<&str>) -> Option<Result<Self, String>> {
        let raw = raw?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut cmd_or_ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut meta = false;
        let mut key: Option<String> = None;
        for token in trimmed.split('+').map(str::trim).filter(|s| !s.is_empty()) {
            match token {
                "cmd_or_ctrl" | "ctrl" | "cmd" => cmd_or_ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                "meta" | "super" | "win" => meta = true,
                other => {
                    if key.is_some() {
                        return Some(Err(format!("hotkey has more than one key: {raw}")));
                    }
                    key = Some(other.to_ascii_lowercase());
                }
            }
        }
        let key = match key {
            Some(k) => k,
            None => return Some(Err(format!("hotkey is missing a key: {raw}"))),
        };
        if !cmd_or_ctrl && !shift && !alt && !meta {
            return Some(Err(format!(
                "hotkey must declare at least one modifier: {raw}"
            )));
        }
        Some(Ok(HotkeySpec {
            id: HOTKEY_SETTING_KEY.to_string(),
            key,
            cmd_or_ctrl,
            shift,
            alt,
            meta,
        }))
    }
}

/// Stable identifier for persisted entries.
pub const HOTKEY_SETTING_STORAGE_KEY: &str = "quick_paste_hotkey";

/// Update payload submitted by the UI. Each field is optional so a
/// partial update does not need to round-trip through every key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettingsUpdate {
    pub retention: Option<RetentionPolicy>,
    pub ignored_apps_add: Vec<String>,
    pub ignored_apps_remove: Vec<String>,
    pub quick_paste_hotkey: Option<Option<HotkeySpec>>,
    /// Updated local peer display name. The outer `Option` signals
    /// whether the caller wants to touch the field; the inner
    /// `Option` carries the value (or `None` to clear it). The
    /// value passes through [`validate_peer_display_name`] before
    /// being persisted; the caller's rejected value never reaches
    /// the storage layer.
    pub local_peer_display_name: Option<Option<String>>,
    /// Updated opt-in `Compartir en red local` toggle. The outer
    /// `Option` mirrors the rest of the update payload — `Some(_)`
    /// means the caller wants to touch the field, `None` means leave
    /// it alone. The inner `bool` carries the requested state. The
    /// validator refuses `true` when no reachable peer identity
    /// exists; the typed [`ValidationCode`] lets the shell render
    /// the matching copy without parsing free-form strings.
    pub local_peer_sharing_enabled: Option<bool>,
}

impl SettingsUpdate {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.retention.is_none()
            && self.ignored_apps_add.is_empty()
            && self.ignored_apps_remove.is_empty()
            && self.quick_paste_hotkey.is_none()
            && self.local_peer_display_name.is_none()
            && self.local_peer_sharing_enabled.is_none()
    }

    /// Validate the update against the current [`Settings`]. Returns
    /// either the merged settings or a typed [`ValidationError`].
    pub fn validate(&self, current: &Settings) -> Result<Settings, ValidationError> {
        if self.quick_paste_hotkey.is_none()
            && self.retention.is_none()
            && self.ignored_apps_add.is_empty()
            && self.ignored_apps_remove.is_empty()
            && self.local_peer_display_name.is_none()
            && self.local_peer_sharing_enabled.is_none()
        {
            return Err(ValidationError::empty());
        }
        if let Some(Some(spec)) = self.quick_paste_hotkey.as_ref() {
            if spec.id.trim().is_empty() {
                return Err(ValidationError::invalid_hotkey());
            }
            if spec.key.trim().is_empty() {
                return Err(ValidationError::invalid_hotkey());
            }
            if !spec.cmd_or_ctrl && !spec.shift && !spec.alt && !spec.meta {
                return Err(ValidationError::invalid_hotkey());
            }
        }

        let mut next = current.clone();
        if let Some(policy) = self.retention {
            next.retention = policy;
        }

        for id in &self.ignored_apps_remove {
            next.ignored_apps.retain(|existing| existing != id);
        }

        for id in &self.ignored_apps_add {
            validate_identifier(id)?;
            if !next.ignored_apps.iter().any(|existing| existing == id) {
                next.ignored_apps.push(id.clone());
            }
        }

        if let Some(maybe) = &self.quick_paste_hotkey {
            next.quick_paste_hotkey = maybe.clone();
        }

        if let Some(maybe) = &self.local_peer_display_name {
            next.local_peer_display_name = maybe
                .as_ref()
                .map(|raw| validate_peer_display_name(raw))
                .transpose()?;
        }

        if let Some(enabled) = self.local_peer_sharing_enabled {
            next.local_peer_sharing_enabled = enabled;
        }

        Ok(next)
    }
}

fn validate_identifier(id: &str) -> Result<(), ValidationError> {
    if id.trim().is_empty() {
        return Err(ValidationError::invalid_identifier());
    }
    if id.trim().len() > MAX_IDENTIFIER_LENGTH {
        return Err(ValidationError::identifier_too_long());
    }
    if id.chars().any(char::is_whitespace) {
        return Err(ValidationError::invalid_identifier());
    }
    Ok(())
}

/// Validate the user-supplied local peer display name. Returns
/// the trimmed value when it passes; rejects empty (after trim),
/// control characters and values longer than
/// [`MAX_PEER_DISPLAY_NAME_LENGTH`] Unicode scalar values.
pub fn validate_peer_display_name(raw: &str) -> Result<String, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::invalid_peer_display_name());
    }
    if trimmed.chars().any(is_invalid_display_char) {
        return Err(ValidationError::invalid_peer_display_name());
    }
    if trimmed.chars().count() > MAX_PEER_DISPLAY_NAME_LENGTH {
        return Err(ValidationError::peer_display_name_too_long());
    }
    Ok(trimmed.to_string())
}

fn is_invalid_display_char(c: char) -> bool {
    // Reject ASCII control chars plus a few common confusables
    // (zero-width spaces, BOM, line/paragraph separators, …) so a
    // malicious value cannot render an empty row in the UI.
    if c.is_control() {
        return true;
    }
    matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}')
}

/// Typed validation errors the shell surfaces back to the user. The
/// `field` is always present so the frontend can highlight the offending
/// input; the `code` is a stable identifier used by tests and metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub code: ValidationCode,
    pub field: &'static str,
    pub message: String,
}

impl ValidationError {
    pub fn empty() -> Self {
        Self {
            code: ValidationCode::Empty,
            field: "settings",
            message: "no settings to update".to_string(),
        }
    }

    pub fn invalid_retention() -> Self {
        Self {
            code: ValidationCode::InvalidRetention,
            field: "retention",
            message: "retention value is not supported".to_string(),
        }
    }

    pub fn invalid_hotkey() -> Self {
        Self {
            code: ValidationCode::InvalidHotkey,
            field: "quick_paste_hotkey",
            message: "hotkey must declare a modifier and a key".to_string(),
        }
    }

    pub fn invalid_identifier() -> Self {
        Self {
            code: ValidationCode::InvalidIdentifier,
            field: "ignored_apps",
            message: "identifier must not be empty or contain whitespace".to_string(),
        }
    }

    pub fn identifier_too_long() -> Self {
        Self {
            code: ValidationCode::IdentifierTooLong,
            field: "ignored_apps",
            message: format!(
                "identifier must be at most {} characters",
                MAX_IDENTIFIER_LENGTH
            ),
        }
    }

    pub fn invalid_peer_display_name() -> Self {
        Self {
            code: ValidationCode::InvalidPeerDisplayName,
            field: "local_peer_display_name",
            message: "name must not be empty or contain control characters".to_string(),
        }
    }

    pub fn peer_display_name_too_long() -> Self {
        Self {
            code: ValidationCode::PeerDisplayNameTooLong,
            field: "local_peer_display_name",
            message: format!(
                "name must be at most {} characters",
                MAX_PEER_DISPLAY_NAME_LENGTH
            ),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationCode {
    Empty,
    InvalidRetention,
    InvalidHotkey,
    InvalidIdentifier,
    IdentifierTooLong,
    InvalidPeerDisplayName,
    PeerDisplayNameTooLong,
}

impl ValidationCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidationCode::Empty => "empty",
            ValidationCode::InvalidRetention => "invalid_retention",
            ValidationCode::InvalidHotkey => "invalid_hotkey",
            ValidationCode::InvalidIdentifier => "invalid_identifier",
            ValidationCode::IdentifierTooLong => "identifier_too_long",
            ValidationCode::InvalidPeerDisplayName => "invalid_peer_display_name",
            ValidationCode::PeerDisplayNameTooLong => "peer_display_name_too_long",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current() -> Settings {
        Settings::defaults()
    }

    #[test]
    fn empty_update_is_rejected() {
        let update = SettingsUpdate::empty();
        let error = update
            .validate(&current())
            .expect_err("empty update must fail validation");
        assert_eq!(error.code, ValidationCode::Empty);
    }

    #[test]
    fn retention_policy_round_trips_through_validate() {
        let update = SettingsUpdate {
            retention: Some(RetentionPolicy::Days7),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current()).expect("valid");
        assert_eq!(next.retention, RetentionPolicy::Days7);
    }

    #[test]
    fn ignored_app_identifier_is_rejected_when_empty() {
        let update = SettingsUpdate {
            ignored_apps_add: vec![String::new()],
            ..SettingsUpdate::default()
        };
        let error = update
            .validate(&current())
            .expect_err("empty identifier must fail");
        assert_eq!(error.code, ValidationCode::InvalidIdentifier);
        assert_eq!(error.field, "ignored_apps");
    }

    #[test]
    fn ignored_app_identifier_is_rejected_with_whitespace() {
        let update = SettingsUpdate {
            ignored_apps_add: vec!["com apple".to_string()],
            ..SettingsUpdate::default()
        };
        let error = update
            .validate(&current())
            .expect_err("whitespace must fail");
        assert_eq!(error.code, ValidationCode::InvalidIdentifier);
    }

    #[test]
    fn ignored_app_identifier_too_long_is_rejected() {
        let id = "a".repeat(MAX_IDENTIFIER_LENGTH + 1);
        let update = SettingsUpdate {
            ignored_apps_add: vec![id],
            ..SettingsUpdate::default()
        };
        let error = update
            .validate(&current())
            .expect_err("oversized identifier must fail");
        assert_eq!(error.code, ValidationCode::IdentifierTooLong);
    }

    #[test]
    fn ignored_app_identifier_is_accepted() {
        let update = SettingsUpdate {
            ignored_apps_add: vec!["com.apple.Terminal".to_string()],
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current()).expect("valid");
        assert_eq!(next.ignored_apps, vec!["com.apple.Terminal".to_string()]);
    }

    #[test]
    fn ignored_app_remove_prunes_existing_entries() {
        let mut current = Settings::defaults();
        current.ignored_apps.push("com.apple.Terminal".to_string());
        current.ignored_apps.push("firefox".to_string());
        let update = SettingsUpdate {
            ignored_apps_remove: vec!["firefox".to_string()],
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert_eq!(next.ignored_apps, vec!["com.apple.Terminal".to_string()]);
    }

    #[test]
    fn hotkey_without_modifier_is_rejected() {
        let update = SettingsUpdate {
            quick_paste_hotkey: Some(Some(HotkeySpec {
                id: "quick_paste".into(),
                key: "v".into(),
                cmd_or_ctrl: false,
                shift: false,
                alt: false,
                meta: false,
            })),
            ..SettingsUpdate::default()
        };
        let error = update
            .validate(&current())
            .expect_err("hotkey without modifier must fail");
        assert_eq!(error.code, ValidationCode::InvalidHotkey);
    }

    #[test]
    fn hotkey_with_modifier_and_key_is_accepted() {
        let update = SettingsUpdate {
            quick_paste_hotkey: Some(Some(HotkeySpec {
                id: "quick_paste".into(),
                key: "v".into(),
                cmd_or_ctrl: true,
                shift: true,
                alt: false,
                meta: false,
            })),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current()).expect("valid");
        assert!(next.quick_paste_hotkey.is_some());
    }

    #[test]
    fn hotkey_can_be_cleared_by_setting_none() {
        let mut current = Settings::defaults();
        current.quick_paste_hotkey = Some(HotkeySpec {
            id: "quick_paste".into(),
            key: "v".into(),
            cmd_or_ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        });
        let update = SettingsUpdate {
            quick_paste_hotkey: Some(None),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert!(next.quick_paste_hotkey.is_none());
    }

    #[test]
    fn hotkey_spec_round_trips_through_storage_value() {
        let spec = HotkeySpec {
            id: "quick_paste".into(),
            key: "v".into(),
            cmd_or_ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        };
        let parsed = HotkeySpec::parse(Some(&spec.as_setting_value()))
            .expect("parsed")
            .expect("ok");
        assert_eq!(parsed.key, "v");
        assert!(parsed.cmd_or_ctrl);
        assert!(parsed.shift);
    }

    #[test]
    fn hotkey_spec_parse_rejects_keyless_value() {
        let error = HotkeySpec::parse(Some("cmd_or_ctrl+shift"))
            .expect("parsed")
            .expect_err("missing key");
        assert!(error.contains("missing a key"));
    }

    #[test]
    fn hotkey_spec_parse_rejects_empty_value_as_none() {
        assert!(HotkeySpec::parse(Some("")).is_none());
        assert!(HotkeySpec::parse(None).is_none());
    }

    #[test]
    fn settings_serialises_with_snake_case_retention() {
        // Bug 1 regression: the Settings aggregate the frontend reads
        // through `clipvault_settings_get` must carry the retention
        // policy as a flat snake_case string the TypeScript literal
        // union accepts. A regression here would surface as
        // "TypeError: … is not assignable to RetentionPolicy" in the
        // Svelte panel.
        let settings = Settings {
            retention: RetentionPolicy::Days90,
            ignored_apps: vec!["com.apple.Terminal".to_string()],
            quick_paste_hotkey: None,
            local_peer_display_name: None,
            local_peer_sharing_enabled: false,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"retention\":\"days_90\""), "got {json}");
        assert!(
            json.contains("\"ignored_apps\":[\"com.apple.Terminal\"]"),
            "got {json}"
        );
        assert!(
            json.contains("\"local_peer_sharing_enabled\":false"),
            "settings payload must expose the opt-in toggle, got {json}"
        );
    }

    #[test]
    fn settings_update_deserialises_plain_retention_string() {
        // Frontend sends `{ "retention": "forever" }`; the server
        // must accept the bare string instead of demanding the
        // `{"kind": "forever"}` envelope.
        let raw = r#"{"retention":"forever","ignored_apps_add":[],"ignored_apps_remove":[]}"#;
        let update: SettingsUpdate = serde_json::from_str(raw).expect("parse");
        assert_eq!(update.retention, Some(RetentionPolicy::Forever));
    }

    #[test]
    fn settings_update_round_trips_through_serde() {
        let update = SettingsUpdate {
            retention: Some(RetentionPolicy::Days7),
            ignored_apps_add: vec!["com.apple.Terminal".to_string()],
            ignored_apps_remove: vec![],
            quick_paste_hotkey: None,
            local_peer_display_name: Some(Some("Studio".to_string())),
            local_peer_sharing_enabled: Some(true),
        };
        let json = serde_json::to_string(&update).unwrap();
        let parsed: SettingsUpdate = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(parsed, update);
    }

    #[test]
    fn settings_serialises_with_local_peer_display_name() {
        // The frontend relies on `local_peer_display_name` being
        // present in the `Settings` payload even when it is `null`
        // so the Settings panel can render the Identity section
        // with a stable shape.
        let settings = Settings {
            local_peer_display_name: Some("Studio".to_string()),
            ..Settings::defaults()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(
            json.contains("\"local_peer_display_name\":\"Studio\""),
            "got {json}"
        );
    }

    #[test]
    fn peer_display_name_trims_surrounding_whitespace() {
        let update = SettingsUpdate {
            local_peer_display_name: Some(Some("  Studio  ".to_string())),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&Settings::defaults()).expect("valid");
        assert_eq!(next.local_peer_display_name.as_deref(), Some("Studio"));
    }

    #[test]
    fn peer_display_name_rejects_empty_value() {
        let update = SettingsUpdate {
            local_peer_display_name: Some(Some("   ".to_string())),
            ..SettingsUpdate::default()
        };
        let error = update
            .validate(&Settings::defaults())
            .expect_err("empty must fail");
        assert_eq!(error.code, ValidationCode::InvalidPeerDisplayName);
    }

    #[test]
    fn peer_display_name_rejects_control_characters() {
        for bad in ["\u{0001}evil", "line1\nline2", "tab\there"] {
            let update = SettingsUpdate {
                local_peer_display_name: Some(Some(bad.to_string())),
                ..SettingsUpdate::default()
            };
            let error = update
                .validate(&Settings::defaults())
                .expect_err("control must fail");
            assert_eq!(error.code, ValidationCode::InvalidPeerDisplayName);
        }
    }

    #[test]
    fn peer_display_name_rejects_oversized_value() {
        let bad = "a".repeat(MAX_PEER_DISPLAY_NAME_LENGTH + 1);
        let update = SettingsUpdate {
            local_peer_display_name: Some(Some(bad)),
            ..SettingsUpdate::default()
        };
        let error = update
            .validate(&Settings::defaults())
            .expect_err("oversized must fail");
        assert_eq!(error.code, ValidationCode::PeerDisplayNameTooLong);
    }

    #[test]
    fn peer_display_name_can_be_cleared() {
        let mut current = Settings::defaults();
        current.local_peer_display_name = Some("Studio".to_string());
        let update = SettingsUpdate {
            local_peer_display_name: Some(None),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert!(next.local_peer_display_name.is_none());
    }

    #[test]
    fn peer_display_name_does_not_change_other_settings() {
        let mut current = Settings::defaults();
        current.ignored_apps.push("com.apple.Terminal".to_string());
        current.quick_paste_hotkey = Some(HotkeySpec {
            id: "quick_paste".into(),
            key: "v".into(),
            cmd_or_ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        });
        let update = SettingsUpdate {
            local_peer_display_name: Some(Some("Studio".to_string())),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert_eq!(next.ignored_apps, current.ignored_apps);
        assert_eq!(next.quick_paste_hotkey, current.quick_paste_hotkey);
        assert_eq!(next.local_peer_display_name.as_deref(), Some("Studio"));
    }

    #[test]
    fn local_peer_sharing_enabled_defaults_to_false() {
        // The `local-peer-discovery` change ships behind an opt-in
        // toggle. A fresh install MUST NOT auto-enable sharing.
        assert!(!Settings::defaults().local_peer_sharing_enabled);
    }

    #[test]
    fn local_peer_sharing_enabled_can_be_toggled_through_settings_update() {
        let mut current = Settings::defaults();
        assert!(!current.local_peer_sharing_enabled);
        let update = SettingsUpdate {
            local_peer_sharing_enabled: Some(true),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert!(next.local_peer_sharing_enabled);
        current = next;
        let update = SettingsUpdate {
            local_peer_sharing_enabled: Some(false),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert!(!next.local_peer_sharing_enabled);
    }

    #[test]
    fn local_peer_sharing_enabled_omitted_keeps_current_value() {
        // A partial update that does not touch the toggle must leave
        // the previous value intact: the field is opt-in and a UI
        // round-trip that re-sends every other key would otherwise
        // silently flip sharing back to the default.
        let mut current = Settings::defaults();
        current.local_peer_sharing_enabled = true;
        let update = SettingsUpdate {
            local_peer_display_name: Some(Some("Studio".to_string())),
            ..SettingsUpdate::default()
        };
        let next = update.validate(&current).expect("valid");
        assert!(next.local_peer_sharing_enabled);
        assert_eq!(next.local_peer_display_name.as_deref(), Some("Studio"));
    }

    #[test]
    fn parse_local_peer_sharing_enabled_recognises_only_true() {
        // The parser is intentionally conservative: anything other
        // than a literal `true` (case-insensitive, trimmed) falls
        // back to `false` so a manually edited `app_settings` row
        // can never accidentally enable sharing.
        assert!(parse_local_peer_sharing_enabled(Some("true")));
        assert!(parse_local_peer_sharing_enabled(Some("TRUE")));
        assert!(parse_local_peer_sharing_enabled(Some(" true ")));
        for falsey in [
            None,
            Some(""),
            Some("false"),
            Some("FALSE"),
            Some("yes"),
            Some("1"),
            Some("garbage"),
        ] {
            assert!(
                !parse_local_peer_sharing_enabled(falsey),
                "value {falsey:?} must parse as disabled"
            );
        }
    }

    #[test]
    fn local_peer_sharing_enabled_value_round_trips_through_parser() {
        // The encoder and decoder must agree: every value the
        // encoder emits must be accepted by the parser and vice
        // versa. This is the round-trip the persistence layer
        // relies on, so a future contributor who tweaks the encoder
        // without updating the parser sees a test failure instead
        // of a silent sharing toggle.
        for raw in [true, false] {
            let encoded = local_peer_sharing_enabled_value(raw);
            assert_eq!(parse_local_peer_sharing_enabled(Some(encoded)), raw);
        }
    }
}
