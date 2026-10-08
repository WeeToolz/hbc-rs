//! Byte-exact representation of Wii `/shared2/sys/net/02/config.dat`.

pub const HEADER_LEN: usize = 8;
pub const PROFILE_LEN: usize = 0x91c;
pub const PROFILE_COUNT: usize = 3;
pub const CONFIG_LEN: usize = HEADER_LEN + PROFILE_LEN * PROFILE_COUNT;
pub const SCANNED_NETWORK_LIMIT: usize = 32;

pub const USE_WIRED: u8 = 1 << 0;
pub const USE_DHCP_DNS: u8 = 1 << 1;
pub const USE_DHCP_ADDR: u8 = 1 << 2;
pub const USE_PROXY: u8 = 1 << 4;
pub const HAS_INTERNET: u8 = 1 << 5;
pub const IS_ACTIVE: u8 = 1 << 7;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NetworkSecurity {
    Open,
    Wep,
    WpaTkip,
    WpaAes,
    Wpa2,
    #[default]
    Unknown,
}

impl NetworkSecurity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Wep => "WEP",
            Self::WpaTkip => "WPA TKIP",
            Self::WpaAes => "WPA AES",
            Self::Wpa2 => "WPA2 AES",
            Self::Unknown => "Unknown security",
        }
    }

    pub const fn config_encryption(self) -> Option<u8> {
        match self {
            Self::Open => Some(0),
            Self::Wep => Some(1),
            Self::WpaTkip => Some(4),
            Self::Wpa2 => Some(5),
            Self::WpaAes => Some(6),
            Self::Unknown => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScannedNetwork {
    pub ssid: [u8; 32],
    pub ssid_len: u8,
    pub security: NetworkSecurity,
    /// Signal bucket from weakest (0) to strongest (3).
    pub signal: u8,
}

impl ScannedNetwork {
    pub const EMPTY: Self = Self {
        ssid: [0; 32],
        ssid_len: 0,
        security: NetworkSecurity::Unknown,
        signal: 0,
    };

    pub fn name_bytes(&self) -> &[u8] {
        &self.ssid[..usize::from(self.ssid_len)]
    }

    pub const fn signal_label(&self) -> &'static str {
        match self.signal {
            3 => "Strong",
            2 => "Normal",
            1 => "Fair",
            _ => "Weak",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScannedNetworks {
    pub entries: [ScannedNetwork; SCANNED_NETWORK_LIMIT],
    pub len: usize,
}

impl Default for ScannedNetworks {
    fn default() -> Self {
        Self {
            entries: [ScannedNetwork::EMPTY; SCANNED_NETWORK_LIMIT],
            len: 0,
        }
    }
}

impl ScannedNetworks {
    pub fn as_slice(&self) -> &[ScannedNetwork] {
        &self.entries[..self.len.min(SCANNED_NETWORK_LIMIT)]
    }
}

const MANUAL_OFFSET: usize = 4;
const WIFI_OFFSET: usize = 0x7c4;

/// The exact serialized system configuration bytes. Access fields by their
/// documented offsets so padding and unknown bytes remain unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkConfig {
    bytes: [u8; CONFIG_LEN],
}

impl NetworkConfig {
    pub const fn from_bytes(bytes: [u8; CONFIG_LEN]) -> Self {
        Self { bytes }
    }

    pub const fn as_bytes(&self) -> &[u8; CONFIG_LEN] {
        &self.bytes
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8; CONFIG_LEN] {
        &mut self.bytes
    }

    pub fn profile(&self, index: usize) -> Option<Profile<'_>> {
        (index < PROFILE_COUNT).then(|| Profile {
            bytes: &self.bytes[profile_start(index)..profile_start(index) + PROFILE_LEN],
        })
    }

    pub fn profile_mut(&mut self, index: usize) -> Option<ProfileMut<'_>> {
        (index < PROFILE_COUNT).then(|| {
            let start = profile_start(index);
            ProfileMut {
                bytes: &mut self.bytes[start..start + PROFILE_LEN],
            }
        })
    }

    pub fn interface_kind(&self) -> u8 {
        self.bytes[4]
    }

    pub fn active_wifi_profile(&self) -> Result<usize, NetworkProfileError> {
        let index = self.active_profile()?;
        if self
            .profile(index)
            .is_some_and(|profile| profile.flags() & USE_WIRED != 0)
        {
            return Err(NetworkProfileError::ActiveProfileIsWired);
        }
        Ok(index)
    }

    pub fn active_profile(&self) -> Result<usize, NetworkProfileError> {
        let mut active = None;
        for index in 0..PROFILE_COUNT {
            if self
                .profile(index)
                .is_some_and(|profile| profile.flags() & IS_ACTIVE != 0)
            {
                if active.replace(index).is_some() {
                    return Err(NetworkProfileError::MultipleActiveProfiles);
                }
            }
        }
        active.ok_or(NetworkProfileError::NoActiveProfile)
    }

    pub fn validate_format(&self) -> Result<(), NetworkFormatError> {
        for index in 0..PROFILE_COUNT {
            let flags = self.profile(index).expect("profile index is valid").flags();
            if flags & 0x48 != 0 {
                return Err(NetworkFormatError::ReservedProfileFlags(index));
            }
        }
        self.active_profile().map_err(NetworkFormatError::Profile)?;
        Ok(())
    }

    pub fn set_interface_kind(&mut self, value: u8) {
        self.bytes[4] = value;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkProfileError {
    NoActiveProfile,
    ActiveProfileIsWired,
    MultipleActiveProfiles,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkFormatError {
    ReservedProfileFlags(usize),
    Profile(NetworkProfileError),
}

const fn profile_start(index: usize) -> usize {
    HEADER_LEN + index * PROFILE_LEN
}

#[derive(Clone, Copy)]
pub struct Profile<'a> {
    bytes: &'a [u8],
}

impl<'a> Profile<'a> {
    pub fn flags(self) -> u8 {
        self.bytes[0]
    }

    pub fn manual_ipv4(self, field: ManualIpv4Field) -> [u8; 4] {
        let offset = MANUAL_OFFSET + field.offset();
        self.bytes[offset..offset + 4]
            .try_into()
            .expect("fixed IPv4 width")
    }

    pub fn wifi(self) -> WifiConfig<'a> {
        WifiConfig {
            bytes: &self.bytes[WIFI_OFFSET..WIFI_OFFSET + WIFI_LEN],
        }
    }
}

pub struct ProfileMut<'a> {
    bytes: &'a mut [u8],
}

impl ProfileMut<'_> {
    pub fn flags(&self) -> u8 {
        self.bytes[0]
    }

    pub fn set_flag(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.bytes[0] |= flag;
        } else {
            self.bytes[0] &= !flag;
        }
    }

    pub fn manual_ipv4(&self, field: ManualIpv4Field) -> [u8; 4] {
        let offset = MANUAL_OFFSET + field.offset();
        self.bytes[offset..offset + 4]
            .try_into()
            .expect("fixed IPv4 width")
    }

    pub fn set_manual_ipv4(&mut self, field: ManualIpv4Field, value: [u8; 4]) {
        let offset = MANUAL_OFFSET + field.offset();
        self.bytes[offset..offset + 4].copy_from_slice(&value);
    }

    pub fn wifi(&self) -> WifiConfig<'_> {
        WifiConfig {
            bytes: &self.bytes[WIFI_OFFSET..WIFI_OFFSET + WIFI_LEN],
        }
    }

    pub fn wifi_mut(&mut self) -> WifiConfigMut<'_> {
        WifiConfigMut {
            bytes: &mut self.bytes[WIFI_OFFSET..WIFI_OFFSET + WIFI_LEN],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualIpv4Field {
    Address,
    Netmask,
    Gateway,
    PrimaryDns,
    SecondaryDns,
}

impl ManualIpv4Field {
    const fn offset(self) -> usize {
        match self {
            Self::Address => 0,
            Self::Netmask => 4,
            Self::Gateway => 8,
            Self::PrimaryDns => 12,
            Self::SecondaryDns => 16,
        }
    }
}

pub const WIFI_LEN: usize = 0x6c;
const WIFI_SSID_LEN_OFFSET: usize = 33;
const WIFI_ENCRYPTION_OFFSET: usize = 37;
const WIFI_KEY_LEN_OFFSET: usize = 41;
const WIFI_KEY_OFFSET: usize = 44;

#[derive(Clone, Copy)]
pub struct WifiConfig<'a> {
    bytes: &'a [u8],
}

