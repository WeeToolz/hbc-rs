//! Cache the running title's NAND data directory and build safe file paths.

use alloc::ffi::CString;
use core::str;
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use core::{cell::UnsafeCell, ffi::CStr};

pub(crate) const TITLE_DATA_DIR_BUFFER_LEN: usize = 30;

const DIRECT_DOL_TITLE_ID: u64 = 0;
const DIRECT_DOL_DATA_DIRECTORY: &[u8] = b"/title/00010001/48424345/data";
const ES_INVALID_ARGUMENT: i32 = -1017;

const UNINITIALIZED: u8 = 0;
const INITIALIZING: u8 = 1;
const READY: u8 = 2;
const FAILED: u8 = 3;
const INVALID_DIRECTORY_ERROR: i32 = i32::MIN;
const IPC_EQUEUEFULL: i32 = -8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TitlePathError {
    Es(i32),
    InvalidPath,
    Busy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TitleDataDirectory {
    bytes: [u8; TITLE_DATA_DIR_BUFFER_LEN],
    len: usize,
}

impl TitleDataDirectory {
    pub(crate) fn hbc_development_directory() -> Result<Self, TitlePathError> {
        let mut bytes = [0; TITLE_DATA_DIR_BUFFER_LEN];
        bytes[..DIRECT_DOL_DATA_DIRECTORY.len()].copy_from_slice(DIRECT_DOL_DATA_DIRECTORY);
        Self::from_es_buffer(bytes)
    }

    pub(crate) fn for_title_id_result(
        title_id: Result<u64, i32>,
        resolve_es_directory: impl FnOnce(u64) -> Result<Self, TitlePathError>,
    ) -> Result<Self, TitlePathError> {
        let title_id = match title_id {
            Ok(title_id) => title_id,
            Err(ES_INVALID_ARGUMENT) => return Self::hbc_development_directory(),
            Err(error) => return Err(TitlePathError::Es(error)),
        };
        if title_id == DIRECT_DOL_TITLE_ID {
            return Self::hbc_development_directory();
        }
        match resolve_es_directory(title_id) {
            Err(TitlePathError::Es(ES_INVALID_ARGUMENT)) => Self::hbc_development_directory(),
            result => result,
        }
    }

    pub(crate) fn from_es_buffer(
        bytes: [u8; TITLE_DATA_DIR_BUFFER_LEN],
    ) -> Result<Self, TitlePathError> {
        let len = bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(TitlePathError::InvalidPath)?;
        str::from_utf8(&bytes[..len]).map_err(|_| TitlePathError::InvalidPath)?;
        Ok(Self { bytes, len })
    }

    pub(crate) fn development_parents(self) -> Option<[&'static CStr; 3]> {
        (self.bytes[..self.len] == *DIRECT_DOL_DATA_DIRECTORY).then_some([
            c"/title/00010001",
            c"/title/00010001/48424345",
            c"/title/00010001/48424345/data",
        ])
    }

    pub(crate) fn file_path(self, name: &str) -> Result<CString, TitlePathError> {
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(TitlePathError::InvalidPath);
        }
        let directory =
            str::from_utf8(&self.bytes[..self.len]).map_err(|_| TitlePathError::InvalidPath)?;
        CString::new(alloc::format!("{directory}/{name}")).map_err(|_| TitlePathError::InvalidPath)
    }
}

struct TitleDataDirectoryCache {
    state: AtomicU8,
    error: AtomicI32,
    directory: UnsafeCell<[u8; TITLE_DATA_DIR_BUFFER_LEN]>,
}

// The state transition publishes the directory before any reader can access it.
unsafe impl Sync for TitleDataDirectoryCache {}

