use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::TableState;

use crate::action::Action;
use crate::state::{AppMode, AppState, Focus, HistoryDisplayRow, RowInfo};

/// Pure mapping from key events to actions. No state mutation.
pub(crate) fn handle_key(
    state: &AppState,
    row_infos: &[RowInfo],
    key: KeyEvent,
    dir: &Path,
) -> Option<Action> {
    // When help overlay is visible, only handle dismiss keys
    if state.show_help {
        return match key.code {
            KeyCode::Char('?') | KeyCode::Esc => Some(Action::ToggleHelp),
            _ => None,
        };
    }

    // When filter input is active, route all keys to filter handling
    if state.filter_editing {
        return handle_filter_key(key);
    }

    match &state.mode {
        AppMode::Table => handle_table_key(state, row_infos, key, dir),
        AppMode::Detail(_) => handle_detail_key(key),
        AppMode::ThreadDetail(_) => handle_thread_detail_key(key),
        AppMode::ConfirmClearHistory => handle_confirm_clear_key(key),
        AppMode::ConfirmKillProcess(pid) => handle_confirm_kill_key(key, *pid),
    }
}

fn handle_table_key(
    state: &AppState,
    row_infos: &[RowInfo],
    key: KeyEvent,
    dir: &Path,
) -> Option<Action> {
    match key.code {
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Tab | KeyCode::BackTab => Some(Action::ToggleFocus),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('/') => Some(Action::StartFilter),
        KeyCode::Char('X') => Some(Action::PromptClearHistory),
        KeyCode::Char('K') => {
            if matches!(state.focus, Focus::Active)
                && let Some(info) = row_infos.get(state.selected)
                && let Some(run) = state.active_runs.get(&info.run_id)
            {
                return Some(Action::PromptKillProcess(run.pid));
            }
            None
        }
        KeyCode::Enter => match state.focus {
            Focus::Active => {
                if let Some(info) = row_infos.get(state.selected) {
                    return Some(Action::EnterDetail(info.run_id.clone()));
                }
                None
            }
            Focus::History => open_history_row(state, state.history_selected, dir),
        },
        _ => None,
    }
}

fn handle_confirm_clear_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') => Some(Action::ClearHistory),
        _ => Some(Action::CancelClear),
    }
}

fn handle_confirm_kill_key(key: KeyEvent, pid: u32) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') => Some(Action::KillProcess(pid)),
        _ => Some(Action::CancelKill),
    }
}

fn handle_filter_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::FilterAccept),
        KeyCode::Esc => Some(Action::FilterCancel),
        KeyCode::Backspace => Some(Action::FilterBackspace),
        KeyCode::Char(c) => Some(Action::FilterInput(c)),
        _ => None,
    }
}

fn handle_common_detail_key(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::ExitDetail),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::ScrollDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::ScrollUp),
        KeyCode::Char('d') => Some(Action::HalfPageDown),
        KeyCode::Char('u') => Some(Action::HalfPageUp),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Char('G') | KeyCode::End => Some(Action::ScrollToBottom),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::ScrollToTop),
        KeyCode::Char('r') => Some(Action::ScrollToResponse),
        KeyCode::Char('y') => Some(Action::YankResponse),
        _ => None,
    }
}

fn handle_detail_key(key: KeyEvent) -> Option<Action> {
    handle_common_detail_key(&key).or_else(|| handle_run_detail_key(&key))
}

fn handle_run_detail_key(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('s') => Some(Action::ToggleSystemPrompt),
        KeyCode::Tab => Some(Action::NextSibling),
        KeyCode::BackTab => Some(Action::PrevSibling),
        _ => None,
    }
}

fn open_history_row(state: &AppState, history_idx: usize, dir: &Path) -> Option<Action> {
    let display_rows = state.build_history_display_rows();
    match display_rows.get(history_idx) {
        Some(HistoryDisplayRow::Single(idx)) => {
            let record = &state.history[*idx];
            if let Some(run_id) = &record.run_id {
                let path = dir.join("runs").join(format!("{run_id}.events.jsonl"));
                if path.exists() {
                    Some(Action::EnterDetail(run_id.clone()))
                } else {
                    Some(Action::Flash("log file not found".into(), 20))
                }
            } else {
                Some(Action::Flash("no log available for this entry".into(), 20))
            }
        }
        Some(HistoryDisplayRow::ThreadSummary { thread_id, .. }) => {
            Some(Action::EnterThreadDetail(thread_id.clone()))
        }
        None => None,
    }
}

