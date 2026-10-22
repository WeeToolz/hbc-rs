//! Persistent settings model and XML codec.

use alloc::{format, string::String};

use crate::metadata::{UserSettings, parse_settings_xml, truncate_utf8_bytes};

pub const SETTINGS_PATH: &str = "settings.xml";
pub const MAX_SETTINGS_XML_SIZE: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsInitializationError<E> {
    Load(E),
    Save(E),
}

/// Load persisted settings. Save the HBC defaults only when the caller confirms
/// that the settings file itself is missing.
pub fn load_or_create<E>(
    defaults: UserSettings,
    load: impl FnOnce() -> Result<UserSettings, E>,
    is_missing: impl FnOnce(&E) -> bool,
    save: impl FnOnce(&UserSettings) -> Result<(), E>,
) -> Result<UserSettings, SettingsInitializationError<E>> {
    match load() {
        Ok(settings) => Ok(settings),
        Err(error) if is_missing(&error) => {
            save(&defaults).map_err(SettingsInitializationError::Save)?;
            Ok(defaults)
        }
        Err(error) => Err(SettingsInitializationError::Load(error)),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsCodecError {
    InvalidXml(String),
    TooLarge,
    Allocation,
}

/// Encode the settings fields that HBC persists.
pub fn encode_settings_xml(settings: &UserSettings) -> Result<String, SettingsCodecError> {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hbc><device>{}</device><sort_order>{}</sort_order><browse_mode>{}</browse_mode>",
        settings.device, settings.sort_order, settings.browse_mode
    );
    if let Some(selected_app) = settings
        .selected_app
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|value| truncate_utf8_bytes(value, 63))
    {
        xml.push_str("<app_sel>");
        for character in selected_app.chars() {
            match character {
                '&' => xml.push_str("&amp;"),
                '<' => xml.push_str("&lt;"),
                '>' => xml.push_str("&gt;"),
                '\'' => xml.push_str("&apos;"),
                '"' => xml.push_str("&quot;"),
                c => xml.push(c),
            }
        }
        xml.push_str("</app_sel>");
    }
    xml.push_str("</hbc>");
    if xml.len() > MAX_SETTINGS_XML_SIZE - 1 {
        return Err(SettingsCodecError::TooLarge);
    }
    Ok(xml)
}

/// Parse and validate the settings fields stored by HBC.
pub fn decode_settings_xml(bytes: &[u8]) -> Result<UserSettings, SettingsCodecError> {
    if bytes.is_empty() || bytes.len() > MAX_SETTINGS_XML_SIZE {
        return Err(SettingsCodecError::TooLarge);
    }
    let xml = core::str::from_utf8(bytes)
        .map_err(|_| SettingsCodecError::InvalidXml("settings are not UTF-8".into()))?;
    let mut settings = parse_settings_xml(xml).map_err(SettingsCodecError::InvalidXml)?;
    settings.selected_app = settings
        .selected_app
        .map(|name| name.chars().take(63).collect());
    Ok(settings)
}

/// In-memory settings state. Saving is needed only when the value changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsState {
    current: UserSettings,
    persisted: UserSettings,
}

impl Default for SettingsState {
    fn default() -> Self {
        let defaults = UserSettings::default();
        Self {
            current: defaults.clone(),
            persisted: defaults,
        }
    }
}

impl SettingsState {
    pub fn current(&self) -> &UserSettings {
        &self.current
    }
    pub fn current_mut(&mut self) -> &mut UserSettings {
        &mut self.current
    }
    pub fn is_dirty(&self) -> bool {
        self.current != self.persisted
    }
    pub fn mark_persisted(&mut self) {
        self.persisted = self.current.clone();
    }

    /// Save the current value only when it differs from the last saved value.
    /// Keep the dirty state when the storage operation fails.
    pub fn save_if_dirty<E>(
        &mut self,
        save: impl FnOnce(&UserSettings) -> Result<(), E>,
    ) -> Result<bool, E> {
        if !self.is_dirty() {
            return Ok(false);
        }
        save(&self.current)?;
        self.mark_persisted();
        Ok(true)
    }

    pub fn load(&mut self, settings: UserSettings) {
        self.current = settings.clone();
        self.persisted = settings;
    }
}

