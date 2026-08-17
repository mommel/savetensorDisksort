//! Hierarchical collapsible folder tree widget for Ratatui.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::{BTreeMap, HashSet};

use crate::accounting::FolderNode;

/// Represents a flattened row in the visible tree view.
#[derive(Debug, Clone)]
pub struct TreeRow {
    pub id: String,
    pub display_name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub size_human: String,
    pub file_count: usize,
}

#[derive(Debug, Default)]
pub struct TreeItemState {
    pub expanded_paths: HashSet<String>,
    pub selected_index: usize,
}

impl TreeItemState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_expand(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            self.expanded_paths.remove(path);
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }
}

/// Flatten a nested FolderTree into a list of visible `TreeRow`s based on expansion state.
pub fn flatten_folder_tree(
    tree: &BTreeMap<String, FolderNode>,
    state: &TreeItemState,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();

    for (mp_name, root_node) in tree {
        flatten_node(mp_name, mp_name, root_node, 0, state, &mut rows);
    }

    rows
}

fn flatten_node(
    name: &str,
    full_path: &str,
    node: &FolderNode,
    depth: usize,
    state: &TreeItemState,
    out: &mut Vec<TreeRow>,
) {
    let is_dir = !node.children.is_empty();
    let is_expanded = state.expanded_paths.contains(full_path);

    out.push(TreeRow {
        id: full_path.to_string(),
        display_name: name.to_string(),
        depth,
        is_dir,
        is_expanded,
        size_human: node.size_human.clone(),
        file_count: node.file_count,
    });

    if is_dir && is_expanded {
        for (child_name, child_node) in &node.children {
            let child_path = format!("{}/{}", full_path, child_name);
            flatten_node(child_name, &child_path, child_node, depth + 1, state, out);
        }
    }
}

/// Render flattened rows to Ratatui `Line` spans.
pub fn render_folder_tree<'a>(rows: &'a [TreeRow], selected_idx: usize) -> Vec<Line<'a>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let is_selected = i == selected_idx;
            let indent = "  ".repeat(row.depth);

            let icon = if row.is_dir {
                if row.is_expanded {
                    "▼ "
                } else {
                    "▶ "
                }
            } else {
                "• "
            };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::Rgb(30, 40, 60))
            } else if row.is_dir {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let size_style = Style::default().fg(Color::DarkGray);

            let main_text = format!("{}{}{}", indent, icon, row.display_name);
            let size_text = format!(" ({})", row.size_human);

            Line::from(vec![
                Span::styled(main_text, style),
                Span::styled(size_text, size_style),
            ])
        })
        .collect()
}