/// Pure mapping from mouse events to actions.
pub(crate) fn handle_mouse(
    state: &AppState,
    row_infos: &[RowInfo],
    event: MouseEvent,
    dir: &Path,
) -> Option<Action> {
    if state.show_help || state.filter_editing {
        return None;
    }
    if matches!(
        state.mode,
        AppMode::ConfirmClearHistory | AppMode::ConfirmKillProcess(_)
    ) {
        return None;
    }

    match event.kind {
        MouseEventKind::ScrollDown => handle_scroll(state, event, false),
        MouseEventKind::ScrollUp => handle_scroll(state, event, true),
        MouseEventKind::Down(MouseButton::Left) => handle_left_click(state, row_infos, event, dir),
        _ => None,
    }
}

fn handle_scroll(state: &AppState, event: MouseEvent, up: bool) -> Option<Action> {
    match state.mode {
        AppMode::Table => {
            // Pointer-aware: if the wheel is over a specific subtable, target
            // its selection; otherwise fall back to the focused subtable.
            let over_active = state
                .last_active_inner
                .is_some_and(|r| point_in_rect(event.column, event.row, r));
            let over_history = state
                .last_history_inner
                .is_some_and(|r| point_in_rect(event.column, event.row, r));

            if over_active {
                let next = step_index(state.selected, state.row_count, up);
                next.map(Action::SelectActiveRow)
            } else if over_history {
                let count = state.build_history_display_rows().len();
                let next = step_index(state.history_selected, count, up);
                next.map(Action::SelectHistoryRow)
            } else {
                Some(if up { Action::MoveUp } else { Action::MoveDown })
            }
        }
        AppMode::Detail(_) | AppMode::ThreadDetail(_) => Some(if up {
            Action::ScrollUp
        } else {
            Action::ScrollDown
        }),
        _ => None,
    }
}

fn step_index(current: usize, count: usize, up: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if up {
        Some(current.saturating_sub(1))
    } else {
        Some((current + 1).min(count - 1))
    }
}

fn handle_left_click(
    state: &AppState,
    row_infos: &[RowInfo],
    event: MouseEvent,
    dir: &Path,
) -> Option<Action> {
    if !matches!(state.mode, AppMode::Table) {
        return None;
    }

    if let Some(rect) = state.last_active_inner
        && let Some(idx) = row_at(rect, &state.table_state, event.column, event.row)
        && idx < row_infos.len()
    {
        if idx == state.selected {
            return Some(Action::EnterDetail(row_infos[idx].run_id.clone()));
        }
        return Some(Action::SelectActiveRow(idx));
    }

    if let Some(rect) = state.last_history_inner
        && let Some(idx) = row_at(rect, &state.history_table_state, event.column, event.row)
    {
        let count = state.build_history_display_rows().len();
        if idx >= count {
            return None;
        }
        if idx == state.history_selected {
            return open_history_row(state, idx, dir);
        }
        return Some(Action::SelectHistoryRow(idx));
    }

    None
}

fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// Map a mouse coordinate inside a stateful Table's inner area to the absolute
/// data-row index. Returns `None` when the click is on the header, on an empty
/// row below the data, or outside the rect.
///
/// Inner-area layout: row 0 is the header; rows 1.. are data, mapped through
/// the table's scroll offset.
fn row_at(rect: Rect, table_state: &TableState, x: u16, y: u16) -> Option<usize> {
    if !point_in_rect(x, y, rect) {
        return None;
    }
    if rect.height < 2 {
        return None;
    }
    let header_y = rect.y;
    if y <= header_y {
        return None;
    }
    let visible = (y - header_y - 1) as usize;
    Some(table_state.offset() + visible)
}

fn handle_thread_detail_key(key: KeyEvent) -> Option<Action> {
    handle_common_detail_key(&key).or_else(|| handle_thread_turn_key(&key))
}