impl TitleDataDirectoryCache {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNINITIALIZED),
            error: AtomicI32::new(INVALID_DIRECTORY_ERROR),
            directory: UnsafeCell::new([0; TITLE_DATA_DIR_BUFFER_LEN]),
        }
    }

    fn get_or_init(
        &self,
        resolve: impl FnOnce() -> Result<TitleDataDirectory, TitlePathError>,
    ) -> Result<TitleDataDirectory, TitlePathError> {
        let mut resolve = Some(resolve);
        loop {
            match self.state.load(Ordering::Acquire) {
                READY => {
                    let bytes = unsafe { *self.directory.get() };
                    return TitleDataDirectory::from_es_buffer(bytes);
                }
                FAILED => {
                    let error = self.error.load(Ordering::Relaxed);
                    return if error == INVALID_DIRECTORY_ERROR {
                        Err(TitlePathError::InvalidPath)
                    } else {
                        Err(TitlePathError::Es(error))
                    };
                }
                // The Wii has one CPU. Spinning here can prevent the thread
                // that owns the lookup from ever finishing its IOS request.
                INITIALIZING => return Err(TitlePathError::Busy),
                UNINITIALIZED => {
                    if self
                        .state
                        .compare_exchange(
                            UNINITIALIZED,
                            INITIALIZING,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    match resolve.take().expect("title path resolver used once")() {
                        Ok(directory) => {
                            unsafe { *self.directory.get() = directory.bytes };
                            self.state.store(READY, Ordering::Release);
                        }
                        Err(TitlePathError::Es(error)) if error == IPC_EQUEUEFULL => {
                            self.state.store(UNINITIALIZED, Ordering::Release);
                            return Err(TitlePathError::Es(error));
                        }
                        Err(TitlePathError::Es(error)) => {
                            self.error.store(error, Ordering::Relaxed);
                            self.state.store(FAILED, Ordering::Release);
                        }
                        Err(TitlePathError::InvalidPath) => {
                            self.error.store(INVALID_DIRECTORY_ERROR, Ordering::Relaxed);
                            self.state.store(FAILED, Ordering::Release);
                        }
                        Err(TitlePathError::Busy) => {
                            self.state.store(UNINITIALIZED, Ordering::Release);
                            return Err(TitlePathError::Busy);
                        }
                    }
                }
                _ => return Err(TitlePathError::InvalidPath),
            }
        }
    }
}

pub(crate) struct CachedTitleDataDirectory(TitleDataDirectoryCache);

impl CachedTitleDataDirectory {
    pub(crate) const fn new() -> Self {
        Self(TitleDataDirectoryCache::new())
    }

    pub(crate) fn get_or_init(
        &self,
        resolve: impl FnOnce() -> Result<TitleDataDirectory, TitlePathError>,
    ) -> Result<TitleDataDirectory, TitlePathError> {
        self.0.get_or_init(resolve)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CachedTitleDataDirectory, TitleDataDirectory, TitlePathError, DIRECT_DOL_TITLE_ID,
        ES_INVALID_ARGUMENT,
    };
    use core::cell::Cell;

    fn title_data_directory() -> TitleDataDirectory {
        let mut bytes = [0; super::TITLE_DATA_DIR_BUFFER_LEN];
        bytes[..29].copy_from_slice(b"/title/00010001/48424345/data");
        TitleDataDirectory::from_es_buffer(bytes).unwrap()
    }

    #[test]
    fn development_directory_prepares_only_its_own_parents() {
        let development = TitleDataDirectory::hbc_development_directory().unwrap();
        assert_eq!(
            development.development_parents().unwrap()[2],
            c"/title/00010001/48424345/data"
        );
        let channel = TitleDataDirectory::for_title_id_result(Ok(0x100014c554c5a), |_| {
            let mut bytes = [0; super::TITLE_DATA_DIR_BUFFER_LEN];
            bytes[..29].copy_from_slice(b"/title/00010001/4c554c5a/data");
            TitleDataDirectory::from_es_buffer(bytes)
        })
        .unwrap();
        assert_eq!(channel.development_parents(), None);
    }

    #[test]
    fn direct_dol_uses_the_explicit_hbc_development_directory() {
        let resolved = TitleDataDirectory::for_title_id_result(Ok(DIRECT_DOL_TITLE_ID), |_| {
            panic!("direct DOL must not request an ES title data directory")
        })
        .unwrap();
        assert_eq!(
            resolved.file_path("theme.zip").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/theme.zip"
        );
    }

    #[test]
    fn channel_and_non_direct_title_keep_the_es_data_directory() {
        let calls = Cell::new(0);
        let result = TitleDataDirectory::for_title_id_result(Ok(0x0001_0001_4842_4345), |_| {
            calls.set(calls.get() + 1);
            Err(TitlePathError::Es(-7))
        });
        assert_eq!(result, Err(TitlePathError::Es(-7)));
        assert_eq!(calls.get(), 1);

        let calls = Cell::new(0);
        let result = TitleDataDirectory::for_title_id_result(Ok(0x0000_0001_0000_0002), |_| {
            calls.set(calls.get() + 1);
            Ok(title_data_directory())
        })
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(
            result.file_path("settings.xml").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/settings.xml"
        );
    }

