//! In-memory mirror of the user's lists. The UI reads it every frame; SQLite is written behind it.

use std::collections::HashMap;

use cinebox_core::{
    CatalogItem, LibraryEntry, LibraryList, LibraryMark, ListStatus, MediaKind, Section, TmdbId,
};

struct Slot {
    /// Higher is more recently changed.
    seq: u64,
    entry: LibraryEntry,
}

#[derive(Default)]
pub struct Library {
    slots: HashMap<(MediaKind, TmdbId), Slot>,
    next_seq: u64,
}

impl Library {
    /// `entries` must be newest first, as [`cinebox_core::Store::library_entries`] returns them.
    pub fn from_entries(entries: Vec<LibraryEntry>) -> Self {
        let next_seq = entries.len() as u64;
        let slots = entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let key = (entry.item.kind, entry.item.id);
                let seq = next_seq - index as u64;
                (key, Slot { seq, entry })
            })
            .collect();

        Self {
            slots,
            next_seq: next_seq + 1,
        }
    }

    pub fn mark(&self, kind: MediaKind, id: TmdbId) -> LibraryMark {
        self.slots
            .get(&(kind, id))
            .map_or_else(LibraryMark::default, |slot| slot.entry.mark)
    }

    /// Entries in `list`, optionally one section only, most recently changed first.
    pub fn list(&self, list: LibraryList, section: Option<Section>) -> Vec<&LibraryEntry> {
        let mut slots: Vec<&Slot> = self
            .slots
            .values()
            .filter(|slot| slot.entry.mark.in_list(list))
            .filter(|slot| section.is_none_or(|section| slot.entry.section == section))
            .collect();

        slots.sort_unstable_by_key(|slot| std::cmp::Reverse(slot.seq));
        slots.into_iter().map(|slot| &slot.entry).collect()
    }

    pub fn count(&self, list: LibraryList, section: Option<Section>) -> usize {
        self.slots
            .values()
            .filter(|slot| slot.entry.mark.in_list(list))
            .filter(|slot| section.is_none_or(|section| slot.entry.section == section))
            .count()
    }

    pub fn set_status(&mut self, item: &CatalogItem, section: Section, status: Option<ListStatus>) {
        self.update(item, section, |mark| mark.status = status);
    }

    pub fn set_liked(&mut self, item: &CatalogItem, section: Section, liked: bool) {
        self.update(item, section, |mark| mark.liked = liked);
    }

    /// Returns `true` when the status was empty and is now [`ListStatus::Watching`].
    pub fn mark_watching_if_unset(&mut self, item: &CatalogItem, section: Section) -> bool {
        if self.mark(item.kind, item.id).status.is_some() {
            return false;
        }

        self.set_status(item, section, Some(ListStatus::Watching));
        true
    }

    fn update(&mut self, item: &CatalogItem, section: Section, apply: impl FnOnce(&mut LibraryMark)) {
        let key = (item.kind, item.id);
        let mut mark = self.mark(item.kind, item.id);
        apply(&mut mark);

        if mark.is_empty() {
            self.slots.remove(&key);
            return;
        }

        let seq = self.next_seq;
        self.next_seq += 1;
        let entry = LibraryEntry {
            item: item.clone(),
            section,
            mark,
        };
        self.slots.insert(key, Slot { seq, entry });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(id: u32) -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(id),
            kind: MediaKind::Movie,
            title: format!("Movie {id}"),
            year: None,
            vote: None,
            poster_path: None,
        }
    }

    fn ids(entries: &[&LibraryEntry]) -> Vec<u32> {
        entries.iter().map(|entry| entry.item.id.get()).collect()
    }

    #[test]
    fn list_is_most_recently_changed_first() {
        let mut library = Library::default();
        let planned = LibraryList::Status(ListStatus::Planned);

        library.set_status(&movie(1), Section::Movies, Some(ListStatus::Planned));
        library.set_status(&movie(2), Section::Movies, Some(ListStatus::Planned));
        library.set_status(&movie(1), Section::Movies, Some(ListStatus::Planned));

        assert_eq!(ids(&library.list(planned, None)), vec![1, 2]);
    }

    #[test]
    fn loaded_entries_keep_database_order() {
        let entry = |id| LibraryEntry {
            item: movie(id),
            section: Section::Movies,
            mark: LibraryMark {
                status: None,
                liked: true,
            },
        };
        let library = Library::from_entries(vec![entry(5), entry(3)]);

        assert_eq!(ids(&library.list(LibraryList::Liked, None)), vec![5, 3]);
    }

    #[test]
    fn clearing_everything_forgets_the_title() {
        let mut library = Library::default();

        library.set_liked(&movie(1), Section::Movies, true);
        library.set_liked(&movie(1), Section::Movies, false);

        assert_eq!(library.count(LibraryList::Liked, None), 0);
    }

    #[test]
    fn mark_watching_keeps_existing_status() {
        let mut library = Library::default();
        library.set_status(&movie(1), Section::Movies, Some(ListStatus::Dropped));

        let changed = library.mark_watching_if_unset(&movie(1), Section::Movies);

        assert!(!changed);
    }

    #[test]
    fn section_filter_hides_other_sections() {
        let mut library = Library::default();
        let watching = LibraryList::Status(ListStatus::Watching);
        library.set_status(&movie(1), Section::Movies, Some(ListStatus::Watching));
        library.set_status(&movie(2), Section::Cartoons, Some(ListStatus::Watching));

        assert_eq!(ids(&library.list(watching, Some(Section::Cartoons))), vec![2]);
    }
}
