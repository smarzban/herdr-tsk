//! Application entry: mode select, load store/context, run Board or Capture UI.

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, SystemTime};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::DefaultTerminal;

use crate::attention::{self, RefreshResult};
use crate::config::{default_config_dir, SettingsRecord, WalkthroughRecord};
use crate::context::{build_snapshot, InvocationSnapshot, RawHostContext};
use crate::dispatch::{cleanup_dispatch_attempt, resume_dispatch_attempt, DispatchRecoveryResult};
use crate::domain::{DomainError, DomainState};
use crate::host::{HerdrHost, HostPorts};
use crate::save_recovery::SaveRecovery;
use crate::store::{default_state_dir, StoreError, TaskStore};
use crate::ui::board::{
    apply_dispatch_recovery_result, apply_intent, board_intent_may_persist, draw_board,
    resolve_board_command, BoardInputMode, BoardModel, IntentOutcome, SaveResolution,
    WalkthroughOutcome,
};
use crate::ui::capture::{
    apply_capture_intent, draw_capture, CaptureModel, CaptureOutcome, TITLE_REQUIRED_MESSAGE,
};
use crate::ui::input::{
    map_board_form_key, map_capture_key_state, map_capture_paste_state, map_edit_paste,
    map_key_with, BoardIntent, CaptureIntent,
};
use crate::ui::mouse::{
    capture_layout_for_model, enable_terminal_input, keyboard_enhancement_supported,
    map_capture_mouse,
};
use crate::ui::scheduler;

/// In-flight off-thread host dispatch.
struct PendingDispatch {
    rx: Receiver<DispatchRecoveryResult>,
}

/// Env var set by open-capture launcher for Capture UI mode.
pub const MODE_ENV: &str = "TSK_MODE";

/// One binary, two modes (Board default; Capture for quick-capture).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Primary Tasks board.
    Board,
    /// Capture UI form.
    Capture,
}

/// Resolve the TUI mode from an explicit mode-env value and remaining argv (after argv0).
///
/// Default is [`AppMode::Board`]. `capture` (env string or arg) selects Capture. Process command
/// validation belongs to [`crate::cli::router`]. Prefer this pure helper in tests.
pub fn resolve_mode_from<S: AsRef<str>>(
    mode_env: Option<&str>,
    args: impl IntoIterator<Item = S>,
) -> AppMode {
    if mode_env.is_some_and(|v| v.eq_ignore_ascii_case("capture")) {
        return AppMode::Capture;
    }

    let mut iter = args.into_iter();
    // Skip program name when present.
    let _argv0 = iter.next();
    for arg in iter {
        if arg.as_ref().eq_ignore_ascii_case("capture") {
            return AppMode::Capture;
        }
    }
    AppMode::Board
}

/// Resolve the TUI mode from `TSK_MODE` and remaining argv (after argv0).
pub fn resolve_mode<S: AsRef<str>>(args: impl IntoIterator<Item = S>) -> AppMode {
    resolve_mode_from(env::var(MODE_ENV).ok().as_deref(), args)
}

fn load_snapshot() -> InvocationSnapshot {
    let raw = RawHostContext::from_env();
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    build_snapshot(&raw, cwd)
}

/// Load store + context into domain and board view-model (no TTY).
///
/// Does not poll the host: the board loop never calls [`run_attention_cycle`]
/// after load; the function remains for the tests that drive it directly.
pub fn load_board() -> Result<(TaskStore, DomainState, BoardModel), Box<dyn Error>> {
    let store = TaskStore::new(default_state_dir());
    let state = store.load()?;
    let snapshot = load_snapshot();
    let mut model = BoardModel::from_domain(&state, snapshot.this_repo.clone());
    model.verb_modifier = SettingsRecord::new(default_config_dir()).verb_modifier();
    Ok((store, state, model))
}

/// One attention poll: merge disk → host pane list → reactor → optional persist + model sync.
///
/// Reloads the disk snapshot before applying so attention never acts on an older local copy.
/// Persists only when human status actually changed (no-op observation must not rewrite disk).
/// Host list failure skips apply (returns empty result). Used on board open and while the
/// board is focused.
pub fn run_attention_cycle(
    store: &TaskStore,
    domain: &mut DomainState,
    model: &mut BoardModel,
    host: &dyn HostPorts,
) -> RefreshResult {
    // Freshest disk first: poll must not apply against stale tasks then stomp newer edits.
    if let Ok(disk) = store.load() {
        domain.merge_tasks_from_disk(&disk);
    }
    match attention::poll_host(domain, host) {
        Ok(result) if result.status_changed() => {
            model.apply_attention_result(&result);
            if let Err(e) = store.reload_merge_save(domain) {
                // Attention polls have no interactive Retry/Cancel surface. Keep the working
                // state visible rather than rendering the prior persisted status as current.
                model.sync_from_domain(domain);
                model.set_message(format!("attention save failed: {e}"));
            } else {
                model.sync_from_domain(domain);
                let n = result.status_changed.len();
                model.set_message(format!(
                    "attention · {n} task{} updated from agent",
                    if n == 1 { "" } else { "s" }
                ));
            }
            result
        }
        Ok(result) => {
            // Disk merge (and any pure no-ops) still need model sync for concurrent edits.
            model.apply_attention_result(&result);
            model.sync_from_domain(domain);
            result
        }
        Err(_) => {
            // An unavailable snapshot must not turn links stale.
            let result = RefreshResult::unavailable(domain);
            model.apply_attention_result(&result);
            model.sync_from_domain(domain);
            result
        }
    }
}

/// Retained test seam for a one-off attention refresh.
///
/// the board loop never calls this on the frame path.
pub fn run_board_open_refresh(
    store: &TaskStore,
    domain: &mut DomainState,
    model: &mut BoardModel,
    host: &dyn HostPorts,
) -> RefreshResult {
    run_attention_cycle(store, domain, model, host)
}

/// The one walkthrough read on the open path: no record means the card opens.
///
/// the board loop never calls this on the launch path; it remains for the
/// tests that drive it directly. A record present means no card at all.
///
/// Reading has no error channel by construction (see [`WalkthroughRecord::is_dismissed`]): an
/// unreadable record reads as not dismissed, so the worst a broken config location can do is
/// show the walkthrough again, never keep the board shut.
///
/// The open goes through [`BoardIntent::OpenWalkthrough`] -- the same intent the palette's
/// replay command dispatches -- so launch and replay are one route with one state guard, and
/// the reducer's refusal to cover a state that owns its own controls applies to both.
pub fn open_walkthrough_for_launch(
    domain: &mut DomainState,
    model: &mut BoardModel,
    record: &WalkthroughRecord,
) {
    if record.is_dismissed() {
        return;
    }
    // The reducer's `OpenWalkthrough` arm has no failure mode, and a board that could not
    // raise its onboarding card is still a usable board: nothing here fails the launch.
    let _ = apply_intent(domain, model, BoardIntent::OpenWalkthrough, None, None);
}

/// Record the dismissal an open walkthrough just reported, at most once per close.
///
/// Call once per board loop iteration, which is the hand-off contract
/// [`BoardModel::take_walkthrough_outcome`] states. Two of the four outcomes are the user
/// dismissing the card and each records; `Unpresentable` and `Interrupted` are the board
/// taking the card away with nothing answered, so they record nothing and the walkthrough
/// returns at the next launch. Taking the outcome is what makes one close one write: a second
/// close of an already-closed card reports nothing to take.
///
/// One *dismissal* is one write, which is not the same as one write per install: a card
/// replayed from the palette and dismissed again reports again and writes again, over an
/// already-dismissed record. That is harmless (the payload is the same constant and the write
/// is atomic) and it is the honest reading of the criterion -- what must never happen is a
/// second write for a single dismissal, or a retry on a keystroke that dismissed nothing.
///
/// `write` is a closure rather than the record itself so a test can count the attempts:
/// "exactly once per dismissal, never retried per keystroke" is a property of this function,
/// not of whatever the file ends up holding.
///
/// A failed write is **presented, never propagated**: the board keeps running with the
/// failure on the chrome row. The outcome has already been taken, so no later keystroke
/// retries it -- one attempt per dismissal is the whole retry policy, and the only cost of the
/// lost record is that the walkthrough appears once more.
pub fn record_walkthrough_dismissal(
    model: &mut BoardModel,
    write: impl FnOnce() -> Result<(), StoreError>,
) {
    let Some(outcome) = model.take_walkthrough_outcome() else {
        return;
    };
    match outcome {
        WalkthroughOutcome::Completed | WalkthroughOutcome::Skipped => {
            if let Err(error) = write() {
                model.set_message(format!("walkthrough dismissal save failed: {error}"));
            }
        }
        WalkthroughOutcome::Unpresentable | WalkthroughOutcome::Interrupted => {}
    }
}

/// What the board's wait for input answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePoll {
    /// Nothing arrived inside the poll window: tick the background work and come round again.
    Idle,
    /// An event is queued and ready to read.
    Event,
}

/// The board loop's input-wait duration for one frame.
///
/// the paints no animation yet, so every real call site passes `false` here; the parameter
/// stays so a future animation source can shorten the wait without moving this call site.
/// Wired straight through [`scheduler::next_wait`] rather than a fixed constant -- the base
/// tick and the short-tick floor stay the Frame Scheduler's, not a second copy in the loop.
pub fn board_poll_duration(active_animations: bool) -> Duration {
    scheduler::next_wait(active_animations, scheduler::DEFAULT_BASE_TICK)
}

/// One board frame in the only order survives: **settle, paint, wait**.
///
/// The settle lives here rather than at the caller's convenience because the *placement* is
/// the property, not the arithmetic. `run_board`'s loop body is a run of `continue`s -- an
/// unmapped key, an area gate, a mouse that hit nothing, and above all the poll timeout -- and
/// a settle that ended up behind any of them would wait on an event that a user who walked
/// away never produces. Owning the paint and the wait is what makes that impossible to get
/// wrong: the wait cannot answer [`FramePoll::Idle`] without the settle having already run,
/// because both are inside this call.
///
/// So the guard is structural. Moving the settle out of here to "handle it after the event"
/// means changing this signature, and the tests that drive it (`tests/e2e_persist.rs`: the
/// idle frame, and the settle-before-paint order) stop building.
///
/// This function also owns the poll duration: it computes
/// [`board_poll_duration`] itself and hands it to `wait`, so the call site cannot substitute a
/// fixed constant of its own -- the only way to change the wait is to change this function.
pub fn board_frame(
    model: &mut BoardModel,
    write: impl FnOnce() -> Result<(), StoreError>,
    paint: impl FnOnce(&BoardModel) -> io::Result<()>,
    wait: impl FnOnce(Duration) -> io::Result<bool>,
) -> io::Result<FramePoll> {
    record_walkthrough_dismissal(model, write);
    paint(model)?;
    if wait(board_poll_duration(false))? {
        Ok(FramePoll::Event)
    } else {
        Ok(FramePoll::Idle)
    }
}

/// One frame plus, only on `FramePoll::Idle`, the store-only revalidation).
///
/// `run_board`'s loop calls this and nothing else to decide whether a tick was idle; there is
/// no separate call the loop makes only when idle for a later edit to drop in silence, unlike
/// the shape a prior round left (`board_frame`, then a sibling `revalidate_board_from_store`
/// call in the loop's own `if poll == FramePoll::Idle` arm) that a test exercising both calls
/// only through their own bodies could never prove `run_board` actually wired together. A test
/// that drives this one function and observes the merge lands exactly the same assertion
/// `run_board`'s own Idle branch depends on, because it is the same code, not a copy of it.
#[allow(clippy::too_many_arguments)]
pub fn board_idle_tick(
    model: &mut BoardModel,
    write: impl FnOnce() -> Result<(), StoreError>,
    paint: impl FnOnce(&BoardModel) -> io::Result<()>,
    wait: impl FnOnce(Duration) -> io::Result<bool>,
    store: &TaskStore,
    domain: &mut DomainState,
    watch: &mut StoreWatch,
    save_recovery: &SaveRecovery<DomainState>,
) -> io::Result<FramePoll> {
    let poll = board_frame(model, write, paint, wait)?;
    if poll == FramePoll::Idle {
        revalidate_board_from_store(store, domain, model, watch, save_recovery);
    }
    Ok(poll)
}

/// Load store + context into a board view-model (no TTY).
pub fn load_board_model() -> Result<BoardModel, Box<dyn Error>> {
    let (_store, _state, model) = load_board()?;
    Ok(model)
}

