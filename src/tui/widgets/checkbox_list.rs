//! Selectable item checkbox list widget for the Plan view.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::planner::{OpStatus, SortOperation};

pub struct CheckboxList;

impl CheckboxList {
    pub fn render<'a>(
        operations: &'a [SortOperation],
        selected_idx: usize,
        title: &'a str,
    ) -> List<'a> {
        let items: Vec<ListItem> = operations
            .iter()
            .enumerate()
            .map(|(i, op)| {
                let is_cursor = i == selected_idx;
                let check = if op.selected { "[x] " } else { "[ ] " };

                let status_icon = match op.status {
                    OpStatus::Pending => "⏳ ",
                    OpStatus::InProgress => "🔄 ",
                    OpStatus::Completed => "✓ ",
                    OpStatus::Failed => "✗ ",
                    OpStatus::Skipped => "⊘ ",
                };

                let check_style = if op.selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let status_style = match op.status {
                    OpStatus::Completed => Style::default().fg(Color::Green),
                    OpStatus::Failed => Style::default().fg(Color::Red),
                    OpStatus::InProgress => Style::default().fg(Color::Yellow),
                    _ => Style::default().fg(Color::Gray),
                };

                let text_style = if is_cursor {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(Color::Rgb(30, 40, 60))
                } else {
                    Style::default().fg(Color::White)
                };

                let src_name = std::path::Path::new(&op.source)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&op.source);

                let line = Line::from(vec![
                    Span::styled(check, check_style),
                    Span::styled(status_icon, status_style),
                    Span::styled(format!("{:<32}", src_name), text_style),
                    Span::styled(format!(" {:>10}", op.size_human), Style::default().fg(Color::Yellow)),
                    Span::styled(format!("  → {}", op.destination), Style::default().fg(Color::DarkGray)),
                ]);

                ListItem::new(line)
            })
            .collect();

        List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
    }
}
