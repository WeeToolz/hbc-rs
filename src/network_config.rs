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

#[cfg(any(target_arch = "powerpc", test))]
const EXTERNAL_BACKUP_HEADER_LEN: usize = 20;
#[cfg(any(target_arch = "powerpc", test))]
const EXTERNAL_BACKUP_MAGIC: &[u8; 4] = b"HBCN";
#[cfg(any(target_arch = "powerpc", test))]
const EXTERNAL_BACKUP_VERSION: u8 = 1;
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) const ORIGINAL_BACKUP_KIND: u8 = 1;
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) const CURRENT_BACKUP_KIND: u8 = 2;

#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn active_storage_backup_path(
    prefix: &str,
    device_id: u32,
) -> Option<alloc::string::String> {
    matches!(prefix, "sd:" | "usb:" | "carda:" | "cardb:")
        .then(|| alloc::format!("{prefix}/apps/hbc-rs/netcfg-{device_id:08x}.v1"))
}

#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn active_storage_current_backup_path(
    prefix: &str,
    device_id: u32,
) -> Option<alloc::string::String> {
    matches!(prefix, "sd:" | "usb:" | "carda:" | "cardb:")
        .then(|| alloc::format!("{prefix}/apps/hbc-rs/netcur-{device_id:08x}.v1"))
}

/// Encode a console-bound backup record. CRC protects the record from damage;
/// the ES device ID prevents a valid backup from another Wii from being used.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn encode_external_backup(
    device_id: u32,
    kind: u8,
    config: &[u8],
) -> Option<alloc::vec::Vec<u8>> {
    if !matches!(kind, ORIGINAL_BACKUP_KIND | CURRENT_BACKUP_KIND) || config.len() != CONFIG_LEN {
        return None;
    }
    let mut record = alloc::vec![0; EXTERNAL_BACKUP_HEADER_LEN + config.len()];
    record[..4].copy_from_slice(EXTERNAL_BACKUP_MAGIC);
    record[4] = EXTERNAL_BACKUP_VERSION;
    record[5] = kind;
    record[8..12].copy_from_slice(&device_id.to_be_bytes());
    record[12..16].copy_from_slice(&(CONFIG_LEN as u32).to_be_bytes());
    record[EXTERNAL_BACKUP_HEADER_LEN..].copy_from_slice(config);
    let mut integrity = crc32fast::Hasher::new();
    integrity.update(&record[..16]);
    integrity.update(&record[EXTERNAL_BACKUP_HEADER_LEN..]);
    record[16..20].copy_from_slice(&integrity.finalize().to_be_bytes());
    Some(record)
}

#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn decode_external_backup(record: &[u8], device_id: u32, kind: u8) -> Option<&[u8]> {
    if record.len() != EXTERNAL_BACKUP_HEADER_LEN + CONFIG_LEN
        || &record[..4] != EXTERNAL_BACKUP_MAGIC
        || record[4] != EXTERNAL_BACKUP_VERSION
        || record[5] != kind
        || !matches!(kind, ORIGINAL_BACKUP_KIND | CURRENT_BACKUP_KIND)
        || record[6..8] != [0, 0]
        || u32::from_be_bytes(record[8..12].try_into().ok()?) != device_id
        || u32::from_be_bytes(record[12..16].try_into().ok()?) as usize != CONFIG_LEN
    {
        return None;
    }
    let mut integrity = crc32fast::Hasher::new();
    integrity.update(&record[..16]);
    integrity.update(&record[EXTERNAL_BACKUP_HEADER_LEN..]);
    if integrity.finalize().to_be_bytes() != record[16..20] {
        return None;
    }
    Some(&record[EXTERNAL_BACKUP_HEADER_LEN..])
}

#[derive(Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum ConfigCommitError<E> {
    BackupWrite(E),
    BackupRead(E),
    BackupMismatch,
    PrimaryWrite(E),
    PrimaryRead(E),
    PrimaryMismatch,
    BackupCleanup(E),
}