impl<'a> WifiConfig<'a> {
    pub fn ssid(&self) -> &'a [u8; 32] {
        self.bytes[..32].try_into().expect("fixed SSID width")
    }

    pub fn ssid_len(&self) -> u8 {
        self.bytes[WIFI_SSID_LEN_OFFSET]
    }

    pub fn encryption(&self) -> u8 {
        self.bytes[WIFI_ENCRYPTION_OFFSET]
    }

    pub fn key_len(&self) -> u8 {
        self.bytes[WIFI_KEY_LEN_OFFSET]
    }

    pub fn key(&self) -> &'a [u8; 64] {
        self.bytes[WIFI_KEY_OFFSET..WIFI_KEY_OFFSET + 64]
            .try_into()
            .expect("fixed key width")
    }
}

pub struct WifiConfigMut<'a> {
    bytes: &'a mut [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WifiValidationError {
    InvalidSsidLength,
    OpenNetworkHasKey,
    InvalidWep64Key,
    InvalidWep128Key,
    InvalidWpaKey,
    UnsupportedEncryption,
}

/// Validate the saved SSID and key fields before writing config.dat.
pub fn validate_wifi(wifi: WifiConfig<'_>) -> Result<(), WifiValidationError> {
    if !(1..=32).contains(&wifi.ssid_len()) {
        return Err(WifiValidationError::InvalidSsidLength);
    }
    let key_len = usize::from(wifi.key_len());
    if key_len > 64 {
        return Err(WifiValidationError::InvalidWpaKey);
    }
    let key = &wifi.key()[..key_len];
    match wifi.encryption() {
        0 if key_len == 0 => Ok(()),
        0 => Err(WifiValidationError::OpenNetworkHasKey),
        1 if key_len == 5 || (key_len == 10 && is_hex(key)) => Ok(()),
        1 => Err(WifiValidationError::InvalidWep64Key),
        2 if key_len == 13 || (key_len == 26 && is_hex(key)) => Ok(()),
        2 => Err(WifiValidationError::InvalidWep128Key),
        4..=6 if (8..=63).contains(&key_len) || (key_len == 64 && is_hex(key)) => Ok(()),
        4..=6 => Err(WifiValidationError::InvalidWpaKey),
        _ => Err(WifiValidationError::UnsupportedEncryption),
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    fn config_with_active_profile(index: usize, flags: u8) -> NetworkConfig {
        let mut config = NetworkConfig::from_bytes([0; CONFIG_LEN]);
        config.profile_mut(index).unwrap().set_flag(flags, true);
        config
    }

    #[test]
    fn format_accepts_one_active_wired_profile_but_editor_rejects_it() {
        let config = config_with_active_profile(1, IS_ACTIVE | USE_WIRED);
        assert_eq!(config.validate_format(), Ok(()));
        assert_eq!(
            config.active_wifi_profile(),
            Err(NetworkProfileError::ActiveProfileIsWired)
        );
    }

    #[test]
    fn format_rejects_reserved_flags_and_ambiguous_active_profiles() {
        let mut reserved = config_with_active_profile(0, IS_ACTIVE);
        reserved.profile_mut(0).unwrap().set_flag(0x08, true);
        assert_eq!(
            reserved.validate_format(),
            Err(NetworkFormatError::ReservedProfileFlags(0))
        );

        let mut ambiguous = config_with_active_profile(0, IS_ACTIVE);
        ambiguous.profile_mut(2).unwrap().set_flag(IS_ACTIVE, true);
        assert_eq!(
            ambiguous.validate_format(),
            Err(NetworkFormatError::Profile(
                NetworkProfileError::MultipleActiveProfiles
            ))
        );
    }
}

fn is_hex(text: &[u8]) -> bool {
    text.iter().all(u8::is_ascii_hexdigit)
}

impl WifiConfigMut<'_> {
    pub fn encryption(&self) -> u8 {
        self.bytes[WIFI_ENCRYPTION_OFFSET]
    }

    pub fn set_ssid(&mut self, ssid: &[u8]) -> bool {
        if ssid.is_empty() || ssid.len() > 32 {
            return false;
        }
        self.bytes[..32].fill(0);
        self.bytes[..ssid.len()].copy_from_slice(ssid);
        self.bytes[WIFI_SSID_LEN_OFFSET] = ssid.len() as u8;
        true
    }

    pub fn set_encryption(&mut self, encryption: u8) -> bool {
        if matches!(encryption, 0..=2 | 4..=6) {
            self.bytes[WIFI_ENCRYPTION_OFFSET] = encryption;
            true
        } else {
            false
        }
    }

    pub fn set_key(&mut self, key: &[u8]) -> bool {
        if key.len() > 64 {
            return false;
        }
        self.bytes[WIFI_KEY_OFFSET..WIFI_KEY_OFFSET + 64].fill(0);
        self.bytes[WIFI_KEY_OFFSET..WIFI_KEY_OFFSET + key.len()].copy_from_slice(key);
        self.bytes[WIFI_KEY_LEN_OFFSET] = key.len() as u8;
        true
    }
}
