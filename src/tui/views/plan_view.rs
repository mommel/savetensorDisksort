//! TUI View: Sort plan review and interactive item selection.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::planner::SortPlan;
use crate::tui::widgets::CheckboxList;
use crate::utils::format_bytes;

pub fn render_plan_view(
    f: &mut Frame,
    area: Rect,
    plan: Option<&SortPlan>,
    selected_op_index: usize,
) {
    if let Some(plan) = plan {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(10)])
            .split(area);

        // Top: Space Analysis & Target Info
        let fits_style = if plan.space_analysis.fits {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        };

        let fits_text = if plan.space_analysis.fits {
            "✓ Fits on target drive (with 5% safety margin)"
        } else {
            "✗ Insufficient space on target drive!"
        };

        let header_lines = vec![
            Line::from(vec![
                Span::styled("Target Drive: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&plan.target_drive, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("  |  Total Operations: "),
                Span::styled(format!("{}", plan.operations.len()), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Required Move Space: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format_bytes(plan.space_analysis.total_move_size), Style::default().fg(Color::Yellow)),
                Span::raw("  |  Target Free Space: "),
                Span::styled(format_bytes(plan.space_analysis.target_drive_free_before), Style::default().fg(Color::Cyan)),
                Span::raw(" → "),
                Span::styled(format_bytes(plan.space_analysis.target_drive_free_after), Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("Capacity Check: ", Style::default().fg(Color::DarkGray)),
                Span::styled(fits_text, fits_style),
            ]),
            Line::from(vec![
                Span::styled(
                    "Keys: [Space] Toggle Item  |  [A] Select/Deselect All  |  [D] Dry-Run  |  [X] Execute",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ];

        let header_p = Paragraph::new(header_lines)
            .block(
                Block::default()
                    .title("Plan Configuration & Capacity Check")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(header_p, chunks[0]);

        // Bottom: Checkbox list of operations
        let op_list = CheckboxList::render(
            &plan.operations,
            selected_op_index,
            "Planned Move Operations ([Space] to toggle, [A] all)",
        );
        f.render_widget(op_list, chunks[1]);
    } else {
        let empty_p = Paragraph::new(
            "No sort plan generated yet. Generate one via CLI 'disksort plan' or from inventory.",
        )
        .block(Block::default().title("Sort Plan").borders(Borders::ALL));
        f.render_widget(empty_p, area);
    }
}