#[derive(Debug, Eq, PartialEq)]
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) enum ConfigSaveError<E> {
    OriginalBackup(E),
    CurrentBackupWrite(E),
    CurrentBackupRead(E),
    CurrentBackupInvalid,
    PrimaryRead(E),
    PrimaryChanged,
    PrimaryWrite(E),
    PrimaryVerificationRead(E),
    PrimaryVerificationFailed,
}

#[derive(Debug, Eq, PartialEq)]
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) enum UnbackedSaveError<E> {
    PrimaryRead(E),
    PrimaryChanged,
    PrimaryWrite(E),
    VerificationRead(E),
    VerificationFailed,
}

/// Perform an explicitly authorized write without a backup. Production still
/// reads the current bytes, checks the editor snapshot, and verifies the full
/// replacement after the write.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn commit_unbacked_config<E>(
    current: &[u8],
    replacement: &[u8],
    is_valid: impl Fn(&[u8]) -> bool,
    mut read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    mut write_primary: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), UnbackedSaveError<E>> {
    match read_primary().map_err(UnbackedSaveError::PrimaryRead)? {
        Some(bytes) if bytes == current && is_valid(&bytes) => {}
        _ => return Err(UnbackedSaveError::PrimaryChanged),
    }
    write_primary(replacement).map_err(UnbackedSaveError::PrimaryWrite)?;
    match read_primary().map_err(UnbackedSaveError::VerificationRead)? {
        Some(bytes) if bytes == replacement && is_valid(&bytes) => Ok(()),
        _ => Err(UnbackedSaveError::VerificationFailed),
    }
}

/// Run the same guarded save sequence for NAND-backed and removable recovery
/// files. The caller supplies real storage operations; every write is read
/// back before the next step can change the primary file.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn save_with_recovery_backups<E>(
    current: &[u8],
    replacement: &[u8],
    is_valid: impl Fn(&[u8]) -> bool,
    mut verify_original_backup: impl FnMut() -> Result<(), E>,
    mut write_current_backup: impl FnMut(&[u8]) -> Result<(), E>,
    mut read_current_backup: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    mut read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    mut write_primary: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), ConfigSaveError<E>> {
    verify_original_backup().map_err(ConfigSaveError::OriginalBackup)?;
    write_current_backup(current).map_err(ConfigSaveError::CurrentBackupWrite)?;
    match read_current_backup().map_err(ConfigSaveError::CurrentBackupRead)? {
        Some(bytes) if bytes == current && is_valid(&bytes) => {}
        _ => return Err(ConfigSaveError::CurrentBackupInvalid),
    }
    match read_primary().map_err(ConfigSaveError::PrimaryRead)? {
        Some(bytes) if bytes == current => {}
        _ => return Err(ConfigSaveError::PrimaryChanged),
    }
    write_primary(replacement).map_err(ConfigSaveError::PrimaryWrite)?;
    match read_primary().map_err(ConfigSaveError::PrimaryVerificationRead)? {
        Some(bytes) if bytes == replacement && is_valid(&bytes) => Ok(()),
        _ => Err(ConfigSaveError::PrimaryVerificationFailed),
    }
}

#[cfg(test)]
pub(crate) fn commit_config<E>(
    original: &[u8],
    replacement: &[u8],
    write_backup: impl FnMut(&[u8]) -> Result<(), E>,
    read_backup: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    write_primary: impl FnMut(&[u8]) -> Result<(), E>,
    read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    mut remove_backup: impl FnMut() -> Result<(), E>,
) -> Result<(), ConfigCommitError<E>> {
    commit_direct_config(
        original,
        replacement,
        write_backup,
        read_backup,
        |bytes| bytes == original,
        write_primary,
        read_primary,
    )?;
    remove_backup().map_err(ConfigCommitError::BackupCleanup)
}

