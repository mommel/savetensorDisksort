//! TUI View: Mountpoint scan status and triggers.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::config::DiskSortConfig;
use crate::persistence::Inventory;

pub fn render_scan_view(
    f: &mut Frame,
    area: Rect,
    config: &DiskSortConfig,
    inventory: Option<&Inventory>,
    is_scanning: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(10)])
        .split(area);

    let status_title = if is_scanning {
        "Scan Status: Scanning storage mountpoints..."
    } else if inventory.is_some() {
        "Scan Status: Inventory Loaded"
    } else {
        "Scan Status: Idle (Press [S] to start scan)"
    };

    let status_text = if let Some(inv) = inventory {
        vec![
            Line::from(vec![
                Span::raw("Last Scan Time: "),
                Span::styled(
                    inv.scan_timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::raw("Total Files Found: "),
                Span::styled(
                    inv.summary.total_files.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  |  Total Size: "),
                Span::styled(
                    &inv.summary.total_size_human,
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  |  Duplicate Candidates: "),
                Span::styled(
                    inv.summary.duplicate_candidates.to_string(),
                    Style::default().fg(Color::Magenta),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "Actions: Press [S] to Rescan  |  [Tab] to View Inventory  |  [P] to Plan Sort",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ]
    } else {
        vec![
            Line::from("No scan performed yet in this session."),
            Line::from("Configure mountpoints below and press [S] to initiate parallel discovery."),
        ]
    };

    let status_p = Paragraph::new(status_text)
        .block(
            Block::default()
                .title(status_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(status_p, chunks[0]);

    // Mountpoints list
    let mp_items: Vec<ListItem> = config
        .mountpoints
        .iter()
        .map(|mp| {
            let label = mp.label.as_deref().unwrap_or("No label");
            Line::from(vec![
                Span::styled("📁 ", Style::default().fg(Color::Yellow)),
                Span::styled(format!("{:<30}", mp.path), Style::default().fg(Color::White)),
                Span::styled(format!(" [{}]", label), Style::default().fg(Color::DarkGray)),
            ])
        })
        .map(ListItem::new)
        .collect();

    let mp_list = List::new(mp_items).block(
        Block::default()
            .title(format!("Configured Mountpoints ({})", config.mountpoints.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(mp_list, chunks[1]);
}
