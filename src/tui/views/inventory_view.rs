//! TUI View: Discovered inventory browser and file details panel.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::discovery::FileInfo;
use crate::persistence::Inventory;
use crate::tui::widgets::{flatten_folder_tree, render_folder_tree, TreeItemState};

pub fn render_inventory_view(
    f: &mut Frame,
    area: Rect,
    inventory: Option<&Inventory>,
    tree_state: &TreeItemState,
    selected_file: Option<&FileInfo>,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    if let Some(inv) = inventory {
        // Left: Folder tree
        let rows = flatten_folder_tree(&inv.folder_tree, tree_state);
        let tree_lines = render_folder_tree(&rows, tree_state.selected_index);
        let items: Vec<ListItem> = tree_lines.into_iter().map(ListItem::new).collect();

        let tree_list = List::new(items).block(
            Block::default()
                .title("Folder Tree (Enter/→: Expand, ←: Collapse)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        f.render_widget(tree_list, chunks[0]);

        // Right: Details Panel
        let details_lines = if let Some(file) = selected_file {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("File ID: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&file.id, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("Filename: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&file.filename, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("Category: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(file.category.as_str(), Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::styled("Size: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&file.size_human, Style::default().fg(Color::Yellow)),
                    Span::raw(format!(" ({} bytes)", file.size_bytes)),
                ]),
                Line::from(vec![
                    Span::styled("Physical Path: ", Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(vec![
                    Span::styled(format!("  {}", file.real_path), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Mountpoint: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&file.mountpoint, Style::default().fg(Color::Cyan)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        format!("Symlinked From ({} applications):", file.symlinked_from.len()),
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];

            if file.symlinked_from.is_empty() {
                lines.push(Line::from(Span::styled("  (No application symlinks mapped)", Style::default().fg(Color::DarkGray))));
            } else {
                for link in &file.symlinked_from {
                    lines.push(Line::from(vec![
                        Span::styled("  🔗 ", Style::default().fg(Color::Magenta)),
                        Span::styled(link, Style::default().fg(Color::White)),
                    ]));
                }
            }

            lines
        } else {
            vec![
                Line::from("Navigate the folder tree on the left to inspect detailed file metadata and symlink mappings."),
            ]
        };

        let details_p = Paragraph::new(details_lines)
            .block(
                Block::default()
                    .title("Details Panel")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(details_p, chunks[1]);
    } else {
        let empty_p = Paragraph::new("No inventory data loaded. Go to the Scan tab to discover files.")
            .block(
                Block::default()
                    .title("Inventory")
                    .borders(Borders::ALL),
            );
        f.render_widget(empty_p, area);
    }
}
