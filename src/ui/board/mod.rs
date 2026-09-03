//! Queue-board presentation, split by state, commands, reduction, chrome, and drawing.

mod apply;
mod chrome;
mod commands;
mod draw;
mod model;

pub use crate::ui::tier::{
    resolve_responsive, FocusedSurface, ResponsiveGeometry, ResponsivePresentation, WideStage,
    WIDE_SPLIT_MIN_WIDTH,
};
pub use apply::{apply_intent, board_intent_may_persist};
pub use chrome::DELETE_NOTICE_UNDO;
pub use commands::{resolve_board_command, BoardCommand, CommandSurface};
pub use draw::{board_hit_map, board_verb_items, draw_board};
pub use model::{
    project_option_label, BoardInputMode, BoardModel, BoardTab, IntentOutcome, ProjectScopeOption,
    SaveResolution, WalkthroughOutcome, BOARD_TITLE,
};
