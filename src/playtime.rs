//! Checked reset of the first four bytes in the System Menu playtime file.

#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) fn record_has_reset_prefix(file_length: u32) -> bool {
    file_length >= 4
}

#[derive(Debug, Eq, PartialEq)]
#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) enum ResetError<E> {
    Operation(E),
    VerificationMismatch,
}

/// Clear the real record through the supplied read and write operations.
///
/// The caller owns the path and storage checks. This shared path confirms that
/// the record exists before it writes, then reads the committed bytes back.
#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) fn reset_record_with<E>(
    mut read: impl FnMut() -> Result<[u8; 4], E>,
    mut write: impl FnMut([u8; 4]) -> Result<(), E>,
) -> Result<(), ResetError<E>> {
    read().map_err(ResetError::Operation)?;
    write([0; 4]).map_err(ResetError::Operation)?;
    if read().map_err(ResetError::Operation)? == [0; 4] {
        Ok(())
    } else {
        Err(ResetError::VerificationMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::{record_has_reset_prefix, reset_record_with, ResetError};
    use core::cell::{Cell, RefCell};

    #[test]
    fn reset_accepts_larger_records_and_preserves_their_remaining_bytes() {
        assert!(!record_has_reset_prefix(0));
        assert!(!record_has_reset_prefix(3));
        assert!(record_has_reset_prefix(4));
        assert!(record_has_reset_prefix(128));

        let persisted = RefCell::new([0x5a; 128]);
        reset_record_with::<()>(
            || {
                let bytes = persisted.borrow();
                assert!(record_has_reset_prefix(bytes.len() as u32));
                Ok(bytes[..4].try_into().unwrap())
            },
            |prefix| {
                let mut bytes = persisted.borrow_mut();
                assert!(record_has_reset_prefix(bytes.len() as u32));
                bytes[..4].copy_from_slice(&prefix);
                Ok(())
            },
        )
        .unwrap();

        let reloaded = persisted.borrow();
        assert_eq!(&reloaded[..4], &[0, 0, 0, 0]);
        assert_eq!(&reloaded[4..], &[0x5a; 124]);
    }

    #[test]
    fn reset_writes_and_reloads_the_expected_zero_record() {
        let persisted = Cell::new([0x12, 0x34, 0x56, 0x78]);
        let writes = Cell::new(0);

        reset_record_with::<()>(
            || Ok(persisted.get()),
            |record| {
                writes.set(writes.get() + 1);
                persisted.set(record);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(writes.get(), 1);
        assert_eq!(persisted.get(), [0, 0, 0, 0]);
    }

    #[test]
    fn reset_does_not_write_when_the_initial_read_fails() {
        let mut writes = 0;
        let result = reset_record_with::<i32>(
            || Err(-106),
            |_| {
                writes += 1;
                Ok(())
            },
        );

        assert_eq!(result, Err(ResetError::Operation(-106)));
        assert_eq!(writes, 0);
    }

    #[test]
    fn reset_reports_a_write_failure() {
        let mut writes = 0;
        let result = reset_record_with::<i32>(
            || Ok([1, 2, 3, 4]),
            |_| {
                writes += 1;
                Err(-5)
            },
        );

        assert_eq!(result, Err(ResetError::Operation(-5)));
        assert_eq!(writes, 1);
    }

    #[test]
    fn reset_reports_when_the_written_value_does_not_persist() {
        let mut reads = 0;
        let result = reset_record_with::<()>(
            || {
                reads += 1;
                Ok([1, 2, 3, 4])
            },
            |_| Ok(()),
        );

        assert_eq!(result, Err(ResetError::VerificationMismatch));
        assert_eq!(reads, 2);
    }
}
