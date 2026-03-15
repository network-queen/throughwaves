use jamhub_model::Project;

const MAX_UNDO_HISTORY: usize = 50;

pub struct UndoManager {
    undo_stack: Vec<ProjectSnapshot>,
    redo_stack: Vec<ProjectSnapshot>,
}

struct ProjectSnapshot {
    label: String,
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
    pub fn push(&mut self, label: &str, project: &Project) {
        self.undo_stack.push(ProjectSnapshot {
            label: label.to_string(),
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
}
