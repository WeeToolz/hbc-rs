//! Shared screenshot filename selection for the production FAT save path.

#[cfg(any(test, target_arch = "powerpc"))]
use alloc::string::String;

#[derive(Debug, Eq, PartialEq)]
#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) enum SaveError<E> {
    Storage(E),
    IndexExhausted,
    AllocationFailed,
}

#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) fn save_png_with<E>(
    bytes: &[u8],
    mut exists: impl FnMut(&str) -> Result<bool, E>,
    mut write: impl FnMut(&str, &[u8]) -> Result<bool, E>,
) -> Result<String, SaveError<E>> {
    for index in 0..=999 {
        let mut path = String::new();
        path.try_reserve_exact(11)
            .map_err(|_| SaveError::AllocationFailed)?;
        path.push_str("hbc-");
        path.push(char::from(b'0' + (index / 100) as u8));
        path.push(char::from(b'0' + ((index / 10) % 10) as u8));
        path.push(char::from(b'0' + (index % 10) as u8));
        path.push_str(".png");
        if exists(&path).map_err(SaveError::Storage)? {
            continue;
        }
        if write(&path, bytes).map_err(SaveError::Storage)? {
            return Ok(path);
        }
    }
    Err(SaveError::IndexExhausted)
}

#[cfg(test)]
mod tests {
    use super::{save_png_with, SaveError};
    use alloc::{string::String, vec::Vec};

    #[test]
    fn skips_existing_names_and_saves_the_exact_png_bytes() {
        let mut saved: Option<(String, Vec<u8>)> = None;
        let path = save_png_with(
            b"png bytes",
            |path| Ok(path == "hbc-000.png" || path == "hbc-001.png"),
            |path, bytes| {
                saved = Some((path.into(), bytes.to_vec()));
                Ok::<_, ()>(true)
            },
        )
        .unwrap();

        assert_eq!(path, "hbc-002.png");
        assert_eq!(saved, Some(("hbc-002.png".into(), b"png bytes".to_vec())));
    }

    #[test]
    fn reports_existence_check_and_write_failures() {
        assert_eq!(
            save_png_with(b"png", |_| Err(-1), |_, _| Ok::<_, i32>(true)),
            Err(SaveError::Storage(-1))
        );
        assert_eq!(
            save_png_with(b"png", |_| Ok(false), |_, _| Err::<bool, _>(-2)),
            Err(SaveError::Storage(-2))
        );
    }

    #[test]
    fn refuses_to_reuse_any_of_the_thousand_c_screenshot_names() {
        let mut checked = Vec::new();
        let result = save_png_with(
            b"png",
            |path| {
                checked.push(path.to_owned());
                Ok::<_, ()>(true)
            },
            |_, _| Ok(true),
        );

        assert_eq!(result, Err(SaveError::IndexExhausted));
        assert_eq!(checked.len(), 1000);
        assert_eq!(checked.first().map(String::as_str), Some("hbc-000.png"));
        assert_eq!(checked.last().map(String::as_str), Some("hbc-999.png"));
    }

    #[test]
    fn retries_when_exclusive_create_reports_a_name_collision() {
        let mut writes = Vec::new();
        let path = save_png_with(
            b"png",
            |_| Ok::<bool, ()>(false),
            |path, _| {
                writes.push(path.to_owned());
                Ok(path != "hbc-000.png")
            },
        )
        .unwrap();

        assert_eq!(writes, ["hbc-000.png", "hbc-001.png"]);
        assert_eq!(path, "hbc-001.png");
    }
}