fn handle_thread_turn_key(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('[') => Some(Action::PrevTurn),
        KeyCode::Char(']') => Some(Action::NextTurn),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consult_llm_core::monitoring::HistoryRecord;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers, MouseButton};
    use std::path::PathBuf;

    fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_state_with_active(rows: usize) -> (AppState, Vec<RowInfo>) {
        let mut state = AppState::new();
        let row_infos: Vec<RowInfo> = (0..rows)
            .map(|i| RowInfo {
                run_id: format!("run-{i}"),
            })
            .collect();
        state.row_count = rows;
        // Active inner area: x=0,y=0,w=80,h=12  → header at y=0, data at y=1..11
        state.last_active_inner = Some(Rect::new(0, 0, 80, 12));
        // History inner area: x=0,y=12,w=80,h=20 → header at y=12, data at y=13..31
        state.last_history_inner = Some(Rect::new(0, 12, 80, 20));
        (state, row_infos)
    }

    fn push_history(state: &mut AppState, n: usize) {
        for i in 0..n {
            state.history.push_back(HistoryRecord {
                ts: format!("2026-04-28T00:00:0{i}.000Z"),
                run_id: Some(format!("hist-{i}")),
                project: "p".into(),
                model: "m".into(),
                backend: "api".into(),
                duration_ms: 0,
                success: true,
                error: None,
                tokens_in: None,
                tokens_out: None,
                cost: None,
                parsed_ts: None,
                thread_id: None,
                reasoning_effort: None,
                task_mode: None,
            });
        }
        state.invalidate_filter_cache();
        state.ensure_filter_cache();
    }

    fn mouse_action(
        state: &AppState,
        rows: &[RowInfo],
        kind: MouseEventKind,
        row: u16,
    ) -> Option<Action> {
        handle_mouse(
            state,
            rows,
            mouse_event(kind, 5, row),
            &PathBuf::from("/tmp/x"),
        )
    }

    fn left_click(state: &AppState, rows: &[RowInfo], row: u16) -> Option<Action> {
        mouse_action(state, rows, MouseEventKind::Down(MouseButton::Left), row)
    }

    #[test]
    fn wheel_down_over_active_table_selects_next_row() {
        let (state, _rows) = make_state_with_active(3);
        let dir = PathBuf::from("/tmp/x");
        let action = handle_mouse(
            &state,
            &_rows,
            mouse_event(MouseEventKind::ScrollDown, 5, 5),
            &dir,
        );
        assert!(matches!(action, Some(Action::SelectActiveRow(1))));
    }

    #[test]
    fn wheel_up_over_active_clamps_at_zero() {
        let (state, rows) = make_state_with_active(3);
        let dir = PathBuf::from("/tmp/x");
        let action = handle_mouse(
            &state,
            &rows,
            mouse_event(MouseEventKind::ScrollUp, 5, 5),
            &dir,
        );
        assert!(matches!(action, Some(Action::SelectActiveRow(0))));
    }

    #[test]
    fn wheel_in_detail_emits_scroll() {
        let (mut state, rows) = make_state_with_active(0);
        // Simulate Detail mode using enter_detail with non-existent run is hard;
        // construct a minimal AppMode::Detail manually by using the public ctor path.
        // Instead, test via apply: this test only checks the routing, so we set
        // mode directly using the EnterDetail action path.
        state.apply(Action::EnterDetail("nope".into()));
        let dir = PathBuf::from("/tmp/x");
        let action = handle_mouse(
            &state,
            &rows,
            mouse_event(MouseEventKind::ScrollDown, 5, 5),
            &dir,
        );
        assert!(matches!(action, Some(Action::ScrollDown)));
    }

    #[test]
    fn click_active_row_selects() {
        let (state, rows) = make_state_with_active(5);
        // Click on visible row at y=3 (header at 0, data starts y=1, so idx=2).
        let action = left_click(&state, &rows, 3);
        assert!(matches!(action, Some(Action::SelectActiveRow(2))));
    }

    #[test]
    fn click_already_selected_active_opens_detail() {
        let (mut state, rows) = make_state_with_active(5);
        state.selected = 2;
        state.focus = Focus::Active;
        let action = left_click(&state, &rows, 3);
        match action {
            Some(Action::EnterDetail(id)) => assert_eq!(id, "run-2"),
            other => panic!("expected EnterDetail, got {other:?}"),
        }
    }

    #[test]
    fn click_active_when_history_focused_selects_and_switches_focus() {
        let (mut state, rows) = make_state_with_active(5);
        state.focus = Focus::History;
        let action = left_click(&state, &rows, 3);
        // idx=2, not the selected row → SelectActiveRow.
        assert!(matches!(action, Some(Action::SelectActiveRow(2))));
    }

    #[test]
    fn click_already_selected_active_opens_even_when_history_focused() {
        let (mut state, rows) = make_state_with_active(5);
        state.selected = 2;
        state.focus = Focus::History;
        let action = left_click(&state, &rows, 3);
        match action {
            Some(Action::EnterDetail(id)) => assert_eq!(id, "run-2"),
            other => panic!("expected EnterDetail, got {other:?}"),
        }
    }

    #[test]
    fn click_already_selected_history_opens_even_when_active_focused() {
        let (mut state, rows) = make_state_with_active(2);
        push_history(&mut state, 4);
        state.history_selected = 1;
        state.focus = Focus::Active;
        // y=14 → history idx=1.
        let action = left_click(&state, &rows, 14);
        // History row 1 is a Single record without a runs file on disk, so we
        // expect a Flash, not SelectHistoryRow. The point is the open path was
        // taken regardless of focus.
        assert!(matches!(action, Some(Action::Flash(_, _))));
    }

    #[test]
    fn click_history_row_emits_select_history() {
        let (mut state, rows) = make_state_with_active(2);
        push_history(&mut state, 4);
        // History rect starts at y=12; data row 0 is y=13. y=14 → idx=1.
        let action = left_click(&state, &rows, 14);
        assert!(matches!(action, Some(Action::SelectHistoryRow(1))));
    }

    #[test]
    fn click_on_active_header_is_ignored() {
        let (state, rows) = make_state_with_active(3);
        // y=0 is header
        let action = left_click(&state, &rows, 0);
        assert!(action.is_none());
    }

    #[test]
    fn click_below_last_active_row_is_ignored() {
        let (state, rows) = make_state_with_active(3);
        // Active rect goes 0..12; click at y=8 maps to idx=7, > 3 rows.
        // But it would still be inside history rect (y >= 12 only)? y=8 is in
        // active rect since rect.y=0, h=12. idx=7 fails row_infos bound.
        let action = left_click(&state, &rows, 8);
        assert!(action.is_none());
    }

    #[test]
    fn modal_swallows_mouse() {
        let (mut state, rows) = make_state_with_active(3);
        state.show_help = true;
        let action = left_click(&state, &rows, 3);
        assert!(action.is_none());
    }

    #[test]
    fn confirm_kill_swallows_mouse() {
        let (mut state, rows) = make_state_with_active(3);
        state.mode = AppMode::ConfirmKillProcess(123);
        let action = left_click(&state, &rows, 3);
        assert!(action.is_none());
    }

    #[test]
    fn wheel_respects_table_offset() {
        let (mut state, rows) = make_state_with_active(50);
        state.selected = 30;
        let dir = PathBuf::from("/tmp/x");
        let action = handle_mouse(
            &state,
            &rows,
            mouse_event(MouseEventKind::ScrollDown, 5, 5),
            &dir,
        );
        assert!(matches!(action, Some(Action::SelectActiveRow(31))));
    }

    #[test]
    fn click_offset_respected() {
        // Build state with a table_state.offset of 10. Click visual row 2.
        let (mut state, rows) = make_state_with_active(50);
        state.table_state.select(Some(15));
        // Force the inner offset to 10. ratatui's TableState exposes .offset_mut().
        *state.table_state.offset_mut() = 10;
        // y=3 → visible idx=2 → data idx = 10+2 = 12
        let action = left_click(&state, &rows, 3);
        assert!(matches!(action, Some(Action::SelectActiveRow(12))));
    }

    #[test]
    fn home_and_end_scroll_detail_to_bounds() {
        assert!(matches!(
            handle_detail_key(key(KeyCode::Home)),
            Some(Action::ScrollToTop)
        ));
        assert!(matches!(
            handle_detail_key(key(KeyCode::End)),
            Some(Action::ScrollToBottom)
        ));
    }

    #[test]
    fn home_and_end_scroll_thread_detail_to_bounds() {
        assert!(matches!(
            handle_thread_detail_key(key(KeyCode::Home)),
            Some(Action::ScrollToTop)
        ));
        assert!(matches!(
            handle_thread_detail_key(key(KeyCode::End)),
            Some(Action::ScrollToBottom)
        ));
    }

    #[test]
    fn keyboard_q_still_quits() {
        // Sanity check that the key path still works after refactor.
        let (state, rows) = make_state_with_active(1);
        let dir = PathBuf::from("/tmp/x");
        let action = handle_key(&state, &rows, key(KeyCode::Char('q')), &dir);
        assert!(matches!(action, Some(Action::Quit)));
    }
}
