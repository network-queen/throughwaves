use jamhub_model::Project;
use std::time::Instant;

const MAX_UNDO_HISTORY: usize = 50;

/// Minimum interval between pushes to auto-group rapid edits (e.g. slider drags).
const GROUP_INTERVAL_MS: u128 = 400;

pub struct UndoManager {
    undo_stack: Vec<ProjectSnapshot>,
    redo_stack: Vec<ProjectSnapshot>,
}

pub struct ProjectSnapshot {
    pub label: String,
    pub timestamp: Instant,
    project: Project,
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Save the current state before making a change.
    /// Rapid consecutive edits with the same label are grouped (the older
    /// snapshot is kept so undo jumps back to the state before the burst).
    pub fn push(&mut self, label: &str, project: &Project) {
        let now = Instant::now();

        // Group rapid edits with the same label
        if let Some(top) = self.undo_stack.last() {
            let elapsed = now.duration_since(top.timestamp).as_millis();
            if elapsed < GROUP_INTERVAL_MS && top.label == label {
                // Keep the older snapshot (don't push a new one) — the undo
                // will jump back to the state before the burst started.
                self.redo_stack.clear();
                return;
            }
        }

        self.undo_stack.push(ProjectSnapshot {
            label: label.to_string(),
            timestamp: now,
            project: project.clone(),
        });
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Undo: returns the previous project state, saving current for redo.
    pub fn undo(&mut self, current: &Project) -> Option<Project> {
        if let Some(snapshot) = self.undo_stack.pop() {
            self.redo_stack.push(ProjectSnapshot {
                label: snapshot.label.clone(),
                timestamp: snapshot.timestamp,
                project: current.clone(),
            });
            Some(snapshot.project)
        } else {
            None
        }
    }

    /// Redo: returns the next project state, saving current for undo.
    pub fn redo(&mut self, current: &Project) -> Option<Project> {
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push(ProjectSnapshot {
                label: snapshot.label.clone(),
                timestamp: snapshot.timestamp,
                project: current.clone(),
            });
            Some(snapshot.project)
        } else {
            None
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo_stack.last().map(|s| s.label.as_str())
    }

    // --- History browsing API ---

    /// Number of undo steps available.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of redo steps available.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Read-only view of the undo stack (oldest first).
    pub fn undo_entries(&self) -> &[ProjectSnapshot] {
        &self.undo_stack
    }

    /// Read-only view of the redo stack (most recently undone first at the end).
    pub fn redo_entries(&self) -> &[ProjectSnapshot] {
        &self.redo_stack
    }

    /// Jump to a specific undo entry by index (0 = oldest).
    /// Everything above that index moves to the redo stack.
    /// Returns the project state at that index.
    pub fn jump_to_undo(&mut self, index: usize, current: &Project) -> Option<Project> {
        if index >= self.undo_stack.len() {
            return None;
        }
        // Push current state onto redo
        // We need to also push everything above `index` onto redo
        // undo_stack: [0, 1, ..., index, index+1, ..., len-1]
        // We want to restore index's project, and move index+1..len-1 + current to redo

        // First, drain everything above index into redo (in reverse order so redo pops correctly)
        let above: Vec<ProjectSnapshot> = self.undo_stack.drain((index + 1)..).collect();
        // Push current as the most-recently-undone
        self.redo_stack.push(ProjectSnapshot {
            label: above.first().map(|s| s.label.clone()).unwrap_or_default(),
            timestamp: Instant::now(),
            project: current.clone(),
        });
        // Push the drained entries onto redo (they were newer states)
        for snap in above.into_iter().rev() {
            self.redo_stack.push(snap);
        }

        // Pop the target entry
        let target = self.undo_stack.pop().unwrap();
        Some(target.project)
    }

    /// Jump to a specific redo entry by index (0 = oldest in redo stack).
    /// Returns the project state at that index.
    pub fn jump_to_redo(&mut self, index: usize, current: &Project) -> Option<Project> {
        if index >= self.redo_stack.len() {
            return None;
        }
        // redo_stack: [0, 1, ..., index, ..., len-1]  (len-1 is the most recent undo)
        // We want to redo up to `index` — everything from index..len-1 goes back to undo
        // and current + entries 0..index stay / move to undo

        // Push current onto undo
        self.undo_stack.push(ProjectSnapshot {
            label: self.redo_stack.get(index).map(|s| s.label.clone()).unwrap_or_default(),
            timestamp: Instant::now(),
            project: current.clone(),
        });

        // Move entries from index+1 to end onto undo (these are entries between current and target)
        let above: Vec<ProjectSnapshot> = self.redo_stack.drain((index + 1)..).collect();
        for snap in above.into_iter().rev() {
            self.undo_stack.push(snap);
        }

        // Pop the target
        let target = self.redo_stack.pop().unwrap();
        // Everything below index stays in redo
        Some(target.project)
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