/// Binary entry used by `main`. Default mode is the Tasks board.
///
/// Pass `capture` argv (or `TSK_MODE=capture`) for popup-style Capture UI.
pub fn run(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<(), Box<dyn Error>> {
    match resolve_mode(args) {
        AppMode::Board => run_board(),
        AppMode::Capture => run_capture(),
    }
}

/// Standalone capture mode: form loop, exit after save or cancel (popup-style).
fn run_capture() -> Result<(), Box<dyn Error>> {
    let store = TaskStore::new(default_state_dir());
    let mut domain = store.load()?;
    let snapshot = load_snapshot();
    let mut model = CaptureModel::from_snapshot(&snapshot);

    // Query before the alternate screen is entered: it can block on a terminal round-trip,
    // and a blank alternate screen is what the user would be staring at meanwhile.
    let keyboard_enhancement = keyboard_enhancement_supported();
    ratatui::run(|terminal| -> io::Result<()> {
        let _input = enable_terminal_input(keyboard_enhancement)?;
        capture_form_loop(terminal, &store, &mut domain, &snapshot, &mut model)
    })?;
    Ok(())
}

fn run_board() -> Result<(), Box<dyn Error>> {
    let (store, mut domain, mut model) = load_board()?;
    let walkthrough = WalkthroughRecord::new(default_config_dir());
    // `load_board` just read the store, so seed the watch from that snapshot: the first idle
    // tick must not immediately re-merge what is already loaded.
    let mut store_watch = StoreWatch::seeded(&store);
    // the board frame path does no host polling and does not
    // auto-open the walkthrough on launch. `load_board` is the whole open path; the first
    // paint below is of that model, unrefreshed. `run_attention_cycle`,
    // `run_board_open_refresh`, and `open_walkthrough_for_launch` remain for the tests that
    // drive them directly -- this loop simply stops calling them.
    //
    // the Idle branch below is not pure silence, though. Every idle tick revalidates the
    // store -- a `stat` on tasks.json, and only when its mtime/size changed does it pay for
    // `store.load()` + `merge_tasks_from_disk` + `sync_from_domain` (see
    // [`revalidate_board_from_store`]) -- so a quick-capture popup (a separate process writing
    // the same file) becomes visible on an open, idle board without this board ever running a
    // persisting intent. That is disk-merge, not host polling: NC-1's no-attention-polling
    // constraint is about the host, not the store, so it stays satisfied. Save recovery still
    // gates it off via `board_background_work_allowed`, same as it gates the dispatch-recovery
    // reload above.

    // Query before the alternate screen is entered: it can block on a terminal round-trip,
    // and a blank alternate screen is what the user would be staring at meanwhile.
    let keyboard_enhancement = keyboard_enhancement_supported();
    ratatui::run(|terminal| -> io::Result<()> {
        let _input = enable_terminal_input(keyboard_enhancement)?;
        let mut pending_dispatch: Option<PendingDispatch> = None;
        let mut save_recovery = SaveRecovery::new();
        loop {
            // A completed worker must remain queued while the failed save owns the displayed
            // working state. Applying it would reload disk and replace SaveRecovery.
            match process_pending_dispatch(&mut pending_dispatch, &save_recovery) {
                PendingDispatchPoll::Finished(result) => match store.load() {
                    Ok(reloaded) => {
                        // The worker persists every recovery transition. Replace the local
                        // snapshot before rendering so completed/cleaned attempts disappear.
                        domain = reloaded;
                        apply_dispatch_recovery_result(&domain, &mut model, result);
                    }
                    Err(error) => {
                        model.set_message(format!("dispatch recovery reload failed: {error}"));
                    }
                },
                PendingDispatchPoll::Disconnected => {
                    model.set_message("dispatch recovery worker disconnected");
                }
                PendingDispatchPoll::Pending => {}
            }

            // Settle, paint, then wait -- the the board frame path does no host polling
            //, so the wait is only the Frame Scheduler's idle floor
            //, never an attention tick. All three are one call because the order is
            // the correctness property: the walkthrough's report -- from the previous
            // iteration's event or from the dispatch recovery just applied above -- is
            // written before this frame is painted and before the wait can time out into the
            // `continue` below.
            let poll = board_idle_tick(
                &mut model,
                || walkthrough.record_dismissed(),
                |model: &BoardModel| terminal.draw(|frame| draw_board(frame, model)).map(|_| ()),
                event::poll,
                &store,
                &mut domain,
                &mut store_watch,
                &save_recovery,
            )?;
            if poll == FramePoll::Idle {
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let area = terminal_area(terminal)?;
                    // The painted mode owns the keyboard: `resolve_board_surface` resolves the
                    // same input mode at every terminal size (`board_input_mode_for_area` no
                    // longer varies by area, per fix 1's gate removal), so no held-open modal
                    // can end up invisible under a shrunk pane and leave q / Esc unreachable.
                    let mode = resolve_board_surface(area, &mut model);
                    let Some(intent) = board_keyboard_intent(&model, mode, key) else {
                        continue;
                    };
                    let Some(intent) = board_intent_for_area(area, intent) else {
                        continue;
                    };
                    // A command-surface confirmation dispatches its existing intent route.
                    let Some(intent) = resolve_board_command(&mut model, intent) else {
                        continue;
                    };
                    if handle_board_intent(
                        &store,
                        &mut domain,
                        &mut model,
                        intent,
                        &mut pending_dispatch,
                        &mut save_recovery,
                    )? {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    let area = terminal_area(terminal)?;
                    let Some(intent) = board_mouse_intent(area, &mut model, mouse) else {
                        continue;
                    };
                    if handle_board_intent(
                        &store,
                        &mut domain,
                        &mut model,
                        intent,
                        &mut pending_dispatch,
                        &mut save_recovery,
                    )? {
                        break;
                    }
                }
                Event::Paste(text) => {
                    let area = terminal_area(terminal)?;
                    let Some(intent) = board_paste_intent(area, &mut model, &text) else {
                        continue;
                    };
                    if handle_board_intent(
                        &store,
                        &mut domain,
                        &mut model,
                        intent,
                        &mut pending_dispatch,
                        &mut save_recovery,
                    )? {
                        break;
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Whether save recovery permits the board to apply background state changes.
fn board_background_work_allowed(recovery: &SaveRecovery<DomainState>) -> bool {
    !recovery.is_pending()
}

/// Cheap idle-tick change detector for the store's on-disk document.
///
/// Holds only `tasks.json`'s last-seen modification time + length, so the frame loop's Idle
/// branch -- which runs about 4 times a second --
/// pays for a `stat`, not a parse, on every tick where nothing changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StoreWatch {
    last_seen: Option<(SystemTime, u64)>,
}

impl StoreWatch {
    /// Unseeded: the first check always reports changed. Prefer [`Self::seeded`] right after a
    /// load so the first idle tick does not immediately re-merge what was just read.
    pub fn new() -> Self {
        Self { last_seen: None }
    }

    /// Seed from the store's current on-disk signature (e.g. right after `load_board()`).
    pub fn seeded(store: &TaskStore) -> Self {
        Self {
            last_seen: store_state_signature(store),
        }
    }

    /// `Some(signature)` when the store's on-disk signature differs from what was last
    /// recorded; `None` when unchanged. Does **not** update `last_seen` -- call
    /// [`Self::record`] with the returned signature only once the load it gates actually
    /// succeeds, so a transient read failure leaves the watch exactly where it was
    /// and the very next tick tries again instead of treating the failed read as caught up.
    fn poll(&self, store: &TaskStore) -> Option<Option<(SystemTime, u64)>> {
        let current = store_state_signature(store);
        if current == self.last_seen {
            None
        } else {
            Some(current)
        }
    }

    /// Record a signature already returned by [`Self::poll`], after the load it gated
    /// succeeded.
    fn record(&mut self, signature: Option<(SystemTime, u64)>) {
        self.last_seen = signature;
    }
}

/// `tasks.json`'s modification time + length. `None` when the file does not exist yet (a
/// fresh, never-saved store) or its metadata could not be read.
///
/// Store internals (the lock protocol, the atomic-write path) are frozen; this only stats the
/// document store.rs already names in its own doc comment, it does not reimplement any of
/// store.rs's load/save/lock contract.
fn store_state_signature(store: &TaskStore) -> Option<(SystemTime, u64)> {
    let metadata = fs::metadata(store.path().join("tasks.json")).ok()?;
    let modified = metadata.modified().ok()?;
    Some((modified, metadata.len()))
}

/// On the frame loop's Idle branch: revalidate the store cheaply, no host calls.
///
/// Only when [`StoreWatch::changed`] reports a changed mtime/size does this pay for
/// `store.load()` + [`DomainState::merge_tasks_from_disk`] + [`BoardModel::sync_from_domain`],
/// so a quick-capture popup (a separate process writing the same `tasks.json`) becomes
/// visible on an open, idle board without a persisting intent from this board and without a
/// host call.
///
/// Skips entirely while a failed save owns the displayed working state
/// ([`board_background_work_allowed`]): reloading disk under save recovery would replace the
/// working state the user is retrying. An open edit session is never redirected either --
/// `sync_from_domain` reanchors selection by id and never touches the edit binding.
///
/// A failed `store.load()` (e.g. a transient read error) does not advance the watch:
/// the signature [`StoreWatch::poll`] observed is recorded only once this load has actually
/// succeeded and merged, so a failed read leaves `last_seen` exactly where it was and the
/// very next idle tick still sees the store as changed and tries again -- instead of, as
/// before this fix, treating the failed read as caught up and staying stale until the next
/// write produces a signature this watch had not already recorded for nothing.
///
/// Returns whether a merge actually ran, so tests can assert the cheap path stayed cheap.
pub fn revalidate_board_from_store(
    store: &TaskStore,
    domain: &mut DomainState,
    model: &mut BoardModel,
    watch: &mut StoreWatch,
    save_recovery: &SaveRecovery<DomainState>,
) -> bool {
    if !board_background_work_allowed(save_recovery) {
        return false;
    }
    let Some(signature) = watch.poll(store) else {
        return false;
    };
    let Ok(disk) = store.load() else {
        return false;
    };
    domain.merge_tasks_from_disk(&disk);
    model.sync_from_domain(domain);
    watch.record(signature);
    true
}

/// Result of polling a pending dispatch without replacing save-recovery state.
enum PendingDispatchPoll {
    Pending,
    Finished(DispatchRecoveryResult),
    Disconnected,
}

/// Deliver a finished dispatch only when it cannot replace SaveRecovery's retained model.
fn process_pending_dispatch(
    pending_dispatch: &mut Option<PendingDispatch>,
    recovery: &SaveRecovery<DomainState>,
) -> PendingDispatchPoll {
    if !board_background_work_allowed(recovery) {
        return PendingDispatchPoll::Pending;
    }
    let Some(pending) = pending_dispatch.take() else {
        return PendingDispatchPoll::Pending;
    };
    match pending.rx.try_recv() {
        Ok(result) => PendingDispatchPoll::Finished(result),
        Err(TryRecvError::Empty) => {
            *pending_dispatch = Some(pending);
            PendingDispatchPoll::Pending
        }
        Err(TryRecvError::Disconnected) => PendingDispatchPoll::Disconnected,
    }
}
fn terminal_area(terminal: &DefaultTerminal) -> io::Result<Rect> {
    let size = terminal.size()?;
    Ok(Rect::new(0, 0, size.width, size.height))
}

/// One board intent plus the persistence baseline it must recover to on a failed save.
pub struct BoardSaveContext<'a> {
    pub baseline: DomainState,
    pub intent: BoardIntent,
    pub snapshot: Option<&'a InvocationSnapshot>,
    pub host: Option<&'a dyn HostPorts>,
}

/// Apply one board intent through the save-recovery boundary.
///
/// A failed save transfers the working state into [`SaveRecovery`] and leaves the caller's
/// domain empty until explicit Retry promotes that state or Cancel restores the baseline. The
/// board model continues to render the retained working snapshot throughout recovery.
/// Which mapper owns a board keypress. The shared form field map applies only while a
/// field of the open form actually has focus (or its scope dropdown is open). The task
/// page's view mode keeps its own keymap even though a form is open -- otherwise a bare
/// `e` on the page would be inserted into the title draft instead of entering edit mode.
fn board_keyboard_intent(
    model: &BoardModel,
    mode: BoardInputMode,
    key: crossterm::event::KeyEvent,
) -> Option<BoardIntent> {
    // Allowlist, not a denylist: the form mapper owns the keyboard ONLY while the resolved
    // mode is genuinely one of the form's own field/dropdown states. `input_mode()` lets a
    // popup or command surface OUTRANK the form's mode (see its `match self.popup`), so an
    // overriding mode can arrive with a form still open. Excluding just one such mode left
    // every other one swallowed: under `SaveRecovery` the form mapper ate `r`, `c` and Esc,
    // so `map_save_recovery`'s Retry/Cancel never ran and a failed save could not be
    // resolved (or escaped) from the keyboard at all.
    let form_field_mode = matches!(
        mode,
        BoardInputMode::EditTitle
            | BoardInputMode::EditNotes
            | BoardInputMode::EditThread
            | BoardInputMode::EditScope
            | BoardInputMode::FormScopeDropdown
    );
    // `form_focus()` is Some whenever a form is open, so this reads as an invariant. It is still
    // not worth an `expect` here: this runs on every keypress inside the raw-mode event loop, so
    // a panic would abort with the terminal still in raw mode and take the user's shell with it.
    // Falling through to `map_key` degrades to normal-mode routing instead of dying.
    match model.form_focus().filter(|_| form_field_mode) {
        Some(focus) => map_board_form_key(focus, mode == BoardInputMode::FormScopeDropdown, key),
        None => map_key_with(mode, key, model.verb_modifier),
    }
}

pub fn apply_board_intent_with_save_recovery(
    domain: &mut DomainState,
    model: &mut BoardModel,
    recovery: &mut SaveRecovery<DomainState>,
    context: BoardSaveContext<'_>,
    mut persist: impl FnMut(&mut DomainState) -> Result<(), String>,
) -> Result<IntentOutcome, DomainError> {
    let BoardSaveContext {
        baseline,
        intent,
        snapshot,
        host,
    } = context;
    // Resolve a command-surface confirmation before the recovery gate, so the surface
    // dispatches the same intent the direct route would and gains no exemption.
    let Some(intent) = resolve_board_command(model, intent) else {
        return Ok(IntentOutcome::None);
    };
    if recovery.is_pending() {
        match intent {
            // Leaving the board resolves nothing and persists nothing, but the user must never
            // be held in the session by an unresolved save.
            BoardIntent::Quit => return Ok(IntentOutcome::Quit),
            BoardIntent::RetrySave => {
                // The mouse hands this intent in already resolved, so close the surface here
                // exactly as the keyboard's ConfirmCommand route does.
                model.close_command_surface();
                if let Some(working) = recovery.retry(|working| persist(working)) {
                    *domain = working;
                    model.release_task_edit_save();
                    model.sync_from_domain(domain);
                    model.end_save_recovery(SaveResolution::Retried);
                    if !model.has_saved_task() {
                        model.set_message("saved");
                    }
                    return Ok(IntentOutcome::Persisted);
                }
                model.begin_save_recovery(recovery.error().unwrap_or("save failed"));
                return Ok(IntentOutcome::None);
            }
            BoardIntent::CancelSave => {
                model.close_command_surface();
                *domain = recovery.cancel().expect("pending recovery has a baseline");
                model.sync_from_domain(domain);
                let cancelled_quick_add = model.end_save_recovery(SaveResolution::Cancelled);
                if !cancelled_quick_add {
                    model.set_message("save cancelled");
                }
                return Ok(IntentOutcome::None);
            }
            // Navigation and presentation state mutate nothing.
            BoardIntent::SelectNext
            | BoardIntent::SelectPrev
            | BoardIntent::SelectIndex(_)
            | BoardIntent::OpenCommandPalette
            | BoardIntent::CommandNext
            | BoardIntent::CommandPrev
            | BoardIntent::CommandQueryInsert(_)
            | BoardIntent::CommandQueryInsertText(_)
            | BoardIntent::CommandQueryBackspace
            | BoardIntent::CloseCommandSurface
            | BoardIntent::OpenHelp
            | BoardIntent::CloseLayer
            | BoardIntent::OpenTaskPage
            | BoardIntent::PeekDetail
            | BoardIntent::CollapseDetail
            | BoardIntent::PageScrollUp
            | BoardIntent::PageScrollDown
            | BoardIntent::PageWheelScrollUp
            | BoardIntent::PageWheelScrollDown
            | BoardIntent::ToggleDoneDrawer => {
                return apply_intent(domain, model, intent, None, None)
            }
            _ => {
                model.begin_save_recovery(recovery.error().unwrap_or("save failed"));
                return Ok(IntentOutcome::None);
            }
        }
    }

    // decision 8: judged against the durable record, before the reducer runs, so no mutation
    // happens and the session (mode, draft, cursor, binding) survives the refusal intact. The
    // caller presents the returned error on the message row. Both save chords on a line
    // editor are this one surface, so they refuse identically: Enter (`ConfirmEdit`)
    // and Ctrl+Enter (`ConfirmEditNext`).
    if matches!(
        intent,
        BoardIntent::ConfirmEdit | BoardIntent::ConfirmEditNext
    ) {
        if let Some(refusal) = confirm_edit_refusal_against_the_record(&baseline, model) {
            return Err(refusal);
        }
    }

    let holds_task_edit = matches!(
        intent,
        BoardIntent::ConfirmEdit | BoardIntent::ConfirmEditNext
    ) && model.edit_target().is_some()
        // Step saves use the same intent but their own pending-save state. Holding the task
        // form here would retain stale task-edit state that was never created.
        && model.input_mode() != BoardInputMode::EditStep;
    if holds_task_edit {
        model.hold_task_edit_save();
    }
    let outcome = match apply_intent(domain, model, intent, snapshot, host) {
        Ok(outcome) => outcome,
        Err(error) => {
            if holds_task_edit {
                model.release_task_edit_save();
            }
            return Err(error);
        }
    };
    if outcome != IntentOutcome::Persist {
        if holds_task_edit {
            model.release_task_edit_save();
        }
        return Ok(outcome);
    }
    if let Err(error) = persist(domain) {
        let working = std::mem::replace(domain, DomainState::new());
        recovery.fail(baseline, working, error);
        model.begin_save_recovery(recovery.error().unwrap_or("save failed"));
        return Ok(IntentOutcome::None);
    }
    model.release_task_edit_save();
    model.sync_from_domain(domain);
    Ok(IntentOutcome::Persisted)
}

/// Input mode the resolved board mode owns for this area.
///
/// the legacy Resize band used to force `Normal` here on the premise that a modal open
/// before the pane shrank was no longer painted. The Tier
/// Layout Resolver's queue overlay paints an open field editor as a full-screen takeover at
/// every size regardless of `BoardMode`, so that premise no longer holds: forcing
/// `Normal` while the overlay still shows, say, an open title editor would make its keys
/// silently reach the board reducer instead of the field, both losing the keystroke and (once
/// the intent gate below is gone too) risking a stray mutation behind a visibly open editor.
/// Every area now keeps the model's own input mode, the same as Wide and Browse always did.
fn board_input_mode_for_area(_area: Rect, mode: BoardInputMode) -> BoardInputMode {
    mode
}

/// Resolve the surface this area actually paints, before any input is mapped against it.
///
/// the legacy Resize band (<50x18) used to force-close popup/help/detail/command-surface
/// /walkthrough here and swallow every mutating intent in [`board_intent_for_area`] below it,
/// which left's compact-tier controls dead down to 40x10. `BoardMode` still classifies
/// the painted layout (`board_input_mode_for_area`, `board_layout*`), but no longer gates or
/// force-closes anything: every surface is routed the same way at every supported size.
fn resolve_board_surface(area: Rect, model: &mut BoardModel) -> BoardInputMode {
    board_input_mode_for_area(area, model.input_mode())
}

/// Route an intent through unchanged; the area no longer gates it.
///
/// Kept as the named choke point every key/paste/mouse route already passes through, so a
/// future area-dependent rule (if any) has one place to land, and so the call sites and their
/// tests do not need to change shape.
fn board_intent_for_area(_area: Rect, intent: BoardIntent) -> Option<BoardIntent> {
    Some(intent)
}

/// Route a bracketed paste to the board intent the painted surface accepts.
///
/// A paste arrives as `Event::Paste`, so it cannot go through `map_key`; it still passes the
/// same surface resolution, area gate, and command resolution the key route applies, so a
/// paste can never reach a route a key press could not.
fn board_paste_intent(area: Rect, model: &mut BoardModel, text: &str) -> Option<BoardIntent> {
    let mode = resolve_board_surface(area, model);
    let intent = map_edit_paste(mode, text)?;
    let intent = board_intent_for_area(area, intent)?;
    resolve_board_command(model, intent)
}

/// Route a mouse event to the board intent the painted frame accepts.
///
/// `resolve_board_surface`'s side effects run first, exactly as for a key press: a shrink
/// past the classic Resize threshold collapses whatever surface it left open before the
/// hit-map is rebuilt, so a click can never land on a control the shrink already withdrew.
/// The hit-map itself comes from [`crate::ui::board::board_hit_map`], the same painter
/// [`draw_board`] uses, so the click and the screen the user is looking at can never
/// disagree about where a control is. The area gate and command resolution afterward are
/// the same ones the key and paste routes already pass through.
fn board_mouse_intent(
    area: Rect,
    model: &mut BoardModel,
    mouse: crossterm::event::MouseEvent,
) -> Option<BoardIntent> {
    // crossterm's `EnableMouseCapture` turns on all-motion tracking, so
    // a bare pointer move over the pane arrives as an `Event::Mouse` too -- at a rate that
    // can saturate the event loop. `map_board_mouse` only ever acts on
    // `Down(Left)`/`ScrollUp`/`ScrollDown` (every other kind falls through its own leading
    // match to `None`), so gate on the event kind *before* paying for a full board paint
    // (`board_hit_map` renders into a scratch `TestBackend`) and before
    // `resolve_board_surface`'s side effects run for a kind that was never going to
    // dispatch anything. This also stops `resolve_board_surface` firing on motion, which
    // was itself a second reason to gate first.
    use crossterm::event::{MouseButton, MouseEventKind};
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) {
        return None;
    }
    resolve_board_surface(area, model);
    // Minor 3: `board_hit_map` renders a full scratch paint
    // (`TestBackend`) to recover the hit-map, but `map_board_mouse` only ever reads it for
    // `Down(Left)` -- its `ScrollUp`/`ScrollDown` arm resolves through `wheel_board_intent`
    // before the hit-map parameter is touched at all. Continuous scrolling arrives in
    // bursts, and the scratch paint cost (1.15ms at 50 tasks, 5.95ms at 200) was being paid
    // on every one of them for a value the wheel path never reads. Build it only for the
    // one event kind that actually consults it.
    let hits = if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        crate::ui::board::board_hit_map(area, model)
    } else {
        crate::ui::render::QueueHitMap::default()
    };
    let intent = crate::ui::mouse::map_board_mouse(model, &hits, mouse);
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && !matches!(intent, Some(BoardIntent::SelectSectionProject(_)))
    {
        // A double-click is two consecutive clicks on the same project header. Any other
        // pointer target, including inert board space, cancels the armed first click before
        // the next event can be mistaken for its second half.
        model.cancel_project_header_double_click();
    }
    let intent = intent?;
    let intent = board_intent_for_area(area, intent)?;
    resolve_board_command(model, intent)
}

/// Route a bracketed paste to the Capture intent the focused field accepts.
///
/// An unresolved save owns the form until Retry or Cancel, exactly as for a key press, so a
/// paste is inert there rather than editing a draft the form is not accepting.
fn capture_paste_intent(model: &CaptureModel, text: &str) -> Option<CaptureIntent> {
    if model.is_save_recovery() {
        return None;
    }
    map_capture_paste_state(model.focused(), model.is_path_editing(), text)
}

/// Apply one board intent and present any refusal instead of discarding it.
///
/// A refused intent changes nothing: an open edit keeps its mode, its draft, and its cursor,
/// because the domain rejected before [`crate::ui::board::apply_intent`] could clear them. All
/// this boundary adds is the reason, on the board's message line, so a field that will not
/// close says why. It owns no rule of its own: the domain decides what is
/// refused, and this only presents what came back.
fn apply_board_intent_presenting_rejection(
    domain: &mut DomainState,
    model: &mut BoardModel,
    recovery: &mut SaveRecovery<DomainState>,
    context: BoardSaveContext<'_>,
    persist: impl FnMut(&mut DomainState) -> Result<(), String>,
) -> IntentOutcome {
    match apply_board_intent_with_save_recovery(domain, model, recovery, context, persist) {
        Ok(outcome) => outcome,
        Err(error) => {
            // Safe to write here: every other producer on this line (park, dispatch, save
            // recovery) reports through `Ok`, so nothing that reaches this arm is competing
            // with a message about a different action. The line describes the intent the
            // user just issued, which is this one.
            model.set_message(board_rejection_message(&error));
            IntentOutcome::None
        }
    }
}

/// The line the board shows for a refused intent.
///
/// An empty title borrows Capture's phrasing so both surfaces say the same thing about the
/// same refusal. The two refusals that name a task by uuid get a short human phrase
/// instead: their `Display` spends more than a narrow board's whole message row on an id
/// the user cannot act on. Every other reason falls back to its own `Display`, so a refusal
/// this boundary has never seen is still explained rather than silently swallowed.
fn board_rejection_message(error: &DomainError) -> String {
    match error {
        DomainError::EmptyTitle => TITLE_REQUIRED_MESSAGE.to_string(),
        DomainError::UnknownId(_) => "that task is no longer here".to_string(),
        DomainError::SoftDeleted(_) => "that task is deleted".to_string(),
        other => other.to_string(),
    }
}

/// The intents whose **decision** depends on state another actor may have changed, and which
/// therefore must not be decided against a possibly stale in-memory snapshot.
///
/// Undo compares against the latest durable revision before mutating.
///
/// This is deliberately **narrower than "may persist"**: every intent in
/// [`board_intent_may_persist`] loads the durable baseline, because a save needs it, but only
/// these three are *decided* against it. Merging is not free and it re-derives the visible list,
/// so an intent that merely writes does not pay for one.
///
/// **`ConfirmEdit` is deliberately NOT here**, though resolved decision 8 requires it to judge
/// the bound task's availability against the durable record. Merging would satisfy the letter of
/// that and break something else: `merge_tasks_from_disk` replaces the local task wholesale, so
/// the subsequent `edit` records a merge base taken from the *disk* revision, `merge_for_save`
/// then sees a base that matches, and a concurrent same-task edit by another actor is silently
/// overwritten instead of raising a save conflict. That was measured, not assumed — see
/// [`confirm_edit_consults_the_record_without_merging_it`] and the conflict-detection test in
/// `tests/edit_target_binding.rs`. The availability judgment is made instead by
/// [`confirm_edit_refusal_against_the_record`], which *reads* the baseline and never merges it.
///
/// [`confirm_edit_consults_the_record_without_merging_it`]: self::tests
pub fn board_intent_needs_fresh_state(intent: &BoardIntent) -> bool {
    matches!(intent, BoardIntent::Undo)
}

/// resolved decision 8: a soft-deleted task is not editable, and a stale in-memory snapshot is
/// not an excuse for editing one.
///
/// Between an edit's open and its confirm, another actor can soft-delete the bound task and
/// persist it. The user can confirm first, and nothing else on this path would catch it in
/// time. So the confirm consults the freshly loaded durable record for this one judgment.
///
/// It **reads** the record and returns a verdict; it does not merge it. That distinction is the
/// whole design: merging would hand `merge_for_save` a merge base this board never earned and
/// silently defeat same-task conflict detection (see [`board_intent_needs_fresh_state`]).
/// Consulting leaves every local revision exactly where it was, so a concurrent edit still
/// collides at save time the way it always did.
///
/// Absence from the record is deliberately *not* refused here: nothing in the product hard-deletes
/// a task, so an id missing from disk means a store this board has state for and disk does not,
/// which the reducer's own unknown-id path already answers. Only the soft-deleted verdict,
/// the one a second writer can actually produce, is read from the record.
pub fn confirm_edit_refusal_against_the_record(
    record: &DomainState,
    model: &BoardModel,
) -> Option<DomainError> {
    let bound = model.edit_target()?;
    record
        .get(bound)
        .filter(|task| task.soft_deleted)
        .map(|_| DomainError::SoftDeleted(bound))
}

/// Merge the freshly loaded durable baseline into local state before a mutating intent is
/// decided, for the intents [`board_intent_needs_fresh_state`] names. Returns whether it merged.
///
/// Order is load-bearing and matches the board loop's: merge, re-derive presentation, *then* run
/// the reducer. The merge never touches an open edit session — `sync_from_domain` re-derives the
/// task list and the selection and leaves the input mode, the draft, the cursor, and the binding
/// alone — so a refusal that follows still finds the draft byte-identical, which is what makes
///'s "refuse visibly and keep the draft" survivable across a merge.
///
/// A caller holding an unresolved save must not call this: that state allows only navigation plus
/// Retry/Cancel, and re-merging underneath it would move the ground the retry stands on.
pub fn refresh_before_mutation(
    intent: &BoardIntent,
    baseline: &DomainState,
    domain: &mut DomainState,
    model: &mut BoardModel,
) -> bool {
    if !board_intent_needs_fresh_state(intent) {
        return false;
    }
    domain.merge_tasks_from_disk(baseline);
    model.sync_from_domain(domain);
    true
}

/// Apply a board intent. Returns `true` when the board loop should quit.
fn handle_board_intent(
    store: &TaskStore,
    domain: &mut DomainState,
    model: &mut BoardModel,
    intent: BoardIntent,
    pending_dispatch: &mut Option<PendingDispatch>,
    save_recovery: &mut SaveRecovery<DomainState>,
) -> io::Result<bool> {
    let herdr = HerdrHost::from_env();
    let host: &dyn HostPorts = &herdr;

    let baseline = if save_recovery.is_pending() || !board_intent_may_persist(&intent) {
        DomainState::new()
    } else {
        store
            .load()
            .map_err(|error| io::Error::other(error.to_string()))?
    };

    // Do not reload while a failed save is unresolved, because only navigation plus Retry/Cancel
    // are allowed there; otherwise, bring the durable record in before the intent is decided.
    if !save_recovery.is_pending() {
        refresh_before_mutation(&intent, &baseline, domain, model);
    }

    // OpenCapture needs the invocation snapshot `load_board` seeded the board
    // with (scope/capsule/provenance): the reducer stores it on `model.capture_snapshot` at
    // open and reads it back at ConfirmEdit, so a `None` here is what silently turned board
    // `a` into a no-op save that still reported success.
    let loaded_snapshot;
    let snapshot_for_intent = if !save_recovery.is_pending() && intent == BoardIntent::OpenCapture {
        loaded_snapshot = load_snapshot();
        Some(&loaded_snapshot)
    } else {
        None
    };

    match apply_board_intent_presenting_rejection(
        domain,
        model,
        save_recovery,
        BoardSaveContext {
            baseline,
            intent,
            snapshot: snapshot_for_intent,
            host: Some(host),
        },
        |state| {
            store
                .reload_merge_save(state)
                .map_err(|error| error.to_string())
        },
    ) {
        IntentOutcome::Quit => Ok(true),
        IntentOutcome::Persist | IntentOutcome::Persisted => Ok(false),
        IntentOutcome::ResumeDispatch { attempt_id } => {
            if pending_dispatch.is_some() {
                model.set_message("dispatch recovery already in progress…");
                return Ok(false);
            }
            let (tx, rx) = mpsc::channel();
            let worker_store = store.clone();
            let worker_host = HerdrHost::from_env();
            thread::spawn(move || {
                let result = resume_dispatch_attempt(worker_store, attempt_id, &worker_host);
                let _ = tx.send(result);
            });
            *pending_dispatch = Some(PendingDispatch { rx });
            Ok(false)
        }
        IntentOutcome::CleanupDispatch { attempt_id } => {
            if pending_dispatch.is_some() {
                model.set_message("dispatch recovery already in progress…");
                return Ok(false);
            }
            let (tx, rx) = mpsc::channel();
            let worker_store = store.clone();
            let worker_host = HerdrHost::from_env();
            thread::spawn(move || {
                let result = cleanup_dispatch_attempt(worker_store, attempt_id, &worker_host);
                let _ = tx.send(result);
            });
            *pending_dispatch = Some(PendingDispatch { rx });
            Ok(false)
        }
        IntentOutcome::None => Ok(false),
    }
}

/// Run the capture form until save/cancel.
///
/// Standalone capture mode: caller exits the process after this returns (popup closes).
/// Board-initiated: caller resumes the board loop (same process).
fn capture_form_loop(
    terminal: &mut DefaultTerminal,
    store: &TaskStore,
    domain: &mut DomainState,
    snapshot: &InvocationSnapshot,
    model: &mut CaptureModel,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw_capture(frame, model))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let Some(intent) = map_capture_key_state(
                    model.focused(),
                    model.is_path_editing(),
                    model.is_save_recovery(),
                    key,
                ) else {
                    continue;
                };
                match apply_capture_intent(domain, Some(store), snapshot, model, intent) {
                    Ok(CaptureOutcome::Saved(_)) | Ok(CaptureOutcome::Cancelled) => break,
                    Ok(CaptureOutcome::None) => {}
                    Err(e) => {
                        // Persist/domain failures after validation: abort form with error.
                        return Err(io::Error::other(e.to_string()));
                    }
                }
            }
            Event::Mouse(mouse) => {
                let area = terminal_area(terminal)?;
                // One Capture geometry for renderer and live dispatch: scope controls and the
                // disclosed path row must be clickable exactly where they are painted.
                let layout = capture_layout_for_model(area, model);
                let Some(intent) = map_capture_mouse(&layout, mouse) else {
                    continue;
                };
                match apply_capture_intent(domain, Some(store), snapshot, model, intent) {
                    Ok(CaptureOutcome::Saved(_)) | Ok(CaptureOutcome::Cancelled) => break,
                    Ok(CaptureOutcome::None) => {}
                    Err(e) => return Err(io::Error::other(e.to_string())),
                }
            }
            Event::Paste(text) => {
                let Some(intent) = capture_paste_intent(model, &text) else {
                    continue;
                };
                match apply_capture_intent(domain, Some(store), snapshot, model, intent) {
                    Ok(CaptureOutcome::Saved(_)) | Ok(CaptureOutcome::Cancelled) => break,
                    Ok(CaptureOutcome::None) => {}
                    Err(e) => return Err(io::Error::other(e.to_string())),
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}
#[cfg(test)]
mod save_recovery_tests {
    use std::sync::mpsc;

    use super::{
        board_background_work_allowed, process_pending_dispatch, PendingDispatch,
        PendingDispatchPoll,
    };
    use crate::dispatch::DispatchRecoveryResult;
    use crate::domain::DomainState;
    use crate::save_recovery::SaveRecovery;

    #[test]
    fn board_background_work_defers_finished_dispatch_during_save_recovery() {
        let (tx, rx) = mpsc::channel();
        tx.send(DispatchRecoveryResult::Error {
            message: "finished while saving".into(),
        })
        .expect("queue dispatch result");
        let mut pending = Some(PendingDispatch { rx });
        let mut recovery = SaveRecovery::new();
        recovery.fail(DomainState::new(), DomainState::new(), "save failed");

        assert!(matches!(
            process_pending_dispatch(&mut pending, &recovery),
            PendingDispatchPoll::Pending
        ));

        assert!(
            pending.is_some(),
            "finished dispatch stays queued until recovery resolves"
        );
        assert!(
            !board_background_work_allowed(&recovery),
            "background work must not run during save recovery"
        );

        let _ = recovery.cancel();
        assert!(matches!(
            process_pending_dispatch(&mut pending, &recovery),
            PendingDispatchPoll::Finished(DispatchRecoveryResult::Error { .. })
        ));

        assert!(pending.is_none());
        assert!(board_background_work_allowed(&recovery));
    }

    /// A worker that dies without sending must surface as `Disconnected` exactly once,
    /// so the board can clear its pending state instead of polling a dead channel.
    #[test]
    fn dropped_dispatch_worker_reports_disconnected_and_does_not_requeue() {
        let (tx, rx) = mpsc::channel::<DispatchRecoveryResult>();
        drop(tx);
        let mut pending = Some(PendingDispatch { rx });
        let recovery = SaveRecovery::<DomainState>::new();

        assert!(
            matches!(
                process_pending_dispatch(&mut pending, &recovery),
                PendingDispatchPoll::Disconnected
            ),
            "a sender dropped without a result must report Disconnected"
        );
        assert!(
            pending.is_none(),
            "a disconnected worker must not be re-queued for another poll"
        );
        assert!(
            matches!(
                process_pending_dispatch(&mut pending, &recovery),
                PendingDispatchPoll::Pending
            ),
            "with the slot cleared the next poll is idle, not Disconnected again"
        );
    }
}

/// the idle frame tick's store-only revalidation.
#[cfg(test)]
mod idle_store_revalidation_tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{revalidate_board_from_store, StoreWatch};
    use crate::domain::{DomainState, ProvenanceOrigin, TaskScope};
    use crate::save_recovery::SaveRecovery;
    use crate::store::TaskStore;
    use crate::ui::board::{apply_intent, BoardInputMode, BoardModel};
    use crate::ui::input::BoardIntent;

    const THIS_REPO: &str = "/repos/app";

    fn temp_store_dir(label: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tsk-idle-revalidate-{label}-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn project_scope() -> TaskScope {
        TaskScope::Project {
            path: THIS_REPO.to_string(),
        }
    }

    /// an open, idle board picks up a task a *separate* store writer (the standalone
    /// quick-capture popup, per the manifest) saved to the same `tasks.json`, with no
    /// persisting intent run on this board at all.
    #[test]
    fn idle_tick_revalidates_task_written_by_a_separate_store_writer() {
        let dir = temp_store_dir("quick-capture");
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).unwrap();

        let mut domain = store.load().unwrap();
        let mut model = BoardModel::from_domain(&domain, Some(std::path::PathBuf::from(THIS_REPO)));
        let mut watch = StoreWatch::seeded(&store);
        let save_recovery = SaveRecovery::<DomainState>::new();

        // A separate writer (its own TaskStore handle, standing in for the quick-capture
        // popup process) saves a new task to the same on-disk document.
        let writer_store = TaskStore::new(&dir);
        let mut writer_domain = writer_store.load().unwrap();
        let captured_id = writer_domain
            .create(
                "Quick capture from another pane",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        writer_store.save(&writer_domain).unwrap();

        assert!(
            !model.visible_ids().contains(&captured_id),
            "not visible before the idle tick revalidates"
        );

        let merged = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );

        assert!(merged, "idle tick must detect the changed store signature");
        assert!(
            domain.get(captured_id).is_some(),
            "domain must merge the separate writer's task"
        );
        assert!(
            model.visible_ids().contains(&captured_id),
            "model must sync so the quick-capture task renders without a persisting intent"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// The cheap path stays cheap: a second idle tick over an unchanged store must not merge
    /// or sync again.
    #[test]
    fn idle_tick_skips_revalidation_when_store_is_unchanged() {
        let dir = temp_store_dir("unchanged");
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).unwrap();

        let mut domain = store.load().unwrap();
        let mut model = BoardModel::from_domain(&domain, None);
        let mut watch = StoreWatch::seeded(&store);
        let save_recovery = SaveRecovery::<DomainState>::new();

        let first = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );
        assert!(
            !first,
            "a watch seeded from the just-loaded snapshot must see no change"
        );

        let second = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );
        assert!(
            !second,
            "repeated idle ticks over unchanged disk stay no-ops"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// / save recovery: a failed save owns the displayed working state, so the idle tick
    /// must not reload disk out from under it even though the store changed.
    #[test]
    fn idle_tick_skips_revalidation_while_save_recovery_is_pending() {
        let dir = temp_store_dir("save-recovery");
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).unwrap();

        let mut domain = store.load().unwrap();
        let mut model = BoardModel::from_domain(&domain, None);
        let mut watch = StoreWatch::seeded(&store);

        let writer_store = TaskStore::new(&dir);
        let mut writer_domain = writer_store.load().unwrap();
        let captured_id = writer_domain
            .create(
                "Quick capture during save recovery",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        writer_store.save(&writer_domain).unwrap();

        let mut recovery = SaveRecovery::<DomainState>::new();
        recovery.fail(DomainState::new(), DomainState::new(), "save failed");

        let merged =
            revalidate_board_from_store(&store, &mut domain, &mut model, &mut watch, &recovery);

        assert!(
            !merged,
            "must not reload disk while a failed save owns the working state"
        );
        assert!(domain.get(captured_id).is_none());
        assert!(!model.visible_ids().contains(&captured_id));

        let _ = fs::remove_dir_all(&dir);
    }

    /// a failed `store.load()` must not burn the changed signature it never got to use --
    /// otherwise a single transient read failure leaves the board stale until some *later*
    /// write happens to produce yet another distinct signature, which is the exact failure
    /// this watch exists to survive. Corrupts `tasks.json` in place (a stand-in for any
    /// transient read failure) so `store.load()` errors while the on-disk signature has
    /// already changed, then proves the watch still reports that same signature as changed on
    /// the next poll (it was never recorded), and that a later, valid write is picked up.
    #[test]
    fn idle_tick_retries_after_a_failed_load_instead_of_recording_the_burned_signature() {
        let dir = temp_store_dir("failed-load");
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).unwrap();

        let mut domain = store.load().unwrap();
        let mut model = BoardModel::from_domain(&domain, Some(std::path::PathBuf::from(THIS_REPO)));
        let mut watch = StoreWatch::seeded(&store);
        let save_recovery = SaveRecovery::<DomainState>::new();

        // Corrupt the document in place: the signature changes (different length), but
        // `store.load()` fails to parse it -- standing in for any transient read failure on
        // an already-changed document.
        let state_file = dir.join("tasks.json");
        fs::write(&state_file, b"not valid json").unwrap();

        let merged = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );
        assert!(
            !merged,
            "a load that fails to parse must not report a merge"
        );

        // The failed load must not have recorded the corrupt signature: the watch still
        // reports the document as changed relative to what it saw at `seeded()`, so the very
        // next tick keeps trying instead of treating the failed read as caught up.
        assert!(
            watch.poll(&store).is_some(),
            "a failed load must not burn the signature it never merged"
        );

        // Repair the document with a valid write; the watch (never having recorded the
        // corrupt signature) must still pick it up.
        let mut repaired = DomainState::new();
        let captured_id = repaired
            .create(
                "Recovered after a transient load failure",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        store.save(&repaired).unwrap();

        let merged = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );
        assert!(merged, "the repaired document must merge on the next tick");
        assert!(domain.get(captured_id).is_some());
        assert!(model.visible_ids().contains(&captured_id));

        let _ = fs::remove_dir_all(&dir);
    }

    /// M-3(i) /: an open **title** edit must not be redirected by the idle merge this
    /// call site drives -- the doc above claims it (`sync_from_domain` reanchors selection by
    /// id and never touches the edit binding), and `tests/edit_target_binding.rs` already
    /// pins the identical merge+reanchor pair through `run_attention_cycle`, but nothing drove
    /// it through `revalidate_board_from_store` itself. Pin it here so this call site's own
    /// risk is bound to a test, not only to the property it borrows.
    #[test]
    fn idle_tick_does_not_redirect_an_open_title_edit_and_still_merges_a_separate_writer() {
        let dir = temp_store_dir("open-title-edit");
        let store = TaskStore::new(&dir);

        let mut domain = DomainState::new();
        let alpha = domain
            .create(
                "Alpha",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .unwrap();
        store.save(&domain).unwrap();

        let mut model = BoardModel::from_domain(&domain, Some(std::path::PathBuf::from(THIS_REPO)));
        let mut watch = StoreWatch::seeded(&store);
        let save_recovery = SaveRecovery::<DomainState>::new();

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::BeginEditTitle,
            None,
            None,
        )
        .expect("begin title edit");
        assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
        assert_eq!(model.edit_target(), Some(alpha));

        // A separate writer saves a new task while the edit sits open, the same shape as the
        // idle loop's real poll.
        let writer_store = TaskStore::new(&dir);
        let mut writer_domain = writer_store.load().unwrap();
        let captured_id = writer_domain
            .create(
                "Quick capture while a title edit is open",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        writer_store.save(&writer_domain).unwrap();

        let merged = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );
        assert!(
            merged,
            "the idle tick must still merge the separate writer's task"
        );

        assert_eq!(
            model.input_mode(),
            BoardInputMode::EditTitle,
            "AC-24: an open edit session must not be redirected by an idle merge"
        );
        assert_eq!(
            model.edit_target(),
            Some(alpha),
            "the edit must stay bound to the same task the merge ran under"
        );
        assert_eq!(
            model.edit_buffer(),
            "Alpha",
            "the open draft must survive the merge untouched"
        );
        assert!(domain.get(captured_id).is_some());
        assert!(
            model.visible_ids().contains(&captured_id),
            "the merged task still becomes visible around the open edit"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// M-3(i) /: an open **capture draft** must not be redirected by the idle merge
    /// either -- capture is not a task edit at all (there is no `edit_target` to reanchor),
    /// so the only risk is `sync_from_domain` clobbering the draft's own fields or knocking the
    /// board out of `Capture` mode; this pins that it does neither.
    #[test]
    fn idle_tick_does_not_redirect_an_open_capture_draft_and_still_merges_a_separate_writer() {
        let dir = temp_store_dir("open-capture-draft");
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).unwrap();

        let mut domain = store.load().unwrap();
        let mut model = BoardModel::from_domain(&domain, Some(std::path::PathBuf::from(THIS_REPO)));
        let mut watch = StoreWatch::seeded(&store);
        let save_recovery = SaveRecovery::<DomainState>::new();

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            None,
            None,
        )
        .expect("open quick add");
        assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
        for character in "Draft in progress".chars() {
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::QuickAddInsert(character),
                None,
                None,
            )
            .expect("type into the capture draft");
        }
        assert_eq!(model.quick_add_title_value(), "Draft in progress");

        let writer_store = TaskStore::new(&dir);
        let mut writer_domain = writer_store.load().unwrap();
        let captured_id = writer_domain
            .create(
                "Quick capture while a draft is open",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        writer_store.save(&writer_domain).unwrap();

        let merged = revalidate_board_from_store(
            &store,
            &mut domain,
            &mut model,
            &mut watch,
            &save_recovery,
        );
        assert!(
            merged,
            "the idle tick must still merge the separate writer's task"
        );

        assert_eq!(
            model.input_mode(),
            BoardInputMode::QuickAdd,
            "AC-24: an open quick-add draft must not be redirected by an idle merge"
        );
        assert_eq!(
            model.quick_add_title_value(),
            "Draft in progress",
            "the open draft must survive the merge untouched"
        );
        assert!(domain.get(captured_id).is_some());
        assert!(
            model.visible_ids().contains(&captured_id),
            "the merged task still becomes visible around the open draft"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// M-3(ii): drives [`super::board_idle_tick`] itself -- the one function `run_board`'s
    /// loop calls to decide whether a tick was idle, with no separate call in the loop for a
    /// later edit to drop without a test noticing. The prior version of this test called
    /// `board_frame` and `revalidate_board_from_store` as two separate steps in the test body,
    /// which could not tell a `run_board` that had stopped wiring them together apart from one
    /// that still did (deleting the merge call from the loop kept it green). Driving
    /// `board_idle_tick` instead pins the actual call `run_board` makes: the merge below is
    /// not something this test additionally triggers, it is a postcondition of the single call
    /// under test reporting `FramePoll::Idle` at all.
    #[test]
    fn board_idle_tick_reports_idle_and_merges_a_separate_writers_task_in_one_call() {
        use super::{board_idle_tick, FramePoll};
        use crate::store::StoreError;

        let dir = temp_store_dir("real-idle-path");
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).unwrap();

        let mut domain = store.load().unwrap();
        let mut model = BoardModel::from_domain(&domain, Some(std::path::PathBuf::from(THIS_REPO)));
        let mut watch = StoreWatch::seeded(&store);
        let save_recovery = SaveRecovery::<DomainState>::new();

        let writer_store = TaskStore::new(&dir);
        let mut writer_domain = writer_store.load().unwrap();
        let captured_id = writer_domain
            .create(
                "Quick capture, real idle path",
                None,
                project_scope(),
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        writer_store.save(&writer_domain).unwrap();

        let write = || -> Result<(), StoreError> { Ok(()) };
        let paint = |_model: &BoardModel| -> std::io::Result<()> { Ok(()) };
        // Never sees an event, matching the real Idle branch.
        let wait = |_duration: std::time::Duration| -> std::io::Result<bool> { Ok(false) };
        let poll = board_idle_tick(
            &mut model,
            write,
            paint,
            wait,
            &store,
            &mut domain,
            &mut watch,
            &save_recovery,
        )
        .unwrap();
        assert_eq!(poll, FramePoll::Idle);

        assert!(
            domain.get(captured_id).is_some(),
            "reporting Idle must already have merged the separate writer's task, with no \
             further call needed"
        );
        assert!(model.visible_ids().contains(&captured_id));

        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::context::InvocationSnapshot;
    use crate::domain::{AgentMeta, HumanStatus, ObservedStatus, ProvenanceOrigin, TaskScope};
    use crate::host::PaneInfo;
    use crate::ui::board::CommandSurface;
    use crate::ui::capture::CaptureField;
    use crate::ui::input::map_key;
    use crate::ui::mouse::map_board_mouse;

    static TEMP_DIR_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    #[test]
    fn default_mode_is_board() {
        assert_eq!(resolve_mode_from(None, ["tsk"]), AppMode::Board);
        assert_eq!(
            resolve_mode_from(None, ["tsk", "--something"]),
            AppMode::Board
        );
        // Non-capture env values do not select Capture.
        assert_eq!(resolve_mode_from(Some("board"), ["tsk"]), AppMode::Board);
    }

    #[test]
    fn capture_arg_selects_capture_mode() {
        assert_eq!(
            resolve_mode_from(None, ["tsk", "capture"]),
            AppMode::Capture
        );
        // Arg still wins when env is absent or non-capture.
        assert_eq!(
            resolve_mode_from(Some("board"), ["tsk", "capture"]),
            AppMode::Capture
        );
    }

    #[test]
    fn capture_env_selects_capture_mode() {
        assert_eq!(
            resolve_mode_from(Some("capture"), ["tsk"]),
            AppMode::Capture
        );
        assert_eq!(
            resolve_mode_from(Some("CAPTURE"), ["tsk", "--something"]),
            AppMode::Capture
        );
    }

    /// requires every listed control to remain operable down to 40x10, so a
    /// mutating control must reach its intent through the same route `run_board` drives
    /// (surface resolution -> key map -> area gate -> command-surface resolution) at the
    /// legacy Resize band and below, not just at Wide/Browse sizes, and it must actually be
    /// applied: routing to an intent that is never applied proves nothing moved. Formerly
    /// `resize_guidance_blocks_keyboard_task_mutation_but_keeps_quit_reachable`, which
    /// asserted the opposite (now-removed) gate.
    #[test]
    fn compact_controls_operate_down_to_40x10_and_quit_stays_reachable() {
        /// Drive one key through the live route at `area` -- surface resolution, key map,
        /// area gate, command-surface resolution -- and apply the resolved intent, exactly
        /// as `run_board`'s key arm does.
        fn drive(
            domain: &mut DomainState,
            model: &mut BoardModel,
            area: Rect,
            key: KeyEvent,
        ) -> IntentOutcome {
            let mode = resolve_board_surface(area, model);
            let intent = map_key(mode, key)
                .unwrap_or_else(|| panic!("{area:?}: {key:?} must still map to an intent"));
            let intent = board_intent_for_area(area, intent)
                .unwrap_or_else(|| panic!("{area:?}: {key:?}'s intent must route"));
            let intent = resolve_board_command(model, intent).unwrap_or_else(|| {
                panic!("{area:?}: {key:?}'s intent must resolve through the command route")
            });
            apply_intent(domain, model, intent, None, None)
                .unwrap_or_else(|e| panic!("{area:?}: {key:?} must apply cleanly: {e:?}"))
        }
        let alt = |code| KeyEvent::new(code, KeyModifiers::ALT);
        let bare = |code| KeyEvent::new(code, KeyModifiers::NONE);

        for area in [
            Rect::new(0, 0, 49, 18),
            Rect::new(0, 0, 50, 17),
            Rect::new(0, 0, 40, 10),
        ] {
            // 'd' (Complete): Todo -> Done, actually applied.
            let mut domain = DomainState::new();
            let id = domain
                .create(
                    "Stay todo",
                    None,
                    TaskScope::Global,
                    None,
                    None,
                    ProvenanceOrigin::Capture,
                )
                .expect("create task");
            let mut model = BoardModel::from_domain(&domain, None);
            drive(&mut domain, &mut model, area, alt(KeyCode::Char('d')));
            assert_eq!(
                domain.get(id).expect("task").status,
                HumanStatus::Done,
                "{area:?}: 'd' must actually complete the task through the live route"
            );

            // space (PrimaryVerb): Todo -> Doing, actually applied.
            let mut domain = DomainState::new();
            let id = domain
                .create(
                    "Stay todo",
                    None,
                    TaskScope::Global,
                    None,
                    None,
                    ProvenanceOrigin::Capture,
                )
                .expect("create task");
            let mut model = BoardModel::from_domain(&domain, None);
            drive(&mut domain, &mut model, area, alt(KeyCode::Char(' ')));
            assert_eq!(
                domain.get(id).expect("task").status,
                HumanStatus::Started,
                "{area:?}: space must actually start the task through the live route"
            );

            // x (SoftDelete): actually applied.
            let mut domain = DomainState::new();
            let id = domain
                .create(
                    "Stay todo",
                    None,
                    TaskScope::Global,
                    None,
                    None,
                    ProvenanceOrigin::Capture,
                )
                .expect("create task");
            let mut model = BoardModel::from_domain(&domain, None);
            drive(&mut domain, &mut model, area, alt(KeyCode::Char('x')));
            assert!(
                domain.get(id).expect("task").soft_deleted,
                "{area:?}: 'x' must actually soft-delete the task through the live route"
            );

            // ':' (OpenCommandPalette): actually applied (opens the palette; not a mutation).
            let mut domain = DomainState::new();
            domain
                .create(
                    "Stay todo",
                    None,
                    TaskScope::Global,
                    None,
                    None,
                    ProvenanceOrigin::Capture,
                )
                .expect("create task");
            let mut model = BoardModel::from_domain(&domain, None);
            drive(&mut domain, &mut model, area, bare(KeyCode::Char(':')));
            assert_eq!(
                model.command_surface(),
                CommandSurface::Palette,
                "{area:?}: ':' must actually open the command palette through the live route"
            );

            // Quit remains reachable.
            assert_eq!(
                board_intent_for_area(area, BoardIntent::Quit),
                Some(BoardIntent::Quit),
                "{area:?}: quit remains reachable"
            );
        }
    }

    /// Minor 3: `board_mouse_intent` now builds `board_hit_map` only
    /// for `Down(Left)`, never for a wheel step, since `map_board_mouse` resolves
    /// `ScrollUp`/`ScrollDown` through `wheel_board_intent` without ever reading the
    /// hit-map parameter. This proves the fast path still dispatches the same intent the
    /// keyboard's own selection step does, end to end through `board_mouse_intent` itself
    /// -- not just through `map_board_mouse` (already covered at that layer by
    /// `wheel_step_matches_the_keyboard_selection_step` in `ui::mouse::tests`).
    #[test]
    fn board_command_surface_mouse_and_keyboard_dispatch_the_same_intent() {
        use crate::ui::board::{apply_intent, board_hit_map, resolve_board_command};
        use crate::ui::mouse::left_click;
        use crate::ui::render::QueueHitTarget;

        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Command me",
                None,
                TaskScope::Project {
                    path: "/repos/app".into(),
                },
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
        assert_eq!(model.selected_id(), Some(id));
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCommandPalette,
            None,
            None,
        )
        .expect("open palette");
        // Narrow to "delete" (the catalog; reopen needs a done selection and Complete is
        // key-only, so this todo fixture's unambiguous, always-available entry is delete).
        for character in "delete".chars() {
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::CommandQueryInsert(character),
                None,
                None,
            )
            .expect("type query");
        }

        // Live mouse dispatch consumes the same resolved hit-map the renderer uses.
        let area = Rect::new(0, 0, 120, 24);
        let hits = board_hit_map(area, &model);
        assert_eq!(
            model.visible_commands().first().map(|c| c.intent.clone()),
            Some(BoardIntent::SoftDelete)
        );
        let chip = hits
            .regions
            .iter()
            .find(|hit| matches!(hit.target, QueueHitTarget::Command(0)))
            .expect("command hit region");
        let mouse_intent = map_board_mouse(&model, &hits, left_click(chip.area.x + 1, chip.area.y))
            .expect("mouse command intent");

        // the legacy Resize band no longer withholds the command route either; the
        // compact queue frame paints the command surface there, so build the hit map
        // there too and resolve the same click through it -- not just an identity check on
        // the intent already resolved at 120x24 -- before anything closes the surface below.
        let resize = Rect::new(0, 0, 49, 18);
        let resize_hits = board_hit_map(resize, &model);
        let resize_chip = resize_hits
            .regions
            .iter()
            .find(|hit| matches!(hit.target, QueueHitTarget::Command(0)))
            .expect("command hit region at the resize band");
        let resize_mouse_intent = map_board_mouse(
            &model,
            &resize_hits,
            left_click(resize_chip.area.x + 1, resize_chip.area.y),
        )
        .expect("mouse command intent at the resize band");
        assert_eq!(
            resize_mouse_intent, mouse_intent,
            "the resize band must resolve the same command click as 120x24"
        );

        assert_eq!(
            board_intent_for_area(area, mouse_intent.clone()),
            Some(mouse_intent.clone())
        );
        assert_eq!(
            board_intent_for_area(resize, resize_mouse_intent.clone()),
            Some(resize_mouse_intent.clone())
        );

        // a command row click is `SelectCommand(index)`, not the row's own intent
        // directly, so it is not raw-equal to `ConfirmCommand` the way earlier commands here
        // were before that fix (the same reason `SelectIndex`/`SelectProjectOption` are never
        // raw-equal to their keyboard counterparts either). What must still match is what both
        // resolve to and the surface teardown resolving does -- resolve each on its own model
        // clone so resolving one cannot affect the other's outcome.
        let mut mouse_resolved_model = model.clone();
        let resolved_mouse_intent =
            resolve_board_command(&mut mouse_resolved_model, mouse_intent.clone())
                .expect("mouse command resolves to a dispatchable intent");
        let keyboard_intent = resolve_board_command(&mut model, BoardIntent::ConfirmCommand)
            .expect("keyboard command intent");
        assert_eq!(
            resolved_mouse_intent, keyboard_intent,
            "click and Enter must resolve to the same underlying command"
        );
        assert_eq!(
            mouse_resolved_model.command_surface(),
            model.command_surface(),
            "resolving the click must tear the surface down the same way Enter's \
             ConfirmCommand does"
        );
        assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);
    }

    #[test]
    fn intervening_task_click_cancels_an_armed_project_header_double_click() {
        use crate::ui::board::{apply_intent, board_hit_map};
        use crate::ui::mouse::left_click;
        use crate::ui::render::QueueHitTarget;

        let mut domain = DomainState::new();
        let id = domain
            .create(
                "click between headers",
                None,
                TaskScope::Project {
                    path: "/repos/app".into(),
                },
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
        let area = Rect::new(0, 0, 80, 24);

        fn target_mouse(
            area: Rect,
            model: &BoardModel,
            target: impl Fn(QueueHitTarget) -> bool,
        ) -> crossterm::event::MouseEvent {
            let hits = board_hit_map(area, model);
            let hit = hits
                .regions
                .iter()
                .find(|hit| target(hit.target))
                .unwrap_or_else(|| panic!("missing target in {hits:?}"));
            left_click(hit.area.x, hit.area.y)
        }
        let drive_click = |domain: &mut DomainState,
                           model: &mut BoardModel,
                           mouse: crossterm::event::MouseEvent| {
            let intent = board_mouse_intent(area, model, mouse).expect("click maps to intent");
            apply_intent(domain, model, intent, None, None).expect("click applies");
        };

        let mouse = target_mouse(area, &model, |target| {
            matches!(target, QueueHitTarget::SectionProject(_))
        });
        drive_click(&mut domain, &mut model, mouse);
        assert_eq!(
            model.selected_project(),
            None,
            "first header click only arms"
        );

        let mouse = target_mouse(
            area,
            &model,
            |target| matches!(target, QueueHitTarget::Task(task_id) if task_id == id),
        );
        drive_click(&mut domain, &mut model, mouse);
        let mouse = target_mouse(area, &model, |target| {
            matches!(target, QueueHitTarget::SectionProject(_))
        });
        drive_click(&mut domain, &mut model, mouse);
        assert_eq!(
            model.selected_project(),
            None,
            "header click after an intervening task click must be a new first click"
        );

        let mouse = target_mouse(area, &model, |target| {
            matches!(target, QueueHitTarget::SectionProject(_))
        });
        drive_click(&mut domain, &mut model, mouse);
        assert_eq!(
            model.selected_project(),
            Some(Path::new("/repos/app")),
            "only two consecutive header clicks scope the board"
        );
    }

    /// live dispatch reaches the project selector by mouse and keyboard in
    /// every layout that offers it, including the legacy Resize band down to 40x10.
    #[test]
    fn project_selector_mouse_and_keyboard_dispatch_the_same_intent_at_every_size() {
        use crate::ui::board::board_hit_map;
        use crate::ui::mouse::left_click;
        use crate::ui::render::QueueHitTarget;

        let mut domain = DomainState::new();
        domain
            .create(
                "Project scoped",
                None,
                TaskScope::Project {
                    path: "/repos/app".into(),
                },
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");
        let model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));

        for area in [
            Rect::new(0, 0, 120, 24),
            Rect::new(0, 0, 50, 18),
            Rect::new(0, 0, 49, 18),
            Rect::new(0, 0, 40, 10),
        ] {
            let hits = board_hit_map(area, &model);
            let chip = hits
                .regions
                .iter()
                .find(|hit| matches!(hit.target, QueueHitTarget::ProjectChip))
                .unwrap_or_else(|| panic!("no project chip hit region at {area:?}"));
            let mouse_intent =
                map_board_mouse(&model, &hits, left_click(chip.area.x + 1, chip.area.y))
                    .expect("project chip hit");
            assert_eq!(mouse_intent, BoardIntent::OpenProjectSelector);
            // `P` gives the keyboard the identical route to the same intent.
            let keyboard_intent = map_key(
                board_input_mode_for_area(area, model.input_mode()),
                KeyEvent::new(KeyCode::Char('P'), KeyModifiers::NONE),
            );
            assert_eq!(
                keyboard_intent,
                Some(mouse_intent.clone()),
                "`P` must dispatch the same intent as the project chip click at {area:?}"
            );
            assert_eq!(
                board_intent_for_area(area, mouse_intent.clone()),
                Some(mouse_intent.clone()),
                "{area:?} must route the project selector"
            );
        }

        // the legacy Resize band paints a project chip too and no longer
        // withholds the route to it.
        assert_eq!(
            board_intent_for_area(RESIZE_AREA, BoardIntent::OpenProjectSelector),
            Some(BoardIntent::OpenProjectSelector),
            "the project selector must remain reachable down to the legacy Resize band"
        );
    }

    #[test]
    fn every_board_mutation_uses_a_real_persisted_baseline() {
        for intent in [
            BoardIntent::ConfirmEdit,
            BoardIntent::ConfirmEditNext,
            BoardIntent::SetStatus(HumanStatus::Blocked),
            BoardIntent::Complete,
            BoardIntent::Reopen,
            BoardIntent::SoftDelete,
            BoardIntent::Undo,
            BoardIntent::PrimaryVerb,
            BoardIntent::ToggleBlock,
        ] {
            assert!(
                board_intent_may_persist(&intent),
                "{intent:?} must load the persisted baseline before save recovery"
            );
        }
        assert!(!board_intent_may_persist(&BoardIntent::SelectNext));

        // Editing moves a draft, never the store: only ConfirmEdit above writes.
        for intent in [
            BoardIntent::EditInsert('x'),
            BoardIntent::EditInsertText("x".to_string()),
            BoardIntent::EditInsertLineBreak,
            BoardIntent::EditBackspace,
            BoardIntent::EditDeleteForward,
            BoardIntent::EditMoveLeft,
            BoardIntent::EditMoveRight,
            BoardIntent::EditMoveLineStart,
            BoardIntent::EditMoveLineEnd,
            BoardIntent::EditMoveWordLeft,
            BoardIntent::EditMoveWordRight,
            BoardIntent::CancelEdit,
        ] {
            assert!(
                !board_intent_may_persist(&intent),
                "{intent:?} must not reload a persisted baseline"
            );
        }
    }

    /// Fresh on-disk store for one test, cleaned up on drop.
    struct TempStore {
        dir: PathBuf,
        store: TaskStore,
    }

    impl TempStore {
        fn new(label: &str) -> Self {
            use std::time::{SystemTime, UNIX_EPOCH};
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = env::temp_dir().join(format!("tsk-{label}-{nanos}-{seq}"));
            std::fs::create_dir_all(&dir).unwrap();
            let store = TaskStore::new(&dir);
            TempStore { dir, store }
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn ctrl_enter_step_save_uses_the_real_app_save_boundary() {
        let temp = TempStore::new("ctrl-enter-step");
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Ctrl Enter",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("seed task");
        temp.store.save(&domain).expect("seed store");
        let mut model = BoardModel::from_domain(&domain, None);
        let mut pending_dispatch = None;
        let mut recovery = SaveRecovery::new();
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenTaskPage,
            None,
            None,
        )
        .expect("open");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::BeginAddStep,
            None,
            None,
        )
        .expect("edit");
        for ch in "next step".chars() {
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::EditInsert(ch),
                None,
                None,
            )
            .expect("type");
        }
        handle_board_intent(
            &temp.store,
            &mut domain,
            &mut model,
            BoardIntent::ConfirmEditNext,
            &mut pending_dispatch,
            &mut recovery,
        )
        .expect("real ctrl-enter save");
        assert_eq!(
            temp.store
                .load()
                .expect("reload")
                .get(id)
                .expect("task")
                .steps[0]
                .text,
            "next step"
        );
        assert!(!recovery.is_pending());
        assert_eq!(
            model.input_mode(),
            BoardInputMode::EditStep,
            "successful Ctrl+Enter reopens add editor"
        );
    }

    /// fix2 B1: `a` on the real board must create a task through the exact intent
    /// route the running app takes — `handle_board_intent`, which decides the snapshot the
    /// same way the live loop does — not through `apply_intent` hand-fed a snapshot the
    /// product never supplies. This is the app-loop boundary test the second review asked
    /// for: no snapshot is constructed or injected here, only real board intents.
    #[test]
    fn board_plus_title_enter_creates_one_task_through_the_real_app_intent_route() {
        let temp = TempStore::new("board-a-real-route");
        let mut domain = DomainState::new();
        temp.store.save(&domain).expect("seed empty store");
        let mut model = BoardModel::from_domain(&domain, None);
        let mut pending_dispatch = None;
        let mut save_recovery = SaveRecovery::new();

        assert_eq!(model.input_mode(), BoardInputMode::Normal);

        // `a`: OpenCapture, exactly as NORMAL_KEYMAP binds it.
        let quit = handle_board_intent(
            &temp.store,
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            &mut pending_dispatch,
            &mut save_recovery,
        )
        .expect("open capture");
        assert!(!quit);
        assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);

        // Type a title one key at a time, the way the real keyboard loop feeds it in.
        for ch in "Real app-route capture".chars() {
            handle_board_intent(
                &temp.store,
                &mut domain,
                &mut model,
                BoardIntent::QuickAddInsert(ch),
                &mut pending_dispatch,
                &mut save_recovery,
            )
            .expect("type title");
        }

        // Enter saves and closes the status-row line.
        let quit = handle_board_intent(
            &temp.store,
            &mut domain,
            &mut model,
            BoardIntent::QuickAddSave,
            &mut pending_dispatch,
            &mut save_recovery,
        )
        .expect("save quick add");
        assert!(!quit);
        assert_eq!(
            model.input_mode(),
            BoardInputMode::Normal,
            "a successful capture returns to Normal"
        );
        assert!(
            !save_recovery.is_pending(),
            "a real, writable temp store must not enter save recovery"
        );

        let saved = temp.store.load().expect("load persisted domain");
        let created: Vec<_> = saved
            .tasks()
            .iter()
            .filter(|t| t.title == "Real app-route capture")
            .collect();
        assert_eq!(
            created.len(),
            1,
            "board `+` must create exactly one task through the real app intent path, not zero"
        );
        assert!(
            model.visible_ids().contains(&created[0].id),
            "board model must show the task it just created"
        );
    }

    #[test]
    fn quick_add_project_token_matches_a_project_basename_case_insensitively() {
        let snapshot = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: Some(PathBuf::from("/repos/tsk-board")),
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, snapshot.this_repo.clone());

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            Some(&snapshot),
            None,
        )
        .expect("open quick add");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::QuickAddInsertText("Case insensitive scope !p TSK-Board".into()),
            Some(&snapshot),
            None,
        )
        .expect("type title and scope token");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::QuickAddSave,
            Some(&snapshot),
            None,
        )
        .expect("save quick add");

        let task = domain.tasks().first().expect("quick add creates a task");
        assert_eq!(task.title, "Case insensitive scope");
        assert_eq!(
            task.scope,
            TaskScope::Project {
                path: "/repos/tsk-board".into()
            }
        );
    }

    #[test]
    fn quick_add_refusals_keep_the_line_open_and_esc_drops_their_message() {
        let snapshot = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: None,
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, None);

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            Some(&snapshot),
            None,
        )
        .expect("open quick add");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::QuickAddSave,
            Some(&snapshot),
            None,
        )
        .expect("reject empty title");
        assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
        assert_eq!(model.message(), Some(TITLE_REQUIRED_MESSAGE));

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::CancelQuickAdd,
            Some(&snapshot),
            None,
        )
        .expect("close refused quick add");
        assert_eq!(model.input_mode(), BoardInputMode::Normal);
        assert_eq!(model.message(), None, "no refusal leaks onto the board");

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            None,
            None,
        )
        .expect("open snapshot-less quick add");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::QuickAddSave,
            None,
            None,
        )
        .expect("refuse unavailable capture context");
        assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
        assert_eq!(
            model.message(),
            Some("capture context unavailable; press Esc and try again")
        );
    }

    #[test]
    fn expanded_quick_add_save_recovery_cancel_keeps_the_complete_draft_stash() {
        use std::fs;

        let dir = temp_state_dir("expanded-quick-add-save-recovery");
        let blocked = dir.join("not-a-directory");
        fs::write(&blocked, "not a state directory").expect("blocking file");
        let store = TaskStore::new(&blocked);
        let snapshot = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: Some(PathBuf::from("/repos/chosen")),
            title_prefill: Some("Retained title".into()),
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, snapshot.this_repo.clone());
        let mut recovery = SaveRecovery::new();

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            Some(&snapshot),
            None,
        )
        .expect("open quick add");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ExpandQuickAdd,
            Some(&snapshot),
            None,
        )
        .expect("expand quick add");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsertText("retained notes".into()),
            Some(&snapshot),
            None,
        )
        .expect("write notes");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FocusFormField(CaptureField::Scope),
            Some(&snapshot),
            None,
        )
        .expect("focus scope");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormCycleScope,
            Some(&snapshot),
            None,
        )
        .expect("choose project scope");
        assert_eq!(
            model.form_scope(),
            Some(&TaskScope::Project {
                path: "/repos/chosen".into()
            })
        );

        let outcome = apply_board_intent_with_save_recovery(
            &mut domain,
            &mut model,
            &mut recovery,
            BoardSaveContext {
                baseline: DomainState::new(),
                intent: BoardIntent::ConfirmEdit,
                snapshot: Some(&snapshot),
                host: None,
            },
            |working| store.save(working).map_err(|error| error.to_string()),
        )
        .expect("failed save enters recovery");
        assert_eq!(outcome, IntentOutcome::None);
        assert!(recovery.is_pending());
        assert!(
            model.board_form_open(),
            "recovery retains the expanded form"
        );

        apply_board_intent_with_save_recovery(
            &mut domain,
            &mut model,
            &mut recovery,
            BoardSaveContext {
                baseline: DomainState::new(),
                intent: BoardIntent::CancelSave,
                snapshot: None,
                host: None,
            },
            |_| panic!("CancelSave must not persist"),
        )
        .expect("cancel failed save");

        assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
        assert_eq!(model.quick_add_title_value(), "Retained title");
        assert!(
            model.board_form_open(),
            "complete expanded draft remains stashed"
        );
        assert_eq!(
            model.form_scope(),
            Some(&TaskScope::Project {
                path: "/repos/chosen".into()
            })
        );
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ExpandQuickAdd,
            Some(&snapshot),
            None,
        )
        .expect("reopen retained draft");
        assert_eq!(model.input_mode(), BoardInputMode::EditNotes);
        assert_eq!(model.edit_buffer(), "retained notes");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn confirmed_expanded_quick_add_clears_the_form_and_selects_the_new_task() {
        let temp = TempStore::new("confirmed-expanded-quick-add");
        let snapshot = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: None,
            title_prefill: Some("Expanded saved task".into()),
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, None);
        let mut recovery = SaveRecovery::new();

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            Some(&snapshot),
            None,
        )
        .expect("open quick add");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ExpandQuickAdd,
            Some(&snapshot),
            None,
        )
        .expect("expand quick add");
        let outcome = apply_board_intent_with_save_recovery(
            &mut domain,
            &mut model,
            &mut recovery,
            BoardSaveContext {
                baseline: DomainState::new(),
                intent: BoardIntent::ConfirmEdit,
                snapshot: Some(&snapshot),
                host: None,
            },
            |working| temp.store.save(working).map_err(|error| error.to_string()),
        )
        .expect("save expanded quick add");

        let id = domain.tasks()[0].id;
        assert_eq!(outcome, IntentOutcome::Persisted);
        assert!(!recovery.is_pending());
        assert_eq!(model.input_mode(), BoardInputMode::Normal);
        assert!(!model.board_form_open());
        assert_eq!(model.selected_id(), Some(id));
    }

    /// Without a snapshot, quick-add save must neither call `capture_save` nor report
    /// `Persist`. It keeps its draft and says why.
    #[test]
    fn quick_add_save_without_a_capture_snapshot_neither_saves_nor_reports_persist() {
        let mut domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, None);
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenCapture,
            None,
            None,
        )
        .expect("open quick add with no snapshot");
        assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::QuickAddInsert('x'),
            None,
            None,
        )
        .expect("type into title");

        let outcome = apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::QuickAddSave,
            None,
            None,
        )
        .expect("confirm without a snapshot");

        assert_eq!(
            outcome,
            IntentOutcome::None,
            "a snapshot-less confirm must not claim Persist"
        );
        assert!(domain.tasks().is_empty(), "nothing was ever saved");
        assert_eq!(
            model.input_mode(),
            BoardInputMode::QuickAdd,
            "quick add stays open so the draft is not lost"
        );
        assert_eq!(
            model.quick_add_title_value(),
            "x",
            "the draft the user typed must survive the refusal"
        );
        assert!(
            model.message().is_some(),
            "the refusal must say why, not silently no-op"
        );
    }

    /// Area below the 50x18 floor: the board paints resize guidance here.
    const RESIZE_AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 49,
        height: 18,
    };

    /// Host offering two eligible park sources, so Park opens the ambiguity picker.
    fn board_with_one_task() -> (DomainState, BoardModel) {
        let mut domain = DomainState::new();
        domain
            .create(
                "Behind an open surface",
                None,
                TaskScope::Project {
                    path: "/repos/app".into(),
                },
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .expect("create task");
        let model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
        (domain, model)
    }

    /// Every board state whose input mode is a modal surface, each already open when the
    /// pane shrinks below the resize floor.
    #[allow(clippy::type_complexity)]
    #[test]
    fn task_page_view_mode_routes_keys_through_the_page_keymap_not_the_form_field_map() {
        let (mut domain, mut model) = board_with_one_task();
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenTaskPage,
            None,
            None,
        )
        .expect("open page");
        assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
        assert!(model.board_form_open(), "the page keeps its form open");

        for (key, expected) in [
            (
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::ALT),
                BoardIntent::BeginEditTitle,
            ),
            (
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT),
                BoardIntent::BeginEditNotes,
            ),
            (
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT),
                BoardIntent::Complete,
            ),
            (
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::ALT),
                BoardIntent::PrimaryVerb,
            ),
        ] {
            assert_eq!(
                board_keyboard_intent(&model, BoardInputMode::TaskPage, key),
                Some(expected.clone()),
                "page view mode must route {key:?} through the page keymap"
            );
        }

        // A focused field hands back to the form field map: bare characters insert.
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::BeginEditTitle,
            None,
            None,
        )
        .expect("enter title edit");
        assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
        assert_eq!(
            board_keyboard_intent(
                &model,
                BoardInputMode::EditTitle,
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)
            ),
            Some(BoardIntent::EditInsert('e'))
        );
    }

    /// A form can remain allocated while a popup or view owns the resolved input mode. The
    /// keyboard must hand keys to the form only in its genuine field/dropdown modes, never just
    /// because `form_focus()` is present.
    #[test]
    fn board_keyboard_uses_the_form_mapper_only_for_form_field_and_dropdown_modes() {
        let (mut domain, mut model) = board_with_one_task();
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::BeginEditTitle,
            None,
            None,
        )
        .expect("open task form");
        assert!(model.board_form_open());

        for mode in [BoardInputMode::EditTitle, BoardInputMode::EditNotes] {
            assert_eq!(
                board_keyboard_intent(
                    &model,
                    mode,
                    KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
                ),
                Some(BoardIntent::FormFocusNext),
                "{mode:?} must route through the shared form mapper"
            );
        }
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FocusFormField(CaptureField::Scope),
            None,
            None,
        )
        .expect("focus scope field");
        assert_eq!(
            board_keyboard_intent(
                &model,
                BoardInputMode::EditScope,
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)
            ),
            Some(BoardIntent::FormCycleScope),
            "EditScope must route through the shared form mapper"
        );
        assert_eq!(
            board_keyboard_intent(
                &model,
                BoardInputMode::FormScopeDropdown,
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
            ),
            Some(BoardIntent::FormScopeNext),
            "the form scope dropdown must route through the shared form mapper"
        );

        // These modes may overlay an open form, but their own map must win. Each key is one the
        // form mapper would handle differently, so this is an allowlist regression guard rather
        // than a test of identical fallthroughs.
        for (mode, code, expected) in [
            (BoardInputMode::Normal, KeyCode::Char('r'), None),
            (
                BoardInputMode::TaskPage,
                KeyCode::Char('e'),
                Some(BoardIntent::BeginEditTitle),
            ),
            (
                BoardInputMode::ProjectPicker,
                KeyCode::Char('q'),
                Some(BoardIntent::CancelProjectPicker),
            ),
            (
                BoardInputMode::Recovery,
                KeyCode::Char('r'),
                Some(BoardIntent::RecoveryResume),
            ),
            (
                BoardInputMode::CleanupConfirm,
                KeyCode::Char('y'),
                Some(BoardIntent::ConfirmCleanup),
            ),
            (
                BoardInputMode::SaveRecovery,
                KeyCode::Char('r'),
                Some(BoardIntent::RetrySave),
            ),
            (
                BoardInputMode::Palette,
                KeyCode::Char('r'),
                Some(BoardIntent::CommandQueryInsert('r')),
            ),
            (
                BoardInputMode::Help,
                KeyCode::Char('r'),
                Some(BoardIntent::CloseLayer),
            ),
        ] {
            let mods = if mode == BoardInputMode::TaskPage {
                KeyModifiers::ALT
            } else {
                KeyModifiers::NONE
            };
            assert_eq!(
                board_keyboard_intent(&model, mode, KeyEvent::new(code, mods)),
                expected,
                "{mode:?} must not be swallowed by the open form"
            );
        }
    }

    /// SaveRecovery outranks an open form, so Retry and both Cancel keys must retain the only
    /// routes that can resolve a failed save.
    #[test]
    fn save_recovery_with_an_open_form_routes_r_c_and_esc_to_its_own_mapper() {
        let (mut domain, mut model) = board_with_one_task();
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::BeginEditTitle,
            None,
            None,
        )
        .expect("open task form");
        model.begin_save_recovery("injected save failure");
        assert!(
            model.board_form_open(),
            "save recovery retains the failed form"
        );
        assert_eq!(model.input_mode(), BoardInputMode::SaveRecovery);

        for (code, expected) in [
            (KeyCode::Char('r'), BoardIntent::RetrySave),
            (KeyCode::Char('c'), BoardIntent::CancelSave),
            (KeyCode::Esc, BoardIntent::CancelSave),
        ] {
            assert_eq!(
                board_keyboard_intent(
                    &model,
                    model.input_mode(),
                    KeyEvent::new(code, KeyModifiers::NONE)
                ),
                Some(expected),
                "SaveRecovery must own {code:?} while a form stays open"
            );
        }
    }

    /// `BoardMode` is now purely a layout classification (the compact tier vs the
    /// standard tier,); it no longer changes which input mode a key resolves to or
    /// which intents reach the reducer. Every surface the board can have open must resolve
    /// the identical input mode, and route the identical intent for every primary action key
    /// and for quit, at the legacy Resize band as at a wide size.
    ///
    /// Formerly two tests asserting the opposite, now-removed contract
    /// (`resize_guidance_keeps_keyboard_quit_reachable_from_every_open_surface`,
    /// `resize_guidance_blocks_task_mutation_from_every_open_surface`): every open surface was
    /// force-closed and forced to `Normal`, and every intent but quit/close-layer was
    /// swallowed below 50x18.
    /// Fake host returning fixed panes for attention poll tests.
    struct AttentionFakeHost {
        panes: Vec<PaneInfo>,
        list_calls: std::sync::atomic::AtomicUsize,
    }

    impl HostPorts for AttentionFakeHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            Ok(self.list_panes()?.into_iter().map(|p| p.pane_id).collect())
        }

        fn list_panes(&self) -> Result<Vec<PaneInfo>, String> {
            self.list_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.panes.clone())
        }

        fn focus_pane(&self, _pane_id: &str) -> Result<(), String> {
            Ok(())
        }

        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }
    }

    struct FailingAttentionHost;

    impl HostPorts for FailingAttentionHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            Err("host unavailable".into())
        }

        fn list_panes(&self) -> Result<Vec<PaneInfo>, String> {
            Err("host unavailable".into())
        }

        fn focus_pane(&self, _pane_id: &str) -> Result<(), String> {
            Ok(())
        }

        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn attention_cycle_with_fake_host_sets_review_and_syncs_model() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-attn-cycle-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();

        let store = TaskStore::new(&dir);
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Linked agent work",
                None,
                TaskScope::Global,
                None,
                Some(AgentMeta {
                    agent_id: Some("grok".into()),
                    pane_id: Some("w0:p1".into()),
                    agent_session: None,
                }),
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        store.save(&domain).unwrap();

        let mut model = BoardModel::from_domain(&domain, None);
        assert_eq!(domain.get(id).unwrap().status, HumanStatus::Ready);

        let host = AttentionFakeHost {
            panes: vec![PaneInfo {
                pane_id: "w0:p1".into(),
                agent: Some("grok".into()),
                observed: Some(ObservedStatus::Done),
                ..PaneInfo::default()
            }],
            list_calls: std::sync::atomic::AtomicUsize::new(0),
        };

        let result = run_attention_cycle(&store, &mut domain, &mut model, &host);
        assert!(result.status_changed.contains(&id));
        assert_eq!(domain.get(id).unwrap().status, HumanStatus::Review);
        assert_eq!(host.list_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            model
                .message()
                .is_some_and(|m| m.contains("updated from agent")),
            "signal change should set status message"
        );

        // Model reflects attention set after sync; preserve open message like run_board.
        let open_msg = model.message().map(str::to_string);
        let this_repo = model.this_repo().map(|p| p.to_path_buf());
        model = BoardModel::from_domain(&domain, this_repo);
        if let Some(msg) = open_msg {
            model.set_message(msg);
        }
        assert!(model
            .message()
            .is_some_and(|m| m.contains("updated from agent")));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn attention_cycle_save_failure_keeps_changed_status_visible_in_model() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-attn-save-failure-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();
        let store = TaskStore::new(&dir);
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Linked agent work",
                None,
                TaskScope::Global,
                None,
                Some(AgentMeta {
                    agent_id: Some("grok".into()),
                    pane_id: Some("w0:p1".into()),
                    agent_session: None,
                }),
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        store.save(&domain).unwrap();
        let mut model = BoardModel::from_domain(&domain, None);
        fs::remove_file(dir.join("tasks.json")).unwrap();
        fs::create_dir(dir.join("tasks.json")).unwrap();

        let result = run_attention_cycle(
            &store,
            &mut domain,
            &mut model,
            &AttentionFakeHost {
                panes: vec![PaneInfo {
                    pane_id: "w0:p1".into(),
                    agent: Some("grok".into()),
                    observed: Some(ObservedStatus::Done),
                    ..PaneInfo::default()
                }],
                list_calls: std::sync::atomic::AtomicUsize::new(0),
            },
        );

        assert!(result.status_changed.contains(&id));
        assert_eq!(domain.get(id).unwrap().status, HumanStatus::Review);
        assert!(
            model.visible_ids().contains(&id),
            "the board model must not lose the changed task after save failure"
        );
        assert!(model
            .message()
            .is_some_and(|message| message.contains("attention save failed")));
    }

    #[test]
    fn attention_cycle_host_failure_preserves_existing_stale_presentation() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-attn-unavailable-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();
        let store = TaskStore::new(&dir);
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Missing linked agent",
                None,
                TaskScope::Global,
                None,
                Some(AgentMeta {
                    agent_id: Some("grok".into()),
                    pane_id: Some("w0:missing".into()),
                    agent_session: None,
                }),
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        store.save(&domain).unwrap();
        let mut model = BoardModel::from_domain(&domain, None);
        let first = run_attention_cycle(
            &store,
            &mut domain,
            &mut model,
            &AttentionFakeHost {
                panes: Vec::new(),
                list_calls: std::sync::atomic::AtomicUsize::new(0),
            },
        );
        assert!(first.stale_ids().contains(&id));
        assert!(model.is_stale(id));

        let unavailable =
            run_attention_cycle(&store, &mut domain, &mut model, &FailingAttentionHost);
        assert!(unavailable
            .classifications
            .iter()
            .all(|(_, c)| *c != crate::attention::AttentionClassification::Stale));
        assert!(model.is_stale(id));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn attention_cycle_merges_disk_before_apply_keeps_newer_edit() {
        use std::fs;
        use std::thread;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-attn-merge-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();

        let store = TaskStore::new(&dir);
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Stale title",
                None,
                TaskScope::Global,
                None,
                Some(AgentMeta {
                    agent_id: Some("grok".into()),
                    pane_id: Some("w0:p1".into()),
                    agent_session: None,
                }),
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        store.save(&domain).unwrap();

        // Concurrent writer: newer title edit on disk while this board holds stale domain.
        thread::sleep(Duration::from_millis(5));
        let mut other = store.load().unwrap();
        other
            .edit(
                id,
                "Newer title from other board",
                None,
                TaskScope::Global,
                None,
            )
            .unwrap();
        store.save(&other).unwrap();

        // Local domain is still stale (old title) when poll runs.
        assert_eq!(domain.get(id).unwrap().title, "Stale title");

        let mut model = BoardModel::from_domain(&domain, None);
        let host = AttentionFakeHost {
            panes: vec![PaneInfo {
                pane_id: "w0:p1".into(),
                agent: Some("grok".into()),
                observed: Some(ObservedStatus::Done),
                ..PaneInfo::default()
            }],
            list_calls: std::sync::atomic::AtomicUsize::new(0),
        };

        let result = run_attention_cycle(&store, &mut domain, &mut model, &host);
        assert!(result.status_changed.contains(&id));
        let task = domain.get(id).unwrap();
        assert_eq!(task.status, HumanStatus::Review);
        assert_eq!(
            task.title, "Newer title from other board",
            "poll must merge disk before apply so newer edits are not overwritten"
        );

        let reloaded = store.load().unwrap();
        assert_eq!(
            reloaded.get(id).unwrap().title,
            "Newer title from other board"
        );
        assert_eq!(reloaded.get(id).unwrap().status, HumanStatus::Review);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn attention_cycle_noop_observation_does_not_rewrite_disk_updated_at() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-attn-noop-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();

        let store = TaskStore::new(&dir);
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Already review",
                None,
                TaskScope::Global,
                None,
                Some(AgentMeta {
                    agent_id: Some("grok".into()),
                    pane_id: Some("w0:p1".into()),
                    agent_session: None,
                }),
                ProvenanceOrigin::Capture,
            )
            .unwrap();
        domain.set_status(id, HumanStatus::Review).unwrap();
        store.save(&domain).unwrap();
        let disk_before = store.load().unwrap();
        let updated_before = disk_before.get(id).unwrap().updated_at;

        let mut model = BoardModel::from_domain(&domain, None);
        let host = AttentionFakeHost {
            panes: vec![PaneInfo {
                pane_id: "w0:p1".into(),
                agent: Some("grok".into()),
                observed: Some(ObservedStatus::Done),
                ..PaneInfo::default()
            }],
            list_calls: std::sync::atomic::AtomicUsize::new(0),
        };

        let result = run_attention_cycle(&store, &mut domain, &mut model, &host);
        assert!(!result.status_changed());
        assert!(!result.any_change());

        let disk_after = store.load().unwrap();
        assert_eq!(disk_after.get(id).unwrap().updated_at, updated_before);
        assert_eq!(disk_after.get(id).unwrap().status, HumanStatus::Review);

        let _ = fs::remove_dir_all(&dir);
    }

    /// a bracketed paste reaches the board's edit route, and only that route.
    ///
    /// The board loop's paste arm is exactly this resolution followed by the same
    /// `handle_board_intent` call the key arm makes, so a paste cannot reach a route a key
    /// press could not.
    #[test]
    fn a_board_paste_routes_to_the_edit_buffer_and_is_inert_outside_an_edit_mode() {
        let area = Rect::new(0, 0, 120, 40);
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Original",
                None,
                TaskScope::Project {
                    path: "/repos/app".into(),
                },
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
        assert_eq!(model.selected_id(), Some(id));

        // Normal board: a paste is not an edit and changes nothing.
        assert_eq!(board_paste_intent(area, &mut model, "pasted"), None);
        assert_eq!(model.edit_buffer(), "");
        assert_eq!(domain.get(id).expect("task").title, "Original");

        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::BeginEditTitle,
            None,
            None,
        )
        .expect("begin title edit");
        let intent =
            board_paste_intent(area, &mut model, "one\ntwo").expect("a paste in an edit mode");
        assert_eq!(intent, BoardIntent::EditInsertText("one\ntwo".to_string()));
        apply_intent(&mut domain, &mut model, intent, None, None).expect("insert the paste");
        assert!(
            model.edit_buffer().contains("one"),
            "the paste must land in the draft: {:?}",
            model.edit_buffer()
        );

        // the queue overlay paints an open editor as a full-screen takeover at every
        // size, so a paste it accepted before the pane shrank still lands at the
        // legacy Resize band and below.
        let intent = board_paste_intent(RESIZE_AREA, &mut model, "more")
            .expect("a paste still reaches the open editor down to 40x10");
        assert_eq!(intent, BoardIntent::EditInsertText("more".to_string()));
    }

    /// pasting into the open palette narrows the query exactly as typing it would.
    ///
    /// Bracketed paste routes the payload to `Event::Paste`, so without this route the
    /// palette search would silently swallow a paste that worked before the protocol was on.
    #[test]
    fn a_board_paste_into_the_open_palette_narrows_the_query_like_typing() {
        let area = Rect::new(0, 0, 120, 40);
        let mut domain = DomainState::new();
        domain
            .create(
                "Original",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");

        let open_palette = |model: &mut BoardModel, domain: &mut DomainState| {
            apply_intent(domain, model, BoardIntent::OpenCommandPalette, None, None)
                .expect("open the palette");
        };

        // Typed baseline: the same characters entered one key press at a time.
        let mut typed = BoardModel::from_domain(&domain, None);
        open_palette(&mut typed, &mut domain);
        for character in "park".chars() {
            apply_intent(
                &mut domain,
                &mut typed,
                BoardIntent::CommandQueryInsert(character),
                None,
                None,
            )
            .expect("type into the query");
        }

        let mut pasted = BoardModel::from_domain(&domain, None);
        open_palette(&mut pasted, &mut domain);
        let intent = board_paste_intent(area, &mut pasted, "park").expect("a paste in the palette");
        apply_intent(&mut domain, &mut pasted, intent, None, None).expect("insert the paste");

        assert_eq!(pasted.command_query(), typed.command_query());
        assert_eq!(
            pasted
                .visible_commands()
                .iter()
                .map(|command| command.label)
                .collect::<Vec<_>>(),
            typed
                .visible_commands()
                .iter()
                .map(|command| command.label)
                .collect::<Vec<_>>(),
            "a paste must narrow the palette exactly as typing the same run does"
        );

        // The query is one search line, so each break folds to a single space (CRLF included).
        let mut broken = BoardModel::from_domain(&domain, None);
        open_palette(&mut broken, &mut domain);
        let intent =
            board_paste_intent(area, &mut broken, "a\r\nb").expect("a paste in the palette");
        apply_intent(&mut domain, &mut broken, intent, None, None).expect("insert the paste");
        assert_eq!(broken.command_query(), "a b");
    }

    /// pasting a project path while the scope path editor is active extends the path.
    ///
    /// This is the most likely paste in the product; before bracketed paste it arrived as key
    /// presses and worked, so the paste route must keep it working.
    #[test]
    fn a_capture_paste_extends_the_scope_path_while_the_path_editor_is_active() {
        use crate::ui::capture::{CaptureField, CaptureScopeChoice};
        use crate::ui::input::CaptureIntent;

        let snap = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: None,
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = CaptureModel::from_snapshot(&snap);
        apply_capture_intent(
            &mut domain,
            None,
            &snap,
            &mut model,
            CaptureIntent::SelectScope(CaptureScopeChoice::Other),
        )
        .expect("begin the path edit");
        assert_eq!(model.focused(), CaptureField::Scope);
        assert!(model.is_path_editing());

        let intent = capture_paste_intent(&model, "/repos/app").expect("a paste into the path");
        apply_capture_intent(&mut domain, None, &snap, &mut model, intent)
            .expect("insert the paste");
        assert_eq!(model.scope_path_edit(), Some("/repos/app"));

        // The path is one line: a pasted break folds to a single space, CRLF included.
        let intent = capture_paste_intent(&model, "\r\nmore").expect("a paste into the path");
        apply_capture_intent(&mut domain, None, &snap, &mut model, intent)
            .expect("insert the paste");
        assert_eq!(model.scope_path_edit(), Some("/repos/app more"));

        // Scope focus without an active path editor is not a text field: inert, as before.
        apply_capture_intent(
            &mut domain,
            None,
            &snap,
            &mut model,
            CaptureIntent::FocusField(CaptureField::Title),
        )
        .expect("leave the path edit");
        apply_capture_intent(
            &mut domain,
            None,
            &snap,
            &mut model,
            CaptureIntent::FocusField(CaptureField::Scope),
        )
        .expect("focus scope");
        assert!(!model.is_path_editing());
        assert_eq!(capture_paste_intent(&model, "ignored"), None);
    }

    /// a Capture paste reaches the focused text field, and nowhere else.
    #[test]
    fn a_capture_paste_routes_to_the_focused_text_field_and_is_inert_elsewhere() {
        use crate::ui::capture::CaptureField;
        use crate::ui::input::CaptureIntent;

        let snap = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: None,
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = CaptureModel::from_snapshot(&snap);
        assert_eq!(model.focused(), CaptureField::Title);

        let intent = capture_paste_intent(&model, "one\ntwo").expect("a paste into Title");
        assert_eq!(intent, CaptureIntent::InsertText("one\ntwo".to_string()));
        apply_capture_intent(&mut domain, None, &snap, &mut model, intent)
            .expect("insert the paste");
        // Title is a single-line field:'s insert flattens the pasted newline.
        assert_eq!(model.title(), "one two");

        // The scope row is not a text field: a paste there changes nothing.
        apply_capture_intent(
            &mut domain,
            None,
            &snap,
            &mut model,
            CaptureIntent::FocusField(CaptureField::Scope),
        )
        .expect("focus scope");
        assert_eq!(capture_paste_intent(&model, "ignored"), None);
        assert_eq!(model.title(), "one two");
    }

    /// A failed save owns the form until Retry or Cancel, so a paste must be inert too.
    #[test]
    fn a_capture_paste_is_inert_while_a_save_failure_is_unresolved() {
        use std::fs;

        use crate::ui::capture::CaptureField;
        use crate::ui::input::CaptureIntent;

        let dir = temp_state_dir("capture-paste-recovery");
        // A plain file where the state directory should be: every save attempt fails.
        let blocked = dir.join("not-a-directory");
        fs::write(&blocked, "not a state directory").expect("blocking file");
        let store = TaskStore::new(&blocked);

        let snap = InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: None,
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: None,
            agent_meta: None,
        };
        let mut domain = DomainState::new();
        let mut model = CaptureModel::from_snapshot(&snap);
        apply_capture_intent(
            &mut domain,
            None,
            &snap,
            &mut model,
            CaptureIntent::InsertText("Pending".to_string()),
        )
        .expect("seed a title");
        apply_capture_intent(
            &mut domain,
            Some(&store),
            &snap,
            &mut model,
            CaptureIntent::Save,
        )
        .expect("a save failure stays on the form");
        assert!(model.is_save_recovery());
        assert_eq!(model.focused(), CaptureField::Title);

        assert_eq!(
            capture_paste_intent(&model, "pasted"),
            None,
            "recovery accepts only Retry or Cancel, by key or by paste"
        );
        assert_eq!(model.title(), "Pending");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Temp state dir for one open-refresh test.
    fn temp_state_dir(tag: &str) -> PathBuf {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-{tag}-{nanos}-{seq}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// One task linked to `w0:p1`, saved to a fresh store.
    #[test]
    fn refused_title_edit_keeps_its_draft_and_cursor_and_says_a_title_is_required() {
        for draft in ["", "   "] {
            let (mut domain, mut model) = board_with_one_task();
            let id = domain.tasks()[0].id;
            let before = domain.get(id).expect("task").clone();

            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::BeginEditTitle,
                None,
                None,
            )
            .expect("open the title edit");
            for _ in 0..before.title.chars().count() {
                apply_intent(
                    &mut domain,
                    &mut model,
                    BoardIntent::EditBackspace,
                    None,
                    None,
                )
                .expect("clear the seeded title");
            }
            for character in draft.chars() {
                apply_intent(
                    &mut domain,
                    &mut model,
                    BoardIntent::EditInsert(character),
                    None,
                    None,
                )
                .expect("type the draft");
            }
            // Leave the cursor somewhere other than the end, so "unchanged" is a real claim.
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::EditMoveLeft,
                None,
                None,
            )
            .expect("move the cursor");
            let cursor = model.edit_cursor();

            let mut recovery = SaveRecovery::new();
            let outcome = apply_board_intent_presenting_rejection(
                &mut domain,
                &mut model,
                &mut recovery,
                BoardSaveContext {
                    baseline: DomainState::new(),
                    intent: BoardIntent::ConfirmEdit,
                    snapshot: None,
                    host: None,
                },
                |_| panic!("a refused edit must never persist"),
            );

            assert_eq!(outcome, IntentOutcome::None, "draft {draft:?}");
            assert_eq!(
                model.input_mode(),
                BoardInputMode::EditTitle,
                "draft {draft:?}: the field must stay open"
            );
            assert_eq!(
                model.edit_buffer(),
                draft,
                "draft {draft:?}: the typed text must survive the refusal"
            );
            assert_eq!(
                model.edit_cursor(),
                cursor,
                "draft {draft:?}: the cursor must not jump"
            );
            assert_eq!(
                model.message(),
                Some(TITLE_REQUIRED_MESSAGE),
                "draft {draft:?}: the refusal must explain itself"
            );
            assert_eq!(
                domain.get(id).expect("task"),
                &before,
                "draft {draft:?}: a refused edit changes no task"
            );
        }
    }

    /// `ConfirmEdit` reads the durable record for its availability verdict and is **not**
    /// merged from it.
    ///
    /// The distinction is the whole design, and neither half is safe to leave implicit. Adding
    /// `ConfirmEdit` to the merge set is the obvious-looking way to satisfy decision 8 and it
    /// silently defeats same-task save-conflict detection (measured: see
    /// `a_concurrent_edit_to_the_bound_task_still_reaches_the_save_conflict` in
    /// `tests/edit_target_binding.rs`). Dropping the verdict entirely puts the stale-snapshot
    /// hole back. This pins both sides.
    #[test]
    fn a_refusals_line_is_readable_and_never_a_uuid_dump() {
        let id = uuid::Uuid::from_u128(7);

        assert_eq!(
            board_rejection_message(&DomainError::EmptyTitle),
            TITLE_REQUIRED_MESSAGE,
            "both surfaces state the same refusal in the same words"
        );
        for error in [DomainError::UnknownId(id), DomainError::SoftDeleted(id)] {
            let line = board_rejection_message(&error);
            assert!(
                !line.contains(&id.to_string()),
                "{error:?} reported an id the user cannot act on: {line:?}"
            );
            assert!(
                line.len() <= 40,
                "{error:?} needs more than a narrow board row: {line:?}"
            );
        }
        // Unmapped: the domain's own words, but never nothing.
        let unmapped = DomainError::StaleUndo(id);
        assert_eq!(
            board_rejection_message(&unmapped),
            unmapped.to_string(),
            "a refusal this boundary has never seen must still be explained"
        );
    }
}
