//! Access point scan through the Wii wireless driver.

use crate::network_config::{NetworkSecurity, ScannedNetwork, ScannedNetworks};

const SCAN_BUFFER_SIZE: usize = 16 * 1024;
const BSS_FIXED_SIZE: usize = core::mem::size_of::<ogc_wd::BssDescriptor>();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanError {
    Driver(ogc_wd::ErrorCode),
    InvalidDescriptor,
}

/// Scan nearby access points and return validated SSIDs and security modes.
/// The caller must stop the active socket receiver before calling this API.
pub fn scan() -> Result<ScannedNetworks, ScanError> {
    let mut params = ogc_wd::default_scan_parameters();
    let mut bytes = alloc::vec![0; SCAN_BUFFER_SIZE];
    ogc_wd::scan_once_result(&mut params, &mut bytes).map_err(ScanError::Driver)?;
    parse_scan_results(&bytes)
}

fn be16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn parse_scan_results(bytes: &[u8]) -> Result<ScannedNetworks, ScanError> {
    let mut networks = ScannedNetworks::default();
    let mut offset = 0usize;
    while offset
        .checked_add(BSS_FIXED_SIZE)
        .is_some_and(|end| end <= bytes.len())
    {
        let record = &bytes[offset..];
        let length = usize::from(be16(record, 0).ok_or(ScanError::InvalidDescriptor)?);
        if length == 0 {
            break;
        }
        if length < BSS_FIXED_SIZE
            || offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(ScanError::InvalidDescriptor);
        }
        let record = &record[..length];
        // These offsets follow the repr(C) BssDescriptor layout used by libogc.
        let ssid_len = usize::from(be16(record, 10).ok_or(ScanError::InvalidDescriptor)?);
        let capabilities = be16(record, 44).ok_or(ScanError::InvalidDescriptor)?;
        let ies_len = usize::from(be16(record, 60).ok_or(ScanError::InvalidDescriptor)?);
        if ssid_len > 32
            || BSS_FIXED_SIZE
                .checked_add(ies_len)
                .is_none_or(|end| end > length)
        {
            return Err(ScanError::InvalidDescriptor);
        }
        if ssid_len > 0 {
            let ssid: [u8; 32] = record[12..44]
                .try_into()
                .map_err(|_| ScanError::InvalidDescriptor)?;
            let security = detect_security(
                capabilities,
                &record[BSS_FIXED_SIZE..BSS_FIXED_SIZE + ies_len],
            );
            let signal = match be16(record, 2).unwrap_or(0) {
                0xc4..=u16::MAX => 3,
                0xb5..=0xc3 => 2,
                0xab..=0xb4 => 1,
                _ => 0,
            };
            let network = ScannedNetwork {
                ssid,
                ssid_len: ssid_len as u8,
                security,
                signal,
            };
            add_network(&mut networks, network);
        }
        offset += length;
    }
    Ok(networks)
}

fn add_network(list: &mut ScannedNetworks, network: ScannedNetwork) {
    if let Some(existing) = list
        .as_slice()
        .iter()
        .position(|item| item.name_bytes() == network.name_bytes())
    {
        if network.signal > list.entries[existing].signal {
            list.entries[existing] = network;
        }
    } else if list.len < list.entries.len() {
        list.entries[list.len] = network;
        list.len += 1;
    }
}

fn detect_security(capabilities: u16, ies: &[u8]) -> NetworkSecurity {
    let mut offset = 0;
    let mut rsn = None;
    let mut wpa = None;
    while offset + 2 <= ies.len() {
        let id = ies[offset];
        let len = usize::from(ies[offset + 1]);
        offset += 2;
        let Some(end) = offset.checked_add(len).filter(|end| *end <= ies.len()) else {
            return NetworkSecurity::Unknown;
        };
        let data = &ies[offset..end];
        if id == 0x30 {
            rsn = parse_security_ie(data, true);
        } else if id == 0xdd && data.starts_with(&[0x00, 0x50, 0xf2, 0x01]) {
            wpa = parse_security_ie(&data[4..], false);
        }
        offset = end;
    }
    if offset != ies.len() {
        return NetworkSecurity::Unknown;
    }
    if let Some(mode) = rsn {
        return mode;
    }
    if let Some(mode) = wpa {
        return mode;
    }
    if capabilities & 0x0010 != 0 {
        NetworkSecurity::Wep
    } else {
        NetworkSecurity::Open
    }
}

fn parse_security_ie(data: &[u8], is_rsn: bool) -> Option<NetworkSecurity> {
    // WPA vendor IEs carry a version before the group and pairwise suites.
    let mut offset = 2usize;
    if data.len() < 2 {
        return None;
    }
    offset = offset.checked_add(4)?;
    let pairwise_count = usize::from(be16(data, offset)?);
    offset = offset.checked_add(2)?;
    let suites_len = pairwise_count.checked_mul(4)?;
    let suites = data.get(offset..offset.checked_add(suites_len)?)?;
    let mut has_tkip = false;
    let mut has_aes = false;
    for suite in suites.chunks_exact(4) {
        if suite[..3] != [0x00, 0x0f, 0xac] && suite[..3] != [0x00, 0x50, 0xf2] {
            continue;
        }
        match suite[3] {
            2 => has_tkip = true,
            4 => has_aes = true,
            _ => {}
        }
    }
    match (has_aes, has_tkip) {
        (true, _) => Some(if is_rsn {
            NetworkSecurity::Wpa2
        } else {
            NetworkSecurity::WpaAes
        }),
        (false, true) => Some(NetworkSecurity::WpaTkip),
        _ => None,
    }
}
