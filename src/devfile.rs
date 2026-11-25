//! Reborn HBCF request validation shared by the Wii receiver and host tests.

pub const HEADER_LEN: usize = 16;
pub const PATH_CAPACITY: usize = 256;
pub const MAX_UPLOAD_SIZE: u32 = 512 * 1024 * 1024;
pub const MAX_LIST_SIZE: usize = 256 * 1024;
pub const MAX_FRAME_SIZE: usize = 64 * 1024;
pub const FLAG_COMPRESS: u8 = 1;

/// Newlib errno values used by the Reborn developer file protocol.
pub mod errno {
    pub const EIO: i32 = 5;
    pub const EINTR: i32 = 4;
    pub const ENOENT: i32 = 2;
    pub const ENOMEM: i32 = 12;
    pub const EISDIR: i32 = 21;
    pub const EINVAL: i32 = 22;
    pub const EFBIG: i32 = 27;
    pub const EPROTO: i32 = 71;
    pub const EBADMSG: i32 = 77;
    pub const ENOSYS: i32 = 88;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    PutFramed,
    PutRaw,
    Checksum,
    GetRaw,
    GetFramed,
    List,
    Stat,
    Delete,
    MakeDirectory,
}

impl Operation {
    pub const fn from_byte(value: u8) -> Option<Self> {
        match value {
            b'p' => Some(Self::PutFramed),
            b'P' => Some(Self::PutRaw),
            b'C' => Some(Self::Checksum),
            b'G' => Some(Self::GetRaw),
            b'g' => Some(Self::GetFramed),
            b'L' => Some(Self::List),
            b'S' => Some(Self::Stat),
            b'D' => Some(Self::Delete),
            b'M' => Some(Self::MakeDirectory),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestHeader {
    pub operation: Option<Operation>,
    pub flags: u8,
    pub path_len: usize,
    pub size: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestError {
    InvalidMagic,
    InvalidPathLength,
    InvalidPath,
}

impl RequestHeader {
    pub fn decode(header: &[u8; HEADER_LEN]) -> Result<Self, RequestError> {
        if &header[..4] != b"HBCF" {
            return Err(RequestError::InvalidMagic);
        }
        let path_len = usize::from(u16::from_be_bytes([header[6], header[7]]));
        if path_len == 0 || path_len >= PATH_CAPACITY {
            return Err(RequestError::InvalidPathLength);
        }
        Ok(Self {
            operation: Operation::from_byte(header[4]),
            flags: header[5],
            path_len,
            size: u32::from_be_bytes([header[8], header[9], header[10], header[11]]),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPath<'a> {
    pub device: &'a [u8],
    pub bytes: &'a [u8],
    pub trailing_slash: bool,
}

/// Accept only known-device paths that match Reborn's bounded path rules.
pub fn validate_path(path: &[u8]) -> Result<ValidatedPath<'_>, RequestError> {
    if path.is_empty() || path.len() >= PATH_CAPACITY {
        return Err(RequestError::InvalidPathLength);
    }
    if path.contains(&0) {
        return Err(RequestError::InvalidPath);
    }
    let colon = path
        .iter()
        .position(|byte| *byte == b':')
        .ok_or(RequestError::InvalidPath)?;
    if colon == 0 || path.get(colon + 1) != Some(&b'/') {
        return Err(RequestError::InvalidPath);
    }
    let device = &path[..colon];
    if !matches!(device, b"sd" | b"usb" | b"usb2" | b"carda" | b"cardb")
        || path
            .windows(2)
            .any(|window| window == b".." || window == b"//")
        || path.contains(&b'\\')
        || path[colon + 1..].contains(&b':')
        || path.iter().any(|byte| *byte < 0x20)
    {
        return Err(RequestError::InvalidPath);
    }
    Ok(ValidatedPath {
        device,
        bytes: path,
        trailing_slash: path.last() == Some(&b'/'),
    })
}

/// Remove one directory slash, as Reborn does before most file operations.
pub fn strip_directory_slash(path: &[u8]) -> &[u8] {
    if path.len() > 4 && path.last() == Some(&b'/') && path[path.len() - 2] != b':' {
        &path[..path.len() - 1]
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(operation: u8, path: &[u8], size: u32) -> [u8; HEADER_LEN] {
        let mut bytes = [0; HEADER_LEN];
        bytes[..4].copy_from_slice(b"HBCF");
        bytes[4] = operation;
        bytes[6..8].copy_from_slice(&(path.len() as u16).to_be_bytes());
        bytes[8..12].copy_from_slice(&size.to_be_bytes());
        bytes
    }

    #[test]
    fn decodes_hbcf_fields_and_operation() {
        assert_eq!(
            RequestHeader::decode(&header(b'g', b"usb2:/apps/a/boot.dol", 0x1234)),
            Ok(RequestHeader {
                operation: Some(Operation::GetFramed),
                flags: 0,
                path_len: 21,
                size: 0x1234,
            })
        );
        assert_eq!(
            RequestHeader::decode(&header(b'?', b"sd:/x", 1))
                .unwrap()
                .operation,
            None
        );
        assert_eq!(
            RequestHeader::decode(&[0; HEADER_LEN]),
            Err(RequestError::InvalidMagic)
        );
    }

    #[test]
    fn validates_all_supported_device_prefixes_and_preserves_put_slash() {
        for prefix in [b"sd".as_slice(), b"usb", b"usb2", b"carda", b"cardb"] {
            let mut path = prefix.to_vec();
            path.extend_from_slice(b":/apps/test/");
            let parsed = validate_path(&path).unwrap();
            assert_eq!(parsed.device, prefix);
            assert!(parsed.trailing_slash);
        }
        assert!(validate_path(b"sd:/").unwrap().trailing_slash);
    }

    #[test]
    fn rejects_untrusted_path_forms_and_enforces_byte_bound() {
        for path in [
            b"/apps/x".as_slice(),
            b"unknown:/x",
            b"sd:x",
            b"sd:/../x",
            b"sd:/a..b",
            b"sd://x",
            b"sd:/x\\y",
            b"sd:/x:y",
            b"sd:/bad\nname",
            b"sd:/x\0y",
        ] {
            assert_eq!(validate_path(path), Err(RequestError::InvalidPath));
        }
        assert_eq!(
            validate_path(&vec![b'a'; PATH_CAPACITY]),
            Err(RequestError::InvalidPathLength)
        );
        assert_eq!(
            RequestHeader::decode(&header(b'S', &vec![b'a'; PATH_CAPACITY], 0)),
            Err(RequestError::InvalidPathLength)
        );
    }

    #[test]
    fn strips_only_non_root_directory_slash() {
        assert_eq!(strip_directory_slash(b"sd:/apps/x/"), b"sd:/apps/x");
        assert_eq!(strip_directory_slash(b"sd:/"), b"sd:/");
        assert_eq!(strip_directory_slash(b"sd:/apps/x"), b"sd:/apps/x");
    }
}
