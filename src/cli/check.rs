//! Headless checklist item creation and toggling by item short id.

use std::path::PathBuf;

use serde::Serialize;

use crate::cli::add::has_c0_control;
use crate::domain::{ChecklistItem, DomainError, DomainState};
use crate::store::{default_state_dir, TaskStore};
use uuid::Uuid;

/// The `check` action parsed from argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckAction {
    Add { text: String },
    Toggle { short_id: String },
}

/// A check failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    UnknownTask(Uuid),
    SoftDeletedTask(Uuid),
    EmptyItemText,
    InvalidItemText,
    UnknownItem,
    AmbiguousItem,
    Store(String),
}

impl CheckError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownTask(_) => "unknown-task",
            Self::SoftDeletedTask(_) => "soft-deleted-task",
            Self::EmptyItemText => "empty-item-text",
            Self::InvalidItemText => "invalid-item-text",
            Self::UnknownItem => "unknown-item",
            Self::AmbiguousItem => "ambiguous-item",
            Self::Store(_) => "store-error",
        }
    }
}

/// A successful checklist mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    Added {
        short_id: String,
        text: String,
    },
    Toggled {
        short_id: String,
        text: String,
        done: bool,
    },
}

/// Add one checklist item or toggle one item, refusing before any mutation.
///
/// Refusals are resolved at the boundary under the store lock and never reach
/// the durable write, so a refused check persists nothing.
pub fn run(
    task_id: Uuid,
    action: CheckAction,
    state_dir: Option<PathBuf>,
) -> Result<CheckResult, CheckError> {
    let store = TaskStore::new(state_dir.unwrap_or_else(default_state_dir));
    store
        .locked_transition_if_changed(|domain| {
            let outcome = apply(domain, task_id, &action);
            let changed = outcome.is_ok();
            Ok((outcome, changed))
        })
        .map_err(CheckError::Store)?
}

fn apply(
    domain: &mut DomainState,
    task_id: Uuid,
    action: &CheckAction,
) -> Result<CheckResult, CheckError> {
    // A deleted task's checklist is not scriptable (park/link discipline, not edit's).
    match domain.get(task_id) {
        None => return Err(CheckError::UnknownTask(task_id)),
        Some(task) if task.soft_deleted => return Err(CheckError::SoftDeletedTask(task_id)),
        _ => {}
    }
    match action {
        CheckAction::Add { text } => {
            // Same boundary rule as task titles: any C0 control refuses before trimming.
            if has_c0_control(text) {
                return Err(CheckError::InvalidItemText);
            }
            let text = text.trim();
            if text.is_empty() {
                return Err(CheckError::EmptyItemText);
            }
            let item_id = domain
                .add_checklist_item(task_id, text)
                .map_err(map_domain_error)?;
            let items = &domain.get(task_id).expect("task exists").checklist;
            Ok(CheckResult::Added {
                short_id: item_short_id(items, item_id),
                text: text.to_owned(),
            })
        }
        CheckAction::Toggle { short_id } => {
            let items = domain.get(task_id).expect("task exists").checklist.clone();
            let item_id = match resolve_item_short_id(&items, short_id) {
                ShortIdResolution::One(id) => id,
                ShortIdResolution::Ambiguous => return Err(CheckError::AmbiguousItem),
                ShortIdResolution::None => return Err(CheckError::UnknownItem),
            };
            domain
                .toggle_checklist_item(task_id, item_id)
                .map_err(map_domain_error)?;
            let done = domain
                .get(task_id)
                .expect("task exists")
                .checklist
                .iter()
                .any(|item| item.id == item_id && item.done);
            Ok(CheckResult::Toggled {
                short_id: item_short_id(&items, item_id),
                text: items
                    .iter()
                    .find(|item| item.id == item_id)
                    .expect("toggled item exists")
                    .text
                    .clone(),
                done,
            })
        }
    }
}

