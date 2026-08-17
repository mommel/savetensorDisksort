//! Transfer and hashing progress bar widget.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge};
use ratatui::Frame;

use crate::utils::format_bytes;

pub fn render_transfer_progress(
    f: &mut Frame,
    area: Rect,
    current_file: &str,
    copied_bytes: u64,
    total_bytes: u64,
    percent: u16,
) {
    let label = if total_bytes > 0 {
        format!(
            "{} / {} ({}%)",
            format_bytes(copied_bytes),
            format_bytes(total_bytes),
            percent
        )
    } else {
        "0 B / 0 B (0%)".to_string()
    };

    let title = if current_file.is_empty() {
        "Transfer Progress".to_string()
    } else {
        format!("Transferring: {}", current_file)
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .bg(Color::Rgb(20, 30, 45))
                .add_modifier(Modifier::BOLD),
        )
        .percent(percent.min(100))
        .label(label);

    f.render_widget(gauge, area);
}