/// Write an existing primary file only after an independent backup is verified.
/// Keep the backup for recovery from a failed or interrupted direct write.
#[cfg(test)]
pub(crate) fn commit_direct_config<E>(
    original: &[u8],
    replacement: &[u8],
    mut write_backup: impl FnMut(&[u8]) -> Result<(), E>,
    mut read_backup: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    valid_backup: impl Fn(&[u8]) -> bool,
    mut write_primary: impl FnMut(&[u8]) -> Result<(), E>,
    mut read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
) -> Result<(), ConfigCommitError<E>> {
    // Preserve the first recovery image across later saves. An existing image
    // may differ from the current NAND contents after an earlier successful save.
    match read_backup().map_err(ConfigCommitError::BackupRead)? {
        Some(bytes) if valid_backup(&bytes) => {}
        Some(_) => return Err(ConfigCommitError::BackupMismatch),
        None => {
            write_backup(original).map_err(ConfigCommitError::BackupWrite)?;
            match read_backup().map_err(ConfigCommitError::BackupRead)? {
                Some(bytes) if bytes == original && valid_backup(&bytes) => {}
                _ => return Err(ConfigCommitError::BackupMismatch),
            }
        }
    }
    // Recheck the retained backup immediately before the irreversible NAND write.
    match read_backup().map_err(ConfigCommitError::BackupRead)? {
        Some(bytes) if valid_backup(&bytes) => {}
        _ => return Err(ConfigCommitError::BackupMismatch),
    }
    write_primary(replacement).map_err(ConfigCommitError::PrimaryWrite)?;
    match read_primary().map_err(ConfigCommitError::PrimaryRead)? {
        Some(bytes) if bytes == replacement => {}
        _ => return Err(ConfigCommitError::PrimaryMismatch),
    }
    Ok(())
}

/// Commit a replacement when the caller has already created and verified a
/// recovery file in application-owned storage.
#[cfg(test)]
pub(crate) fn commit_with_existing_backup<E>(
    original: &[u8],
    replacement: &[u8],
    mut read_backup: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    valid_backup: impl Fn(&[u8]) -> bool,
    mut write_primary: impl FnMut(&[u8]) -> Result<(), E>,
    mut read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
) -> Result<(), ConfigCommitError<E>> {
    match read_backup().map_err(ConfigCommitError::BackupRead)? {
        Some(bytes) if valid_backup(&bytes) => {}
        Some(_) | None => return Err(ConfigCommitError::BackupMismatch),
    }
    match read_primary().map_err(ConfigCommitError::PrimaryRead)? {
        Some(bytes) if bytes == original => {}
        _ => return Err(ConfigCommitError::BackupMismatch),
    }
    write_primary(replacement).map_err(ConfigCommitError::PrimaryWrite)?;
    match read_primary().map_err(ConfigCommitError::PrimaryRead)? {
        Some(bytes) if bytes == replacement => Ok(()),
        _ => Err(ConfigCommitError::PrimaryMismatch),
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum ConfigRecoverError<E> {
    InvalidBackup,
    Restore(E),
    Verify(E),
    Mismatch,
    Cleanup(E),
}

#[derive(Debug, Eq, PartialEq)]
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) enum RetainedBackupRecoveryError<E> {
    InvalidBackup,
    PrimaryRead(E),
    Restore(E),
    Verify(E),
    Mismatch,
}

/// Restore only when the primary is absent or invalid. Keep the verified
/// backup after recovery so it remains available for a later failure.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn recover_from_retained_backup<E>(
    backup: Option<alloc::vec::Vec<u8>>,
    is_valid: impl Fn(&[u8]) -> bool,
    mut write_primary: impl FnMut(&[u8]) -> Result<(), E>,
    mut read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
) -> Result<bool, RetainedBackupRecoveryError<E>> {
    let Some(backup) = backup else {
        return Ok(false);
    };
    if !is_valid(&backup) {
        return Err(RetainedBackupRecoveryError::InvalidBackup);
    }
    match read_primary().map_err(RetainedBackupRecoveryError::PrimaryRead)? {
        Some(bytes) if is_valid(&bytes) => return Ok(false),
        None | Some(_) => {}
    }
    write_primary(&backup).map_err(RetainedBackupRecoveryError::Restore)?;
    match read_primary().map_err(RetainedBackupRecoveryError::Verify)? {
        Some(bytes) if bytes == backup && is_valid(&bytes) => Ok(true),
        _ => Err(RetainedBackupRecoveryError::Mismatch),
    }
}

