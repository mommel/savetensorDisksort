//! TUI View: Post-execution summary and drive metrics.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::planner::SortPlan;
use crate::utils::format_bytes;

pub fn render_summary_view(f: &mut Frame, area: Rect, plan: Option<&SortPlan>) {
    let lines = if let Some(plan) = plan {
        let mut completed_count = 0;
        let mut failed_count = 0;
        let mut skipped_count = 0;
        let mut moved_bytes = 0u64;

        for op in &plan.operations {
            match op.status {
                crate::planner::OpStatus::Completed => {
                    completed_count += 1;
                    moved_bytes += op.size_bytes;
                }
                crate::planner::OpStatus::Failed => failed_count += 1,
                _ => skipped_count += 1,
            }
        }

        let mut res = vec![
            Line::from(vec![
                Span::styled(
                    "Execution Summary Report",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("Operations Completed: "),
                Span::styled(
                    completed_count.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  |  Failed: "),
                Span::styled(
                    failed_count.to_string(),
                    Style::default().fg(if failed_count > 0 { Color::Red } else { Color::Green }),
                ),
                Span::raw("  |  Skipped: "),
                Span::styled(skipped_count.to_string(), Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::raw("Total Data Relocated: "),
                Span::styled(
                    format_bytes(moved_bytes),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("Target Drive: "),
                Span::styled(&plan.target_drive, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Estimated Space Freed per Source Drive:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ]),
        ];

        for (drive, bytes) in &plan.space_analysis.source_drives_freed {
            res.push(Line::from(vec![
                Span::styled(format!("  • {:<20}", drive), Style::default().fg(Color::Cyan)),
                Span::styled(format!(" freed {}", format_bytes(*bytes)), Style::default().fg(Color::Green)),
            ]));
        }

        res.push(Line::from(""));
        res.push(Line::from(Span::styled(
            "All completed operations have been BLAKE3 verified and symlink references updated.",
            Style::default().fg(Color::Green),
        )));

        res
    } else {
        vec![Line::from("No execution has been performed yet.")]
    };

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Summary")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(p, area);
}
