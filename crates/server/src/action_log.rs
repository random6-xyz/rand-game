use crate::model::ValidatedAction;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionLogEntry {
    pub tick: u64,
    pub player_id: u64,
    pub action: ValidatedAction,
    pub result: String,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ActionLog {
    entries: Vec<ActionLogEntry>,
}

impl ActionLog {
    pub fn push(&mut self, entry: ActionLogEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ActionLogEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
