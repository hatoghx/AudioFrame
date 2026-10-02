use super::document::Project;
use std::time::{Duration, Instant};



pub struct UndoStack {
    past: Vec<Entry>,
    future: Vec<Entry>,
    limit: usize,
    last_at: Instant,
    last_key: Option<u64>,
}

struct Entry {
    label: String,
    doc: Project,
}

const MERGE_WINDOW: Duration = Duration::from_millis(500);

impl Default for UndoStack {
    fn default() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            limit: 200,
            last_at: Instant::now() - MERGE_WINDOW * 2,
            last_key: None,
        }
    }
}

impl UndoStack {
    
    
    pub fn commit(&mut self, label: impl Into<String>, key: Option<u64>, before: Project) {
        let now = Instant::now();
        let mergeable = key.is_some()
            && key == self.last_key
            && now.duration_since(self.last_at) < MERGE_WINDOW
            && !self.past.is_empty();

        self.last_at = now;
        self.last_key = key;

        if mergeable {
            
            self.future.clear();
            return;
        }

        self.past.push(Entry {
            label: label.into(),
            doc: before,
        });
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self, doc: &mut Project) {
        if let Some(entry) = self.past.pop() {
            let current = std::mem::replace(doc, entry.doc);
            self.future.push(Entry {
                label: entry.label,
                doc: current,
            });
            self.last_key = None;
        }
    }

    pub fn redo(&mut self, doc: &mut Project) {
        if let Some(entry) = self.future.pop() {
            let current = std::mem::replace(doc, entry.doc);
            self.past.push(Entry {
                label: entry.label,
                doc: current,
            });
            self.last_key = None;
        }
    }

    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
        self.last_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Track;

    fn commit_edit(
        u: &mut UndoStack,
        doc: &mut Project,
        key: Option<u64>,
        f: impl FnOnce(&mut Project),
    ) {
        let before = doc.clone();
        f(doc);
        u.commit("edit", key, before);
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut u = UndoStack::default();
        let mut doc = Project::default();

        commit_edit(&mut u, &mut doc, None, |d| d.tracks.push(Track::new("A")));
        commit_edit(&mut u, &mut doc, None, |d| d.tracks.push(Track::new("B")));
        assert_eq!(doc.tracks.len(), 2);

        u.undo(&mut doc);
        assert_eq!(doc.tracks.len(), 1);
        u.undo(&mut doc);
        assert_eq!(doc.tracks.len(), 0);
        assert!(!u.can_undo());

        u.redo(&mut doc);
        u.redo(&mut doc);
        assert_eq!(doc.tracks.len(), 2);
        assert_eq!(doc.tracks[1].name, "B");
    }

    #[test]
    fn merge_key_coalesces() {
        let mut u = UndoStack::default();
        let mut doc = Project::default();
        commit_edit(&mut u, &mut doc, None, |d| d.tracks.push(Track::new("A")));

        
        commit_edit(&mut u, &mut doc, Some(7), |d| d.tracks[0].volume_db = -3.0);
        commit_edit(&mut u, &mut doc, Some(7), |d| d.tracks[0].volume_db = -6.0);
        commit_edit(&mut u, &mut doc, Some(7), |d| d.tracks[0].volume_db = -9.0);

        u.undo(&mut doc); 
        assert_eq!(doc.tracks[0].volume_db, 0.0);
        u.undo(&mut doc); 
        assert_eq!(doc.tracks.len(), 0);
    }
}
