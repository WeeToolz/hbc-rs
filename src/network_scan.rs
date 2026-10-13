//! Validation of the IOS wireless scan count and descriptor records.

use crate::network_config::{NetworkSecurity, ScannedNetwork, ScannedNetworks};

// The IOS wire descriptor contains 31 two-byte fields, including byte arrays.
const BSS_FIXED_SIZE: usize = 62;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InvalidDescriptor;

fn be16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub(crate) fn parse_scan_results(bytes: &[u8]) -> Result<ScannedNetworks, InvalidDescriptor> {
    let mut networks = ScannedNetworks::default();
    let count = usize::from(be16(bytes, 0).ok_or(InvalidDescriptor)?);
    let mut offset = 2usize;
    if count > bytes.len().saturating_sub(2) / BSS_FIXED_SIZE {
        return Err(InvalidDescriptor);
    }
    for _ in 0..count {
        let record = &bytes[offset..];
        let length = usize::from(be16(record, 0).ok_or(InvalidDescriptor)?);
        if length < BSS_FIXED_SIZE
            || offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(InvalidDescriptor);
        }
        let record = &record[..length];
        // These offsets follow the repr(C) BssDescriptor layout used by libogc.
        let ssid_len = usize::from(be16(record, 10).ok_or(InvalidDescriptor)?);
        let capabilities = be16(record, 44).ok_or(InvalidDescriptor)?;
        let ies_len = usize::from(be16(record, 60).ok_or(InvalidDescriptor)?);
        if ssid_len > 32
            || BSS_FIXED_SIZE
                .checked_add(ies_len)
                .is_none_or(|end| end > length)
        {
            return Err(InvalidDescriptor);
        }
        if ssid_len > 0 {
            let ssid: [u8; 32] = record[12..44].try_into().map_err(|_| InvalidDescriptor)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dolphin_count_prefix_and_64_byte_bss_info_record() {
        let mut bytes = [0u8; 2 + 64];
        bytes[0..2].copy_from_slice(&[0x00, 0x01]);
        bytes[2..4].copy_from_slice(&[0x00, 0x40]);
        bytes[4..6].copy_from_slice(&[0xff, 0xff]);
        bytes[6..12].copy_from_slice(&[0, 1, 2, 3, 4, 5]);
        bytes[12..14].copy_from_slice(&[0x00, 0x0b]);
        bytes[14..25].copy_from_slice(b"dolphin-emu");
        bytes[56..58].copy_from_slice(&[0x00, 0x02]);

        let networks = parse_scan_results(&bytes).unwrap();

        assert_eq!(networks.len, 1);
        assert_eq!(networks.entries[0].name_bytes(), b"dolphin-emu");
        assert_eq!(networks.entries[0].security, NetworkSecurity::Open);
        assert_eq!(networks.entries[0].signal, 3);
    }

    #[test]
    fn rejects_count_prefix_when_the_declared_record_is_truncated() {
        let bytes = [0x00, 0x01, 0x00, 0x40, 0xff, 0xff];
        assert_eq!(parse_scan_results(&bytes), Err(InvalidDescriptor));
    }
}
