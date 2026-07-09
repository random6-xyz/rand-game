use crate::model::ValidatedAction;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionLogEntry {
    pub tick: u64,
    pub player_id: u64,
    pub action: ValidatedAction,
    pub result: String,
    #[serde(default = "default_count")]
    pub count: u64,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ActionLog {
    entries: Vec<ActionLogEntry>,
}

impl ActionLog {
    pub fn from_entries(entries: Vec<ActionLogEntry>) -> Self {
        Self { entries }
    }

    pub fn push(&mut self, mut entry: ActionLogEntry) {
        if entry.count == 0 {
            entry.count = 1;
        }

        if let Some(last) = self.entries.last_mut()
            && last.can_merge(&entry)
        {
            last.count += entry.count;
            return;
        }

        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ActionLogEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl ActionLogEntry {
    pub fn new(tick: u64, player_id: u64, action: ValidatedAction, result: String) -> Self {
        Self {
            tick,
            player_id,
            action,
            result,
            count: 1,
        }
    }

    pub fn summary(&self) -> String {
        let result = self.normalized_result();
        if self.count <= 1 {
            self.result.clone()
        } else {
            format!("{} ({} times)", result, self.count)
        }
    }

    pub fn can_merge(&self, other: &Self) -> bool {
        self.player_id == other.player_id
            && action_kind(&self.action) == action_kind(&other.action)
            && self.normalized_result() == other.normalized_result()
    }

    fn normalized_result(&self) -> &str {
        result_without_actor_prefix(&self.result).unwrap_or(&self.result)
    }
}

const fn default_count() -> u64 {
    1
}

const fn action_kind(action: &ValidatedAction) -> u8 {
    match action {
        ValidatedAction::Move { .. } => 0,
        ValidatedAction::Mine { .. } => 1,
        ValidatedAction::Build { .. } => 2,
        ValidatedAction::Lift { .. } => 3,
        ValidatedAction::Put { .. } => 4,
        ValidatedAction::Craft { .. } => 5,
        ValidatedAction::Research { .. } => 6,
    }
}

fn result_without_actor_prefix(result: &str) -> Option<&str> {
    let rest = result.strip_prefix("entity ")?;
    let (_, rest) = rest.split_once(' ')?;
    Some(rest)
}

#[cfg(test)]
mod tests {
    use crate::model::{Position, ValidatedAction};

    use super::{ActionLog, ActionLogEntry};

    #[test]
    fn merges_adjacent_identical_actions() {
        let action = ValidatedAction::Mine {
            actor_entity_id: 3,
            target: Position::new(1, 1),
            amount: 1,
        };
        let mut log = ActionLog::default();

        log.push(ActionLogEntry::new(10, 1, action.clone(), "mined".into()));
        log.push(ActionLogEntry::new(11, 1, action, "mined".into()));

        assert_eq!(log.len(), 1);
        assert_eq!(log.entries()[0].tick, 10);
        assert_eq!(log.entries()[0].count, 2);
        assert_eq!(log.entries()[0].summary(), "mined (2 times)");
    }

    #[test]
    fn merges_adjacent_actions_from_different_actors() {
        let first = ValidatedAction::Mine {
            actor_entity_id: 2,
            target: Position::new(0, 0),
            amount: 1,
        };
        let second = ValidatedAction::Mine {
            actor_entity_id: 3,
            target: Position::new(0, 0),
            amount: 1,
        };
        let mut log = ActionLog::default();

        log.push(ActionLogEntry::new(
            12,
            1,
            first,
            "entity 2 mined 1 Energy at (0, 0)".into(),
        ));
        log.push(ActionLogEntry::new(
            12,
            1,
            second,
            "entity 3 mined 1 Energy at (0, 0)".into(),
        ));

        assert_eq!(log.len(), 1);
        assert_eq!(log.entries()[0].count, 2);
        assert_eq!(
            log.entries()[0].summary(),
            "mined 1 Energy at (0, 0) (2 times)"
        );
    }

    #[test]
    fn keeps_non_adjacent_actions_separate() {
        let mine = ValidatedAction::Mine {
            actor_entity_id: 3,
            target: Position::new(1, 1),
            amount: 1,
        };
        let move_action = ValidatedAction::Move {
            actor_entity_id: 3,
            target: Position::new(1, 0),
        };
        let mut log = ActionLog::default();

        log.push(ActionLogEntry::new(10, 1, mine.clone(), "mined".into()));
        log.push(ActionLogEntry::new(10, 1, move_action, "moved".into()));
        log.push(ActionLogEntry::new(11, 1, mine, "mined".into()));

        assert_eq!(log.len(), 3);
        assert!(log.entries().iter().all(|entry| entry.count == 1));
    }
}
