use crate::edits::{AudioEditAction, AudioEdits};

#[derive(Debug, Clone)]
pub(crate) struct DocumentSnapshot {
    pub(crate) samples: Vec<f32>,
    pub(crate) edits: AudioEdits,
    pub(crate) edit_actions: Vec<AudioEditAction>,
    pub(crate) markers: Vec<(usize, String)>,
}

const EDIT_HISTORY_MAX_ENTRIES: usize = 1000;

#[derive(Default)]
pub(crate) struct EditHistory {
    undo_entries: Vec<UndoEntry>,
    redo_entries: Vec<UndoEntry>,
    saved_position: usize,
    snapshots: Vec<DocumentSnapshot>,
}

struct UndoEntry {
    forward_snapshot: usize,
    inverse_snapshot: usize,
}

impl std::fmt::Debug for EditHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditHistory")
            .field("snapshots", &self.snapshots.len())
            .finish()
    }
}

impl EditHistory {
    pub(crate) fn new(initial: DocumentSnapshot) -> Self {
        Self {
            undo_entries: Vec::new(),
            redo_entries: Vec::new(),
            saved_position: 0,
            snapshots: vec![initial],
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.undo_entries.len() != self.saved_position
    }

    pub(crate) fn mark_saved(&mut self) {
        self.saved_position = self.undo_entries.len();
    }

    pub(crate) fn record(&mut self, previous: DocumentSnapshot, current: DocumentSnapshot) {
        let previous_index = self.push_snapshot(previous);
        let current_index = self.push_snapshot(current);
        self.undo_entries.push(UndoEntry {
            forward_snapshot: current_index,
            inverse_snapshot: previous_index,
        });
        if self.undo_entries.len() > EDIT_HISTORY_MAX_ENTRIES {
            self.undo_entries.remove(0);
            self.saved_position = self.saved_position.saturating_sub(1);
        }
        self.redo_entries.clear();
    }

    pub(crate) fn undo(&mut self) -> Option<DocumentSnapshot> {
        let entry = self.undo_entries.pop()?;
        let snapshot = self.snapshots.get(entry.inverse_snapshot).cloned();
        self.redo_entries.push(entry);
        snapshot
    }

    pub(crate) fn redo(&mut self) -> Option<DocumentSnapshot> {
        let entry = self.redo_entries.pop()?;
        let snapshot = self.snapshots.get(entry.forward_snapshot).cloned();
        self.undo_entries.push(entry);
        snapshot
    }

    fn push_snapshot(&mut self, snapshot: DocumentSnapshot) -> usize {
        let index = self.snapshots.len();
        self.snapshots.push(snapshot);
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_tracks_dirty_state_against_save_point() {
        let mut history = EditHistory::new(DocumentSnapshot {
            samples: vec![1.0f32],
            edits: AudioEdits::default(),
            edit_actions: Vec::new(),
            markers: Vec::new(),
        });
        assert!(!history.is_dirty());

        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
        );
        assert!(history.is_dirty());

        history.mark_saved();
        assert!(!history.is_dirty());
    }

    #[test]
    fn history_undo_redo_restores_states() {
        let mut history = EditHistory::new(DocumentSnapshot {
            samples: vec![1.0f32],
            edits: AudioEdits::default(),
            edit_actions: Vec::new(),
            markers: Vec::new(),
        });
        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
        );
        history.record(
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![3.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
        );

        let undone = history.undo().expect("can undo");
        assert_eq!(undone.samples, vec![2.0f32]);

        let undone_again = history.undo().expect("can undo again");
        assert_eq!(undone_again.samples, vec![1.0f32]);
        assert!(history.undo().is_none());

        let redone = history.redo().expect("can redo");
        assert_eq!(redone.samples, vec![2.0f32]);
    }

    #[test]
    fn history_record_clears_redo_stack() {
        let mut history = EditHistory::new(DocumentSnapshot {
            samples: vec![1.0f32],
            edits: AudioEdits::default(),
            edit_actions: Vec::new(),
            markers: Vec::new(),
        });
        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
        );
        history.undo();
        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![3.0f32],
                edits: AudioEdits::default(),
                edit_actions: Vec::new(),
                markers: Vec::new(),
            },
        );
        assert!(history.redo().is_none());
    }
}
