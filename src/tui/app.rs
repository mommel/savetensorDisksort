//! Ratatui App state machine and event handling loop.

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::{Frame, Terminal};
use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::keybinds::{map_key, Action};
use super::views::{
    render_exec_view, render_inventory_view, render_plan_view, render_scan_view,
    render_summary_view,
};
use super::widgets::{flatten_folder_tree, TreeItemState};
use crate::config::DiskSortConfig;
use crate::discovery::{inspect_mountpoint, scan_all_mountpoints, SymlinkMapper};
use crate::executor::{execute_plan, ExecutionOptions};
use crate::persistence::{ExecutionLogger, Inventory};
use crate::planner::{PlanTemplate, SortPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
    Scan = 0,
    Inventory = 1,
    Plan = 2,
    Execute = 3,
    Summary = 4,
}

impl AppTab {
    pub fn next(self) -> Self {
        match self {
            AppTab::Scan => AppTab::Inventory,
            AppTab::Inventory => AppTab::Plan,
            AppTab::Plan => AppTab::Execute,
            AppTab::Execute => AppTab::Summary,
            AppTab::Summary => AppTab::Scan,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            AppTab::Scan => AppTab::Summary,
            AppTab::Inventory => AppTab::Scan,
            AppTab::Plan => AppTab::Inventory,
            AppTab::Execute => AppTab::Plan,
            AppTab::Summary => AppTab::Execute,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    AddingMountpoint,
}


pub enum BackgroundMsg {
    ScanFinished(Inventory),
    PlanFinished(SortPlan),
    ExecProgress {
        op_id: String,
        copied: u64,
        total: u64,
        percent: u16,
    },
    ExecLog(String),
    ExecFinished,
}

pub struct App {
    pub config: DiskSortConfig,
    pub current_tab: AppTab,
    pub inventory: Option<Inventory>,
    pub plan: Option<SortPlan>,
    pub is_scanning: bool,
    pub is_executing: bool,
    pub tree_state: TreeItemState,
    pub selected_op_idx: usize,
    pub current_op_id: String,
    pub current_copied: u64,
    pub current_total: u64,
    pub current_percent: u16,
    pub log_messages: Vec<String>,
    pub cancel_flag: Arc<AtomicBool>,
    pub input_mode: InputMode,
    pub new_mountpoint_input: String,
    pub selected_mountpoint_idx: usize,
    sender: Sender<BackgroundMsg>,
    receiver: Receiver<BackgroundMsg>,
}

impl App {
    pub fn new(config: DiskSortConfig) -> Self {
        let (sender, receiver) = channel();

        // Try auto-loading inventory and plan from output dir if present
        let inv_path = PathBuf::from(&config.output_dir).join("inventory.json");
        let inventory = Inventory::load_from_file(inv_path).ok();

        let plan_path = PathBuf::from(&config.output_dir).join("sort_plan.json");
        let plan = crate::persistence::load_plan_from_file(plan_path).ok();

        Self {
            config,
            current_tab: AppTab::Scan,
            inventory,
            plan,
            is_scanning: false,
            is_executing: false,
            tree_state: TreeItemState::new(),
            selected_op_idx: 0,
            current_op_id: String::new(),
            current_copied: 0,
            current_total: 0,
            current_percent: 0,
            log_messages: Vec::new(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            input_mode: InputMode::Normal,
            new_mountpoint_input: String::new(),
            selected_mountpoint_idx: 0,
            sender,
            receiver,
        }
    }

    pub fn start_scan(&mut self) {
        if self.is_scanning {
            return;
        }
        self.is_scanning = true;
        self.log_messages
            .push("Starting scan across all mountpoints...".into());

        let mps = self.config.mountpoint_paths();
        let app_roots = self.config.app_root_paths();
        let out_dir = self.config.output_dir.clone();
        let tx = self.sender.clone();

        thread::spawn(move || {
            let mut files = scan_all_mountpoints(&mps);
            let mut mapper = SymlinkMapper::new();
            mapper.scan_app_roots(&app_roots);
            mapper.cross_reference_inventory(&mut files);

            let mountpoint_infos = mps.iter().map(|p| inspect_mountpoint(p, None)).collect();

            let inv = Inventory::build(mountpoint_infos, files, mapper.symlink_tree);
            let inv_path = PathBuf::from(&out_dir).join("inventory.json");
            let _ = inv.save_to_file(inv_path);

            let _ = tx.send(BackgroundMsg::ScanFinished(inv));
        });
    }

    pub fn generate_default_plan(&mut self) {
        if let Some(inv) = &self.inventory {
            let target = if let Some(first_mp) = self.config.mountpoints.first() {
                first_mp.path.clone()
            } else {
                "./models_target".to_string()
            };

            let free_bytes = 1_000_000_000_000u64; // Default fallback or inspect
            let plan = SortPlan::generate(&inv.files, &target, free_bytes, PlanTemplate::ByType);

            let plan_path = PathBuf::from(&self.config.output_dir).join("sort_plan.json");
            let _ = crate::persistence::save_plan_to_file(plan_path, &plan);

            self.plan = Some(plan);
            self.log_messages.push("Generated default sort plan".into());
        }
    }

    pub fn start_execution(&mut self, dry_run: bool) {
        if self.is_executing || self.plan.is_none() {
            return;
        }

        self.is_executing = true;
        self.current_tab = AppTab::Execute;
        self.cancel_flag.store(false, Ordering::Relaxed);

        let mut plan = self.plan.clone().unwrap();
        let tx = self.sender.clone();
        let cancel = Arc::clone(&self.cancel_flag);
        let log_path = PathBuf::from(&self.config.output_dir).join("execution.jsonl");

        thread::spawn(move || {
            let logger = ExecutionLogger::new(log_path).ok();
            let tx_prog = tx.clone();

            let options = ExecutionOptions {
                dry_run,
                logger,
                cancel_flag: Some(cancel),
                progress_cb: move |op_id, copied, total| {
                    let pct = if total > 0 {
                        ((copied as f64 / total as f64) * 100.0) as u16
                    } else {
                        0
                    };
                    let _ = tx_prog.send(BackgroundMsg::ExecProgress {
                        op_id: op_id.to_string(),
                        copied,
                        total,
                        percent: pct,
                    });
                },
            };

            match execute_plan(&mut plan, options) {
                Ok(messages) => {
                    for m in messages {
                        let _ = tx.send(BackgroundMsg::ExecLog(m));
                    }
                    let _ = tx.send(BackgroundMsg::PlanFinished(plan));
                }
                Err(e) => {
                    let _ = tx.send(BackgroundMsg::ExecLog(format!("Execution failed: {}", e)));
                }
            }

            let _ = tx.send(BackgroundMsg::ExecFinished);
        });
    }

    pub fn handle_background_messages(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                BackgroundMsg::ScanFinished(inv) => {
                    self.is_scanning = false;
                    self.log_messages.push(format!(
                        "Scan complete: {} files discovered.",
                        inv.summary.total_files
                    ));
                    self.inventory = Some(inv);
                    self.generate_default_plan();
                }
                BackgroundMsg::PlanFinished(plan) => {
                    self.plan = Some(plan);
                }
                BackgroundMsg::ExecProgress {
                    op_id,
                    copied,
                    total,
                    percent,
                } => {
                    self.current_op_id = op_id;
                    self.current_copied = copied;
                    self.current_total = total;
                    self.current_percent = percent;
                }
                BackgroundMsg::ExecLog(msg) => {
                    self.log_messages.push(msg);
                }
                BackgroundMsg::ExecFinished => {
                    self.is_executing = false;
                    self.current_tab = AppTab::Summary;
                }
            }
        }
    }

    pub fn handle_action(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => return true,
            Action::NextTab => self.current_tab = self.current_tab.next(),
            Action::PrevTab => self.current_tab = self.current_tab.prev(),
            Action::Up => {
                if self.current_tab == AppTab::Inventory {
                    if self.tree_state.selected_index > 0 {
                        self.tree_state.selected_index -= 1;
                    }
                } else if self.current_tab == AppTab::Plan && self.selected_op_idx > 0 {
                    self.selected_op_idx -= 1;
                } else if self.current_tab == AppTab::Scan && self.selected_mountpoint_idx > 0 {
                    self.selected_mountpoint_idx -= 1;
                }
            }
            Action::Down => {
                if self.current_tab == AppTab::Inventory {
                    if let Some(inv) = &self.inventory {
                        let rows = flatten_folder_tree(&inv.folder_tree, &self.tree_state);
                        if self.tree_state.selected_index + 1 < rows.len() {
                            self.tree_state.selected_index += 1;
                        }
                    }
                } else if self.current_tab == AppTab::Plan {
                    if let Some(plan) = &self.plan {
                        if self.selected_op_idx + 1 < plan.operations.len() {
                            self.selected_op_idx += 1;
                        }
                    }
                } else if self.current_tab == AppTab::Scan {
                    if self.selected_mountpoint_idx + 1 < self.config.mountpoints.len() {
                        self.selected_mountpoint_idx += 1;
                    }
                }
            }
            Action::Select | Action::Right => {
                if self.current_tab == AppTab::Inventory {
                    if let Some(inv) = &self.inventory {
                        let rows = flatten_folder_tree(&inv.folder_tree, &self.tree_state);
                        if let Some(row) = rows.get(self.tree_state.selected_index) {
                            if row.is_dir {
                                self.tree_state.toggle_expand(&row.id);
                            }
                        }
                    }
                }
            }
            Action::Left => {
                if self.current_tab == AppTab::Inventory {
                    if let Some(inv) = &self.inventory {
                        let rows = flatten_folder_tree(&inv.folder_tree, &self.tree_state);
                        if let Some(row) = rows.get(self.tree_state.selected_index) {
                            if row.is_dir && row.is_expanded {
                                self.tree_state.toggle_expand(&row.id);
                            }
                        }
                    }
                }
            }
            Action::ToggleCheckbox => {
                if self.current_tab == AppTab::Plan {
                    if let Some(plan) = &mut self.plan {
                        if let Some(op) = plan.operations.get_mut(self.selected_op_idx) {
                            op.selected = !op.selected;
                            plan.space_analysis = crate::planner::calculate_space_analysis(
                                &plan.operations,
                                plan.space_analysis.target_drive_free_before,
                            );
                        }
                    }
                }
            }
            Action::SelectAll => {
                if self.current_tab == AppTab::Plan {
                    if let Some(plan) = &mut self.plan {
                        let any_unselected = plan.operations.iter().any(|op| !op.selected);
                        for op in &mut plan.operations {
                            op.selected = any_unselected;
                        }
                        plan.space_analysis = crate::planner::calculate_space_analysis(
                            &plan.operations,
                            plan.space_analysis.target_drive_free_before,
                        );
                    }
                }
            }
            Action::Refresh => {
                self.start_scan();
            }
            Action::DryRun => {
                self.start_execution(true);
            }
            Action::Execute => {
                self.start_execution(false);
            }
            Action::AddMountpoint => {
                if self.current_tab == AppTab::Scan {
                    self.input_mode = InputMode::AddingMountpoint;
                    self.new_mountpoint_input.clear();
                }
            }
            Action::DeleteMountpoint => {
                if self.current_tab == AppTab::Scan {
                    if self.selected_mountpoint_idx < self.config.mountpoints.len() {
                        self.config.mountpoints.remove(self.selected_mountpoint_idx);
                        if self.selected_mountpoint_idx > 0 && self.selected_mountpoint_idx >= self.config.mountpoints.len() {
                            self.selected_mountpoint_idx -= 1;
                        }
                        let config_path = std::path::PathBuf::from("disksort.json");
                        let _ = self.config.save_to_file(config_path);
                    }
                }
            }
            _ => {}
        }
        false
    }
}

