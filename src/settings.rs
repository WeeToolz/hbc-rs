//! Persistent settings model and XML codec.

use alloc::{format, string::String};

use crate::metadata::{parse_settings_xml, UserSettings};

pub const SETTINGS_PATH: &str = "settings.xml";
pub const MAX_SETTINGS_XML_SIZE: usize = 16 * 1024;

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
    if let Some(selected_app) = settings.selected_app.as_deref().filter(|v| !v.is_empty()) {
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
        let defaults = UserSettings {
            device: -1,
            ..UserSettings::default()
        };
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
    pub fn load(&mut self, settings: UserSettings) {
        self.current = settings.clone();
        self.persisted = settings;
    }
}

#[cfg(target_arch = "powerpc")]
pub use crate::platform::settings::{
    load as load_from_nand, save as save_to_nand, SettingsIoError,
};

#[cfg(test)]
mod tests {
    use super::*;

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
    fn settings_defaults_match_hbc_and_dirty_state_is_coalesced() {
        let mut state = SettingsState::default();
        assert_eq!(state.current().device, -1);
        assert!(!state.is_dirty());
        state.current_mut().browse_mode = 1;
        assert!(state.is_dirty());
        state.mark_persisted();
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
