//! TUI View: Live execution progress and console stream.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::tui::widgets::render_transfer_progress;

pub fn render_exec_view(
    f: &mut Frame,
    area: Rect,
    current_op_id: &str,
    current_file: &str,
    copied_bytes: u64,
    total_bytes: u64,
    percent: u16,
    log_messages: &[String],
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(8)])
        .split(area);

    let progress_title = if current_op_id.is_empty() {
        "Execution Progress (Idle)"
    } else {
        current_op_id
    };

    let display_title = if !current_file.is_empty() {
        current_file
    } else {
        progress_title
    };

    render_transfer_progress(
        f,
        chunks[0],
        display_title,
        copied_bytes,
        total_bytes,
        percent,
    );

    let log_items: Vec<ListItem> = log_messages
        .iter()
        .rev()
        .take(50)
        .map(|msg| {
            let color = if msg.contains("error") || msg.contains("Error") || msg.contains("✗") {
                Color::Red
            } else if msg.contains("✓") || msg.contains("complete") {
                Color::Green
            } else if msg.contains("DRY-RUN") {
                Color::Yellow
            } else {
                Color::Gray
            };

            ListItem::new(Line::from(vec![
                Span::styled("› ", Style::default().fg(Color::DarkGray)),
                Span::styled(msg, Style::default().fg(color)),
            ]))
        })
        .collect();

    let logs_widget = List::new(log_items).block(
        Block::default()
            .title("Execution Log (Latest Events)")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(logs_widget, chunks[1]);
}
