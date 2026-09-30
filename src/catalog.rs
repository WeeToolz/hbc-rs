//! Scan-independent app catalog policy.

use crate::metadata::AppMetadata;
use alloc::{string::String, vec::Vec};
use core::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppKind {
    Elf,
    Dol,
    Theme,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppEntry {
    pub directory: String,
    pub kind: AppKind,
    pub size: u64,
    pub has_icon: bool,
    pub metadata: Option<AppMetadata>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Filter {
    #[default]
    All,
    IconsOnly,
    MetadataOnly,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortOrder {
    #[default]
    Name,
    ReleaseDate,
}

/// Return borrowed entries in display order. Date order is newest first.
pub fn visible_entries(entries: &[AppEntry], filter: Filter, sort: SortOrder) -> Vec<&AppEntry> {
    let mut visible: Vec<_> = entries
        .iter()
        .filter(|entry| match filter {
            Filter::All => true,
            Filter::IconsOnly => entry.has_icon,
            Filter::MetadataOnly => entry.metadata.is_some(),
        })
        .collect();
    visible.sort_by(|a, b| compare(a, b, sort));
    visible
}

fn compare(a: &AppEntry, b: &AppEntry, sort: SortOrder) -> Ordering {
    match sort {
        SortOrder::Name => match (&a.metadata, &b.metadata) {
            (Some(ma), Some(mb)) => cmp_optional_text(ma.name.as_deref(), mb.name.as_deref())
                .then_with(|| cmp_text(&a.directory, &b.directory)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => cmp_text(&a.directory, &b.directory),
        },
        SortOrder::ReleaseDate => match (&a.metadata, &b.metadata) {
            (Some(ma), Some(mb)) => mb
                .release_date
                .cmp(&ma.release_date)
                .then_with(|| cmp_optional_text(ma.name.as_deref(), mb.name.as_deref()))
                .then_with(|| cmp_text(&a.directory, &b.directory)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => cmp_text(&a.directory, &b.directory),
        },
    }
}

fn cmp_optional_text(a: Option<&str>, b: Option<&str>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => cmp_text(a, b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn cmp_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    fn app(dir: &str, name: Option<&str>, date: Option<u64>, icon: bool) -> AppEntry {
        AppEntry {
            directory: dir.into(),
            kind: AppKind::Elf,
            size: 1,
            has_icon: icon,
            metadata: name.map(|name| AppMetadata {
                name: Some(name.into()),
                author: None,
                version: None,
                short_description: None,
                long_description: None,
                release_date: date,
                arguments: vec![],
                ahb_access: false,
            }),
        }
    }
    #[test]
    fn filters_and_sorts_deterministically() {
        let entries = [
            app("z", Some("Zulu"), Some(20240101), false),
            app("a", Some("alpha"), Some(20250101), true),
            app("b", None, None, true),
        ];
        assert_eq!(
            visible_entries(&entries, Filter::IconsOnly, SortOrder::Name)
                .iter()
                .map(|e| e.directory.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            visible_entries(&entries, Filter::MetadataOnly, SortOrder::ReleaseDate)
                .iter()
                .map(|e| e.directory.as_str())
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert_eq!(
            visible_entries(&entries, Filter::All, SortOrder::Name)
                .iter()
                .map(|e| e.directory.as_str())
                .collect::<Vec<_>>(),
            ["a", "z", "b"]
        );
    }
}