/// Domain commands re-check what the boundary already validated; mirror the twins.
fn map_domain_error(error: DomainError) -> CheckError {
    match error {
        DomainError::UnknownId(id) => CheckError::UnknownTask(id),
        DomainError::EmptyItemText => CheckError::EmptyItemText,
        DomainError::UnknownChecklistItem(_) => CheckError::UnknownItem,
        other => CheckError::Store(other.to_string()),
    }
}

/// The result of matching one short id against a task's checklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortIdResolution {
    One(Uuid),
    Ambiguous,
    None,
}

/// Resolve an item short id: an unambiguous prefix of one item id in the list.
///
/// A printed short id is never empty, so the empty operand addresses no item
/// rather than whichever item a bare prefix match would hit.
///
/// SHORTCUT(T-5): linear scan per lookup; fine for typed checklists, index by
/// prefix if lists ever grow unbounded.
pub(crate) fn resolve_item_short_id(items: &[ChecklistItem], short_id: &str) -> ShortIdResolution {
    if short_id.is_empty() {
        return ShortIdResolution::None;
    }
    let mut found = None;
    for item in items {
        if item.id.to_string().starts_with(short_id) {
            if found.is_some() {
                return ShortIdResolution::Ambiguous;
            }
            found = Some(item.id);
        }
    }
    found.map_or(ShortIdResolution::None, ShortIdResolution::One)
}

/// The shortest prefix of one item id that no sibling item id shares.
pub(crate) fn item_short_id(items: &[ChecklistItem], id: Uuid) -> String {
    let full = id.to_string();
    for length in 1..full.len() {
        let prefix = &full[..length];
        if items
            .iter()
            .all(|item| item.id == id || !item.id.to_string().starts_with(prefix))
        {
            return prefix.to_owned();
        }
    }
    full
}

/// One checklist item view for single-task listing, in stored order.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChecklistLine {
    pub(crate) id: Uuid,
    pub(crate) done: bool,
    pub(crate) short_id: String,
    pub(crate) text: String,
}

/// The per-item views a single-task listing prints, in stored order.
pub(crate) fn checklist_lines(items: &[ChecklistItem]) -> Vec<ChecklistLine> {
    items
        .iter()
        .map(|item| ChecklistLine {
            id: item.id,
            done: item.done,
            short_id: item_short_id(items, item.id),
            text: item.text.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, text: &str, done: bool) -> ChecklistItem {
        ChecklistItem {
            id: Uuid::parse_str(id).expect("test item id"),
            text: text.into(),
            done,
        }
    }

    #[test]
    fn short_ids_stay_the_shortest_unambiguous_prefix() {
        let items = [
            item("abcdef01-0000-4000-8000-000000000001", "one", false),
            item("abcdef02-0000-4000-8000-000000000002", "two", true),
        ];
        assert_eq!(item_short_id(&items, items[0].id), "abcdef01");
        assert_eq!(item_short_id(&items, items[1].id), "abcdef02");

        let alone = [items[0].clone()];
        assert_eq!(item_short_id(&alone, items[0].id), "a");
    }

    #[test]
    fn prefix_resolution_is_one_ambiguous_or_none() {
        let items = [
            item("abcdef01-0000-4000-8000-000000000001", "one", false),
            item("abcdef01-0000-4000-8000-000000000002", "two", false),
            item("99999999-0000-4000-8000-000000000003", "three", false),
        ];
        assert_eq!(
            resolve_item_short_id(&items, "abcdef01-0000-4000-8000-000000000001"),
            ShortIdResolution::One(items[0].id)
        );
        assert_eq!(
            resolve_item_short_id(&items, "abcdef01"),
            ShortIdResolution::Ambiguous
        );
        assert_eq!(
            resolve_item_short_id(&items, "99999999-0000-4000-8000-000000000003"),
            ShortIdResolution::One(items[2].id)
        );
        assert_eq!(
            resolve_item_short_id(&items, "12341234"),
            ShortIdResolution::None
        );
        let sole = [items[2].clone()];
        assert_eq!(
            resolve_item_short_id(&sole, ""),
            ShortIdResolution::None,
            "the empty operand must address no item, not the sole one"
        );
    }
}