#[cfg(test)]
pub(crate) fn recover_config<E>(
    backup: Option<alloc::vec::Vec<u8>>,
    is_valid: impl Fn(&[u8]) -> bool,
    mut write_primary: impl FnMut(&[u8]) -> Result<(), E>,
    mut read_primary: impl FnMut() -> Result<Option<alloc::vec::Vec<u8>>, E>,
    mut remove_backup: impl FnMut() -> Result<(), E>,
) -> Result<(), ConfigRecoverError<E>> {
    let Some(original) = backup else {
        return Ok(());
    };
    if !is_valid(&original) {
        return Err(ConfigRecoverError::InvalidBackup);
    }
    // A valid primary means the save completed and only backup cleanup was
    // interrupted. Keep the committed settings and remove the stale backup.
    if read_primary()
        .map_err(ConfigRecoverError::Verify)?
        .is_some_and(|bytes| is_valid(&bytes))
    {
        return remove_backup().map_err(ConfigRecoverError::Cleanup);
    }
    write_primary(&original).map_err(ConfigRecoverError::Restore)?;
    match read_primary().map_err(ConfigRecoverError::Verify)? {
        Some(bytes) if bytes == original => {}
        _ => return Err(ConfigRecoverError::Mismatch),
    }
    remove_backup().map_err(ConfigRecoverError::Cleanup)
}

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
                && active.replace(index).is_some()
            {
                return Err(NetworkProfileError::MultipleActiveProfiles);
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

    #[test]
    fn network_backup_path_uses_the_active_storage_device() {
        assert_eq!(
            active_storage_backup_path("usb:", 0x0102_0304).as_deref(),
            Some("usb:/apps/hbc-rs/netcfg-01020304.v1")
        );
        assert_eq!(
            active_storage_backup_path("sd:", 0x0102_0304).as_deref(),
            Some("sd:/apps/hbc-rs/netcfg-01020304.v1")
        );
        assert_eq!(
            active_storage_current_backup_path("usb:", 0x0102_0304).as_deref(),
            Some("usb:/apps/hbc-rs/netcur-01020304.v1")
        );
        assert_eq!(active_storage_backup_path("unknown:", 1), None);
    }

    #[test]
    fn external_backup_record_binds_console_type_and_integrity() {
        let bytes = alloc::vec![0; CONFIG_LEN];
        let record = encode_external_backup(0x0102_0304, ORIGINAL_BACKUP_KIND, &bytes).unwrap();
        assert_eq!(
            &record[..16],
            b"HBCN\x01\x01\0\0\x01\x02\x03\x04\0\0\x1b\x5c"
        );
        assert_eq!(&record[16..20], &[0x99, 0x5d, 0xe5, 0xdd]);
        assert_eq!(
            decode_external_backup(&record, 0x0102_0304, ORIGINAL_BACKUP_KIND),
            Some(bytes.as_slice())
        );
        assert_eq!(
            decode_external_backup(&record, 0x0506_0708, ORIGINAL_BACKUP_KIND),
            None
        );
        assert_eq!(
            decode_external_backup(&record, 0x0102_0304, CURRENT_BACKUP_KIND),
            None
        );

        let mut damaged = record;
        *damaged.last_mut().unwrap() ^= 1;
        assert_eq!(
            decode_external_backup(&damaged, 0x0102_0304, ORIGINAL_BACKUP_KIND),
            None
        );
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;
    use alloc::{vec, vec::Vec};
    use core::cell::RefCell;

    #[derive(Default)]
    struct Files {
        primary: Option<Vec<u8>>,
        backup: Option<Vec<u8>>,
    }

    #[test]
    fn commit_verifies_the_new_persisted_bytes_before_removing_backup() {
        let original = vec![1; CONFIG_LEN];
        let replacement = vec![2; CONFIG_LEN];
        let files = RefCell::new(Files {
            primary: Some(original.clone()),
            backup: None,
        });

        let result = commit_config(
            &original,
            &replacement,
            |bytes| {
                files.borrow_mut().backup = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(files.borrow().backup.clone()),
            |bytes| {
                files.borrow_mut().primary = Some(bytes.to_vec());
                Ok(())
            },
            || Ok(files.borrow().primary.clone()),
            || {
                files.borrow_mut().backup = None;
                Ok(())
            },
        );

        assert_eq!(result, Ok(()));
        let persisted = files.borrow();
        assert_eq!(persisted.primary.as_deref(), Some(replacement.as_slice()));
        assert_eq!(persisted.backup, None);
    }

    #[test]
    fn interrupted_commit_keeps_backup_and_recovery_restores_the_old_bytes() {
        let original = vec![3; CONFIG_LEN];
        let replacement = vec![4; CONFIG_LEN];
        let files = RefCell::new(Files {
            primary: Some(original.clone()),
            backup: None,
        });

        let result = commit_config(
            &original,
            &replacement,
            |bytes| {
                files.borrow_mut().backup = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(files.borrow().backup.clone()),
            |bytes| {
                files.borrow_mut().primary = Some(bytes[..128].to_vec());
                Err(-5)
            },
            || Ok(files.borrow().primary.clone()),
            || {
                files.borrow_mut().backup = None;
                Ok(())
            },
        );
        assert_eq!(result, Err(ConfigCommitError::PrimaryWrite(-5)));
        assert_eq!(files.borrow().backup.as_deref(), Some(original.as_slice()));

        let backup = files.borrow().backup.clone();
        let recovered = recover_config(
            backup,
            |bytes| bytes.len() == CONFIG_LEN && bytes == original,
            |bytes| {
                files.borrow_mut().primary = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(files.borrow().primary.clone()),
            || {
                files.borrow_mut().backup = None;
                Ok(())
            },
        );

        assert_eq!(recovered, Ok(()));
        assert_eq!(files.borrow().primary.as_deref(), Some(original.as_slice()));
        assert_eq!(files.borrow().backup, None);
    }

    #[test]
    fn direct_commit_keeps_verified_external_backup() {
        let original = vec![3; CONFIG_LEN];
        let replacement = vec![4; CONFIG_LEN];
        let files = RefCell::new(Files {
            primary: Some(original.clone()),
            backup: None,
        });
        let result = commit_direct_config(
            &original,
            &replacement,
            |bytes| {
                files.borrow_mut().backup = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(files.borrow().backup.clone()),
            |bytes| bytes.len() == CONFIG_LEN,
            |bytes| {
                files.borrow_mut().primary = Some(bytes.to_vec());
                Ok(())
            },
            || Ok(files.borrow().primary.clone()),
        );
        assert_eq!(result, Ok(()));
        let persisted = files.borrow();
        assert_eq!(persisted.primary.as_deref(), Some(replacement.as_slice()));
        assert_eq!(persisted.backup.as_deref(), Some(original.as_slice()));
    }

    #[test]
    fn existing_application_backup_is_verified_before_primary_commit() {
        let backup = vec![7; CONFIG_LEN];
        let replacement = vec![8; CONFIG_LEN];
        let primary = RefCell::new(Some(vec![7; CONFIG_LEN]));
        let result = commit_with_existing_backup::<i32>(
            &backup,
            &replacement,
            || Ok(Some(backup.clone())),
            |bytes| bytes == backup,
            |bytes| {
                *primary.borrow_mut() = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(primary.borrow().clone()),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(primary.borrow().as_deref(), Some(replacement.as_slice()));
    }

    #[test]
    fn existing_application_backup_must_be_valid_before_primary_commit() {
        let primary = RefCell::new(Some(vec![7; CONFIG_LEN]));
        let result = commit_with_existing_backup::<i32>(
            &[7; CONFIG_LEN],
            &[8; CONFIG_LEN],
            || Ok(Some(vec![0; CONFIG_LEN])),
            |_| false,
            |_| panic!("invalid backup must not change the primary"),
            || Ok(primary.borrow().clone()),
        );
        assert_eq!(result, Err(ConfigCommitError::BackupMismatch));
        assert_eq!(
            primary.borrow().as_deref(),
            Some(vec![7; CONFIG_LEN].as_slice())
        );
    }

    #[test]
    fn recovery_keeps_valid_committed_primary_and_removes_stale_transaction_backup() {
        let original = vec![9; CONFIG_LEN];
        let replacement = vec![10; CONFIG_LEN];
        let files = RefCell::new(Files {
            primary: Some(replacement.clone()),
            backup: Some(original.clone()),
        });
        let backup = files.borrow().backup.clone();
        let result = recover_config(
            backup,
            |bytes| bytes.len() == CONFIG_LEN,
            |bytes| {
                files.borrow_mut().primary = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(files.borrow().primary.clone()),
            || {
                files.borrow_mut().backup = None;
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            files.borrow().primary.as_deref(),
            Some(replacement.as_slice())
        );
        assert_eq!(files.borrow().backup, None);
    }

    #[test]
    fn retained_backup_is_not_used_when_primary_is_valid() {
        let backup = vec![11; CONFIG_LEN];
        let primary = vec![12; CONFIG_LEN];
        let writes = core::cell::Cell::new(0);
        let result = recover_from_retained_backup::<i32>(
            Some(backup),
            |bytes| bytes.len() == CONFIG_LEN,
            |_| {
                writes.set(writes.get() + 1);
                Ok::<_, i32>(())
            },
            || Ok(Some(primary.clone())),
        );
        assert_eq!(result, Ok(false));
        assert_eq!(writes.get(), 0);
    }

    #[test]
    fn retained_backup_does_not_overwrite_primary_when_read_fails() {
        let backup = vec![31; CONFIG_LEN];
        let writes = core::cell::Cell::new(0);
        let result = recover_from_retained_backup(
            Some(backup),
            |bytes| bytes.len() == CONFIG_LEN,
            |_| {
                writes.set(writes.get() + 1);
                Ok::<_, i32>(())
            },
            || Err(77),
        );
        assert_eq!(result, Err(RetainedBackupRecoveryError::PrimaryRead(77)));
        assert_eq!(writes.get(), 0);
    }

    #[test]
    fn retained_backup_restores_invalid_primary_and_remains_available() {
        let backup = vec![13; CONFIG_LEN];
        let primary = RefCell::new(Some(vec![0; 16]));
        let retained_backup = backup.clone();
        let result = recover_from_retained_backup::<i32>(
            Some(backup.clone()),
            |bytes| bytes.len() == CONFIG_LEN,
            |bytes| {
                *primary.borrow_mut() = Some(bytes.to_vec());
                Ok::<_, i32>(())
            },
            || Ok(primary.borrow().clone()),
        );
        assert_eq!(result, Ok(true));
        assert_eq!(primary.borrow().as_deref(), Some(backup.as_slice()));
        assert_eq!(retained_backup, backup);
    }

    #[test]
    fn retained_backup_rejects_invalid_copy_without_writing_primary() {
        let primary = RefCell::new(Some(vec![0; 16]));
        let result = recover_from_retained_backup::<i32>(
            Some(vec![1; 16]),
            |bytes| bytes.len() == CONFIG_LEN,
            |_| panic!("invalid backup must not be written"),
            || Ok(primary.borrow().clone()),
        );
        assert_eq!(result, Err(RetainedBackupRecoveryError::InvalidBackup));
        assert_eq!(primary.borrow().as_deref(), Some(vec![0; 16].as_slice()));
    }

    #[test]
    fn direct_commit_reuses_original_backup_on_repeated_saves() {
        let original = vec![3; CONFIG_LEN];
        let first = vec![4; CONFIG_LEN];
        let second = vec![5; CONFIG_LEN];
        let files = RefCell::new(Files {
            primary: Some(original.clone()),
            backup: None,
        });
        for (before, after) in [(&original, &first), (&first, &second)] {
            let result = commit_direct_config(
                before,
                after,
                |bytes| {
                    assert!(
                        files.borrow().backup.is_none(),
                        "must never replace original backup"
                    );
                    files.borrow_mut().backup = Some(bytes.to_vec());
                    Ok::<_, i32>(())
                },
                || Ok(files.borrow().backup.clone()),
                |bytes| bytes == original,
                |bytes| {
                    files.borrow_mut().primary = Some(bytes.to_vec());
                    Ok(())
                },
                || Ok(files.borrow().primary.clone()),
            );
            assert_eq!(result, Ok(()));
        }
        let saved = files.borrow();
        assert_eq!(saved.primary.as_deref(), Some(second.as_slice()));
        assert_eq!(saved.backup.as_deref(), Some(original.as_slice()));
    }

    #[test]
    fn repeated_saves_keep_original_and_latest_current_recovery_copies() {
        let original = vec![21; CONFIG_LEN];
        let first = vec![22; CONFIG_LEN];
        let second = vec![23; CONFIG_LEN];
        let files = RefCell::new((
            Some(original.clone()),
            None::<Vec<u8>>,
            Some(original.clone()),
        ));

        for (current, replacement) in [(&original, &first), (&first, &second)] {
            let result = save_with_recovery_backups(
                current,
                replacement,
                |bytes| bytes.len() == CONFIG_LEN,
                || {
                    assert_eq!(files.borrow().0.as_deref(), Some(original.as_slice()));
                    Ok::<_, i32>(())
                },
                |bytes| {
                    files.borrow_mut().1 = Some(bytes.to_vec());
                    Ok(())
                },
                || Ok(files.borrow().1.clone()),
                || Ok(files.borrow().2.clone()),
                |bytes| {
                    files.borrow_mut().2 = Some(bytes.to_vec());
                    Ok(())
                },
            );
            assert_eq!(result, Ok(()));
        }

        let files = files.borrow();
        assert_eq!(files.0.as_deref(), Some(original.as_slice()));
        assert_eq!(files.1.as_deref(), Some(first.as_slice()));
        assert_eq!(files.2.as_deref(), Some(second.as_slice()));
    }

    #[test]
    fn save_does_not_touch_primary_when_current_recovery_write_fails() {
        let original = vec![31; CONFIG_LEN];
        let replacement = vec![32; CONFIG_LEN];
        let primary = RefCell::new(Some(original.clone()));
        let result = save_with_recovery_backups(
            &original,
            &replacement,
            |bytes| bytes.len() == CONFIG_LEN,
            || Ok::<_, i32>(()),
            |_| Err(-102),
            || panic!("backup read is not valid after backup write fails"),
            || Ok(primary.borrow().clone()),
            |_| panic!("primary write must wait for a verified recovery copy"),
        );
        assert_eq!(result, Err(ConfigSaveError::CurrentBackupWrite(-102)));
        assert_eq!(primary.borrow().as_deref(), Some(original.as_slice()));
    }

    #[test]
    fn explicitly_unbacked_save_verifies_primary_before_and_after_write() {
        let current = vec![0x31; CONFIG_LEN];
        let replacement = vec![0x32; CONFIG_LEN];
        let primary = RefCell::new(Some(current.clone()));
        let writes = core::cell::Cell::new(0);
        let result = commit_unbacked_config::<i32>(
            &current,
            &replacement,
            |bytes| bytes.len() == CONFIG_LEN,
            || Ok(primary.borrow().clone()),
            |bytes| {
                writes.set(writes.get() + 1);
                *primary.borrow_mut() = Some(bytes.to_vec());
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(writes.get(), 1);
        assert_eq!(primary.borrow().as_deref(), Some(replacement.as_slice()));
    }

    #[test]
    fn explicitly_unbacked_save_refuses_a_changed_primary() {
        let current = vec![0x41; CONFIG_LEN];
        let changed = vec![0x42; CONFIG_LEN];
        let writes = core::cell::Cell::new(0);
        let result = commit_unbacked_config::<i32>(
            &current,
            &[0x43; CONFIG_LEN],
            |bytes| bytes.len() == CONFIG_LEN,
            || Ok(Some(changed.clone())),
            |_| {
                writes.set(writes.get() + 1);
                Ok(())
            },
        );
        assert_eq!(result, Err(UnbackedSaveError::PrimaryChanged));
        assert_eq!(writes.get(), 0);
    }

    #[test]
    fn explicitly_unbacked_save_reports_write_and_readback_failures() {
        let current = vec![0x51; CONFIG_LEN];
        let replacement = vec![0x52; CONFIG_LEN];
        let failed_write = commit_unbacked_config::<i32>(
            &current,
            &replacement,
            |bytes| bytes.len() == CONFIG_LEN,
            || Ok(Some(current.clone())),
            |_| Err(19),
        );
        assert_eq!(failed_write, Err(UnbackedSaveError::PrimaryWrite(19)));

        let reads = core::cell::Cell::new(0);
        let failed_verify = commit_unbacked_config::<i32>(
            &current,
            &replacement,
            |bytes| bytes.len() == CONFIG_LEN,
            || {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    Ok(Some(current.clone()))
                } else {
                    Ok(Some(current.clone()))
                }
            },
            |_| Ok(()),
        );
        assert_eq!(failed_verify, Err(UnbackedSaveError::VerificationFailed));
    }

    #[test]
    fn save_rejects_a_changed_primary_after_verifying_both_backups() {
        let expected = vec![41; CONFIG_LEN];
        let changed = vec![42; CONFIG_LEN];
        let replacement = vec![43; CONFIG_LEN];
        let primary = RefCell::new(Some(changed.clone()));
        let current_backup = RefCell::new(None::<Vec<u8>>);
        let result = save_with_recovery_backups(
            &expected,
            &replacement,
            |bytes| bytes.len() == CONFIG_LEN,
            || Ok::<_, i32>(()),
            |bytes| {
                *current_backup.borrow_mut() = Some(bytes.to_vec());
                Ok(())
            },
            || Ok(current_backup.borrow().clone()),
            || Ok(primary.borrow().clone()),
            |_| panic!("changed primary must not be overwritten"),
        );
        assert_eq!(result, Err(ConfigSaveError::PrimaryChanged));
        assert_eq!(primary.borrow().as_deref(), Some(changed.as_slice()));
        assert_eq!(
            current_backup.borrow().as_deref(),
            Some(expected.as_slice())
        );
    }

    #[test]
    fn direct_commit_rejects_existing_invalid_backup_without_writing() {
        let original = vec![3; CONFIG_LEN];
        let result = commit_direct_config(
            &original,
            &[4; CONFIG_LEN],
            |_| panic!("must not replace an existing backup"),
            || Ok::<_, i32>(Some(vec![0; CONFIG_LEN])),
            |bytes| bytes == original,
            |_| panic!("must not write NAND"),
            || panic!("must not read NAND"),
        );
        assert_eq!(result, Err(ConfigCommitError::BackupMismatch));
    }

    #[test]
    fn direct_commit_rejects_bad_external_backup_before_primary_write() {
        let original = vec![3; CONFIG_LEN];
        let replacement = vec![4; CONFIG_LEN];
        let result = commit_direct_config(
            &original,
            &replacement,
            |_| Ok::<_, i32>(()),
            || Ok(Some(vec![0; CONFIG_LEN])),
            |bytes| bytes == original,
            |_| panic!("primary must not change without a verified backup"),
            || panic!("primary must not be read without a verified backup"),
        );
        assert_eq!(result, Err(ConfigCommitError::BackupMismatch));
    }

    #[test]
    fn recovery_rejects_a_corrupt_backup_without_touching_primary() {
        let original = vec![5; CONFIG_LEN];
        let primary = RefCell::new(Some(original.clone()));
        let recovered = recover_config::<i32>(
            Some(vec![6; 17]),
            |_| false,
            |_| panic!("invalid backup must not be written"),
            || panic!("invalid backup must not be read back"),
            || panic!("invalid backup must remain for diagnosis"),
        );
        assert_eq!(recovered, Err(ConfigRecoverError::InvalidBackup));
        assert_eq!(primary.borrow().as_deref(), Some(original.as_slice()));
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