    #[test]
    fn direct_dol_es_context_error_uses_the_hbc_development_directory() {
        let resolved = TitleDataDirectory::for_title_id_result(Ok(0x0001_0001_444f_4c00), |_| {
            Err(TitlePathError::Es(ES_INVALID_ARGUMENT))
        })
        .unwrap();
        assert_eq!(
            resolved.file_path("theme.zip").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/theme.zip"
        );
    }

    #[test]
    fn unrelated_es_errors_are_preserved() {
        let result = TitleDataDirectory::for_title_id_result(Ok(0x0001_0001_444f_4c00), |_| {
            Err(TitlePathError::Es(-106))
        });
        assert_eq!(result, Err(TitlePathError::Es(-106)));
    }

    #[test]
    fn direct_dol_title_id_lookup_error_uses_the_hbc_development_directory() {
        let resolved = TitleDataDirectory::for_title_id_result(Err(ES_INVALID_ARGUMENT), |_| {
            panic!("failed direct-DOL title lookup must not request GetDataDir")
        })
        .unwrap();
        assert_eq!(
            resolved.file_path("settings.xml").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/settings.xml"
        );
    }

    #[test]
    fn settings_and_theme_paths_use_one_cached_es_lookup() {
        let cache = CachedTitleDataDirectory::new();
        let calls = Cell::new(0);
        let first = cache
            .get_or_init(|| {
                calls.set(calls.get() + 1);
                Ok(title_data_directory())
            })
            .unwrap();
        let second = cache
            .get_or_init(|| {
                calls.set(calls.get() + 1);
                Ok(title_data_directory())
            })
            .unwrap();

        assert_eq!(calls.get(), 1);
        assert_eq!(
            first.file_path("settings.xml").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/settings.xml"
        );
        assert_eq!(
            second.file_path("theme.zip").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/theme.zip"
        );
    }

    #[test]
    fn queue_full_es_error_is_not_cached() {
        let cache = CachedTitleDataDirectory::new();
        let calls = Cell::new(0);
        assert_eq!(
            cache.get_or_init(|| {
                calls.set(calls.get() + 1);
                Err(TitlePathError::Es(super::IPC_EQUEUEFULL))
            }),
            Err(TitlePathError::Es(super::IPC_EQUEUEFULL))
        );
        let recovered = cache
            .get_or_init(|| {
                calls.set(calls.get() + 1);
                Ok(title_data_directory())
            })
            .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(cache.get_or_init(|| unreachable!()), Ok(recovered));
    }

    #[test]
    fn lookup_errors_are_cached_without_retrying_es() {
        let cache = CachedTitleDataDirectory::new();
        let calls = Cell::new(0);
        for _ in 0..2 {
            assert_eq!(
                cache.get_or_init(|| {
                    calls.set(calls.get() + 1);
                    Err(TitlePathError::Es(-7))
                }),
                Err(TitlePathError::Es(-7))
            );
        }
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn overlapping_lookup_returns_busy_without_spinning_or_poisoning_the_cache() {
        let cache = CachedTitleDataDirectory::new();
        let resolved = cache
            .get_or_init(|| {
                assert_eq!(
                    cache.get_or_init(|| unreachable!()),
                    Err(TitlePathError::Busy)
                );
                Ok(title_data_directory())
            })
            .unwrap();
        assert_eq!(cache.get_or_init(|| unreachable!()), Ok(resolved));
    }

    #[test]
    fn invalid_file_names_and_es_buffers_are_rejected() {
        let directory = title_data_directory();
        for name in [
            "",
            "../settings.xml",
            "folder/theme.zip",
            "folder\\theme.zip",
        ] {
            assert_eq!(directory.file_path(name), Err(TitlePathError::InvalidPath));
        }
        assert_eq!(
            TitleDataDirectory::from_es_buffer([b'a'; super::TITLE_DATA_DIR_BUFFER_LEN]),
            Err(TitlePathError::InvalidPath)
        );
    }
}
