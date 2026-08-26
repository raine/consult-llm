mod detail;
mod markdown;
mod table;

use crate::state::{AppMode, AppState, BG, DIM_WHITE, SEPARATOR, TEAL, WHITE};

use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

pub(crate) use markdown::init_syntax;

pub(crate) fn render(frame: &mut ratatui::Frame, state: &mut AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    // Reset rects each frame; the table renderer fills these when in Table-like
    // modes so mouse input can hit-test against the most recent layout.
    state.last_active_inner = None;
    state.last_history_inner = None;

    match &state.mode {
        AppMode::Table => {
            table::render_table_view(frame, area, state);
        }
        AppMode::ConfirmClearHistory => {
            table::render_table_view(frame, area, state);
            render_confirm_dialog(frame, area, "Clear all history?");
        }
        AppMode::ConfirmKillProcess(pid) => {
            let pid = *pid;
            table::render_table_view(frame, area, state);
            render_confirm_dialog(frame, area, &format!("Kill CLI process {pid}?"));
        }
        AppMode::Detail(_) => {
            detail::render_detail_view(frame, area, state);
        }
        AppMode::ThreadDetail(_) => {
            detail::render_thread_detail_view(frame, area, state);
        }
    }

    if state.show_help {
        render_help_overlay(frame, &state.mode);
    }
}

fn render_help_overlay(frame: &mut ratatui::Frame, mode: &AppMode) {
    let shortcuts: Vec<(&str, &str)> = match mode {
        AppMode::Detail(_) => vec![
            ("j / ↓", "Scroll down"),
            ("k / ↑", "Scroll up"),
            ("d", "Half page down"),
            ("u", "Half page up"),
            ("Wheel", "Scroll"),
            ("Tab / S-Tab", "Next / prev sibling"),
            ("g / G / Home / End", "Scroll to top / bottom"),
            ("r", "Jump to response"),
            ("y", "Yank response"),
            ("s", "Toggle system prompt"),
            ("Esc", "Back to table"),
            ("q", "Quit"),
            ("?", "Toggle this help"),
        ],
        AppMode::ThreadDetail(_) => vec![
            ("j / ↓", "Scroll down"),
            ("k / ↑", "Scroll up"),
            ("d", "Half page down"),
            ("u", "Half page up"),
            ("Wheel", "Scroll"),
            ("[ / ]", "Prev / next turn"),
            ("g / G / Home / End", "Scroll to top / bottom"),
            ("r", "Jump to response"),
            ("y", "Yank response"),
            ("Esc", "Back to table"),
            ("q", "Quit"),
            ("?", "Toggle this help"),
        ],
        _ => vec![
            ("j / ↓", "Move down"),
            ("k / ↑", "Move up"),
            ("Wheel", "Move selection"),
            ("Click", "Select / open row"),
            ("Tab", "Switch focus"),
            ("Enter", "Open detail view"),
            ("K", "Kill process"),
            ("/", "Filter history"),
            ("X", "Clear history"),
            ("q", "Quit"),
            ("?", "Toggle this help"),
        ],
    };

    let title = " Shortcuts ";

    let area = frame.area();
    let max_key_len = shortcuts
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let max_action_len = shortcuts
        .iter()
        .map(|(_, a)| a.chars().count())
        .max()
        .unwrap_or(0);
    // 2 left padding + key + " │ " (3) + action + 2 right padding + 2 border
    let menu_width = (max_key_len + max_action_len + 9) as u16;
    // 1 top padding + shortcuts + 1 bottom padding + 2 border
    let menu_height = shortcuts.len() as u16 + 4;

    let menu_area = Rect {
        x: (area.width.saturating_sub(menu_width)) / 2,
        y: (area.height.saturating_sub(menu_height)) / 2,
        width: menu_width.min(area.width),
        height: menu_height.min(area.height),
    };

    frame.render_widget(Clear, menu_area);

    let background = Block::default().style(Style::default().bg(BG));
    frame.render_widget(background, menu_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TEAL));

    let inner = block.inner(menu_area);
    frame.render_widget(block, menu_area);

    let mut lines = Vec::new();
    lines.push(Line::from(""));
    for (key, action) in &shortcuts {
        let key_padding = max_key_len - key.chars().count();
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{}{}", key, " ".repeat(key_padding)),
                Style::default().fg(TEAL),
            ),
            Span::styled(" │ ", Style::default().fg(SEPARATOR)),
            Span::styled(*action, Style::default().fg(WHITE)),
        ]));
    }

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
}

fn render_confirm_dialog(frame: &mut ratatui::Frame, area: Rect, message: &str) {
    let popup_width = (message.len() as u16 + 8).max(36);
    let popup_height = 5;

    let [popup_area] = Layout::horizontal([Constraint::Length(popup_width)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::vertical([Constraint::Length(popup_height)])
        .flex(Flex::Center)
        .areas(popup_area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SEPARATOR))
        .style(Style::default().bg(BG));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines = vec![
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(DIM_WHITE).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(TEAL)),
            Span::styled(" confirm  ", Style::default().fg(DIM_WHITE)),
            Span::styled("n", Style::default().fg(TEAL)),
            Span::styled(" cancel", Style::default().fg(DIM_WHITE)),
        ])
        .alignment(Alignment::Center),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}
