//! Test-only ZIP data. Production parsing does not use this encoder.
use alloc::vec::Vec;

struct Entry<'a> {
    name: &'a str,
    payload: &'a [u8],
    method: u16,
    flags: u16,
}

pub(crate) fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    zip_with_flags(files, 0)
}

pub(crate) fn zip_with_flags(files: &[(&str, &[u8])], flags: u16) -> Vec<u8> {
    build_zip(
        &files
            .iter()
            .map(|&(name, payload)| Entry {
                name,
                payload,
                method: 0,
                flags,
            })
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn zip_with_payload(name: &str, payload: &[u8], method: u16, flags: u16) -> Vec<u8> {
    build_zip(&[Entry {
        name,
        payload,
        method,
        flags,
    }])
}

fn build_zip(files: &[Entry<'_>]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut central = Vec::new();
    for Entry {
        name,
        payload,
        method,
        flags,
    } in files
    {
        let compressed = match method {
            0 => payload.to_vec(),
            8 => miniz_oxide::deflate::compress_to_vec(payload, 6),
            _ => panic!("unsupported fixture compression"),
        };
        let compressed_size = compressed.len() as u32;
        let offset = output.len() as u32;
        let name_bytes = name.as_bytes();
        let size = payload.len() as u32;
        let crc = crc32fast::hash(payload);
        output.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        output.extend_from_slice(&20u16.to_le_bytes());
        output.extend_from_slice(&flags.to_le_bytes());
        output.extend_from_slice(&method.to_le_bytes());
        output.extend_from_slice(&[0; 4]);
        output.extend_from_slice(&crc.to_le_bytes());
        output.extend_from_slice(&compressed_size.to_le_bytes());
        output.extend_from_slice(&size.to_le_bytes());
        output.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(name_bytes);
        output.extend_from_slice(&compressed);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&flags.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&compressed_size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 6]);
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }
    let central_offset = output.len() as u32;
    output.extend_from_slice(&central);
    output.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(&(files.len() as u16).to_le_bytes());
    output.extend_from_slice(&(files.len() as u16).to_le_bytes());
    output.extend_from_slice(&(central.len() as u32).to_le_bytes());
    output.extend_from_slice(&central_offset.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes());
    output
}