#[cfg(target_arch = "powerpc")]
pub use crate::platform::settings::{
    SettingsIoError, load as load_from_nand, save as save_to_nand,
};

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::RefCell;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum StoreError {
        Missing,
        Read,
        Write,
    }

    #[test]
    fn missing_settings_are_created_and_reload_from_the_encoded_bytes() {
        let bytes = RefCell::new(None::<alloc::vec::Vec<u8>>);
        let defaults = UserSettings::default();
        let initialized = load_or_create(
            defaults.clone(),
            || match bytes.borrow().as_deref() {
                Some(saved) => decode_settings_xml(saved).map_err(|_| StoreError::Read),
                None => Err(StoreError::Missing),
            },
            |error| *error == StoreError::Missing,
            |settings| {
                *bytes.borrow_mut() = Some(
                    encode_settings_xml(settings)
                        .map_err(|_| StoreError::Write)?
                        .into_bytes(),
                );
                Ok(())
            },
        );

        assert_eq!(initialized, Ok(defaults.clone()));
        let saved = bytes.borrow().clone().unwrap();
        assert_eq!(
            saved,
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><hbc><device>-1</device><sort_order>0</sort_order><browse_mode>0</browse_mode></hbc>"
        );
        assert_eq!(decode_settings_xml(&saved), Ok(defaults));
    }

    #[test]
    fn load_errors_do_not_write_defaults_and_initial_save_errors_fail() {
        let mut save_called = false;
        let load_error = load_or_create(
            UserSettings::default(),
            || Err::<UserSettings, _>(StoreError::Read),
            |error| *error == StoreError::Missing,
            |_| {
                save_called = true;
                Ok(())
            },
        );
        assert_eq!(
            load_error,
            Err(SettingsInitializationError::Load(StoreError::Read))
        );
        assert!(!save_called);

        let save_error = load_or_create(
            UserSettings::default(),
            || Err::<UserSettings, _>(StoreError::Missing),
            |error| *error == StoreError::Missing,
            |_| Err(StoreError::Write),
        );
        assert_eq!(
            save_error,
            Err(SettingsInitializationError::Save(StoreError::Write))
        );
    }

    #[test]
    fn settings_xml_round_trips_and_escapes_selected_app() {
        let settings = UserSettings {
            device: 2,
            sort_order: 1,
            browse_mode: 1,
            selected_app: Some("a<&\"b".into()),
        };
        let xml = encode_settings_xml(&settings).unwrap();
        assert!(xml.contains("a&lt;&amp;&quot;b"));
        assert_eq!(decode_settings_xml(xml.as_bytes()).unwrap(), settings);
    }

    #[test]
    fn settings_encoder_matches_the_fixed_c_buffer_byte_limit() {
        let settings = UserSettings {
            selected_app: Some("é".repeat(40)),
            ..UserSettings::default()
        };
        let decoded =
            decode_settings_xml(encode_settings_xml(&settings).unwrap().as_bytes()).unwrap();
        assert_eq!(decoded.selected_app.as_deref().unwrap().len(), 62);
    }

    #[test]
    fn settings_defaults_match_hbc_and_dirty_state_is_coalesced() {
        assert_eq!(UserSettings::default().device, -1);
        assert_eq!(UserSettings::default().sort_order, 0);
        assert_eq!(UserSettings::default().browse_mode, 0);
        let mut state = SettingsState::default();
        assert_eq!(state.current().device, -1);
        assert!(!state.is_dirty());
        state.current_mut().browse_mode = 1;
        assert!(state.is_dirty());
        state.mark_persisted();
        assert!(!state.is_dirty());
    }

    #[test]
    fn save_if_dirty_skips_clean_values_and_marks_only_successful_saves() {
        let mut state = SettingsState::default();
        let mut save_calls = 0;
        assert_eq!(
            state.save_if_dirty(|_| {
                save_calls += 1;
                Ok::<(), i32>(())
            }),
            Ok(false)
        );
        assert_eq!(save_calls, 0);

        state.current_mut().browse_mode = 1;
        assert_eq!(state.save_if_dirty(|_| Err(-1)), Err(-1));
        assert!(state.is_dirty());
        assert_eq!(
            state.save_if_dirty(|_| {
                save_calls += 1;
                Ok::<(), i32>(())
            }),
            Ok(true)
        );
        assert_eq!(save_calls, 1);
        assert!(!state.is_dirty());
    }

    #[test]
    fn settings_codec_rejects_invalid_utf8_and_oversized_xml() {
        assert!(matches!(
            decode_settings_xml(&[0xff]),
            Err(SettingsCodecError::InvalidXml(_))
        ));
        assert_eq!(
            decode_settings_xml(&vec![b'a'; MAX_SETTINGS_XML_SIZE + 1]),
            Err(SettingsCodecError::TooLarge)
        );
    }
}
