//! Keyboard shortcuts and input mapping for the TUI.

use crossterm::event::KeyCode;

/// High-level actions triggered by key events in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextTab,
    PrevTab,
    Up,
    Down,
    Left,
    Right,
    Select,
    ToggleCheckbox,
    SelectAll,
    Search,
    DryRun,
    Execute,
    Refresh,
    Help,
    None,
}

pub fn map_key(code: KeyCode) -> Action {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => Action::Quit,
        KeyCode::Tab => Action::NextTab,
        KeyCode::BackTab => Action::PrevTab,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Left | KeyCode::Char('h') => Action::Left,
        KeyCode::Right | KeyCode::Char('l') => Action::Right,
        KeyCode::Enter => Action::Select,
        KeyCode::Char(' ') => Action::ToggleCheckbox,
        KeyCode::Char('a') | KeyCode::Char('A') => Action::SelectAll,
        KeyCode::Char('/') => Action::Search,
        KeyCode::Char('d') | KeyCode::Char('D') => Action::DryRun,
        KeyCode::Char('x') | KeyCode::Char('e') | KeyCode::Char('E') => Action::Execute,
        KeyCode::Char('r') | KeyCode::Char('R') => Action::Refresh,
        KeyCode::Char('?') => Action::Help,
        _ => Action::None,
    }
}