pub fn run_tui(config: DiskSortConfig) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);

    loop {
        app.handle_background_messages();

        terminal.draw(|f| {
            render_app(f, &app);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if app.input_mode == InputMode::AddingMountpoint {
                    match key.code {
                        crossterm::event::KeyCode::Enter => {
                            if !app.new_mountpoint_input.is_empty() {
                                app.config.mountpoints.push(crate::config::ConfigMountpoint {
                                    path: app.new_mountpoint_input.clone(),
                                    label: None,
                                });
                                let config_path = std::path::PathBuf::from("disksort.json");
                                let _ = app.config.save_to_file(config_path);
                            }
                            app.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Backspace => {
                            app.new_mountpoint_input.pop();
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            app.new_mountpoint_input.push(c);
                        }
                        _ => {}
                    }
                } else {
                    let action = map_key(key.code);
                    let should_quit = app.handle_action(action);
                    if should_quit {
                        break;
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn render_app(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Top: Tab navigation
    let titles: Vec<Line> = vec![
        Line::from(" 1. Scan "),
        Line::from(" 2. Inventory "),
        Line::from(" 3. Plan "),
        Line::from(" 4. Execute "),
        Line::from(" 5. Summary "),
    ];

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title(" SaveTensor DiskSort v0.1.0 ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .select(app.current_tab as usize)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, chunks[0]);

    // Center: Active View
    match app.current_tab {
        AppTab::Scan => {
            render_scan_view(
                f,
                chunks[1],
                &app.config,
                app.inventory.as_ref(),
                app.is_scanning,
                app.input_mode,
                &app.new_mountpoint_input,
                app.selected_mountpoint_idx,
            );
        }
        AppTab::Inventory => {
            let selected_file = if let Some(inv) = &app.inventory {
                let rows = flatten_folder_tree(&inv.folder_tree, &app.tree_state);
                rows.get(app.tree_state.selected_index)
                    .and_then(|r| inv.files.iter().find(|f| f.real_path == r.id))
            } else {
                None
            };

            render_inventory_view(
                f,
                chunks[1],
                app.inventory.as_ref(),
                &app.tree_state,
                selected_file,
            );
        }
        AppTab::Plan => {
            render_plan_view(f, chunks[1], app.plan.as_ref(), app.selected_op_idx);
        }
        AppTab::Execute => {
            render_exec_view(
                f,
                chunks[1],
                &app.current_op_id,
                "",
                app.current_copied,
                app.current_total,
                app.current_percent,
                &app.log_messages,
            );
        }
        AppTab::Summary => {
            render_summary_view(f, chunks[1], app.plan.as_ref());
        }
    }

    // Bottom: Status line and key shortcuts
    let status_text = Line::from(vec![
        Span::styled(
            " [Tab] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Next Tab  "),
        Span::styled(
            " [Space] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Toggle  "),
        Span::styled(
            " [D] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Dry-Run  "),
        Span::styled(
            " [X] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Execute  "),
        Span::styled(
            " [R] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Rescan  "),
        Span::styled(
            " [Q] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Quit"),
    ]);

    let status_p = Paragraph::new(status_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    f.render_widget(status_p, chunks[2]);
}
