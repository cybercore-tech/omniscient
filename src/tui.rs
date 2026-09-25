use crate::elevation;
use crate::health::{self, HealthReport};
use crate::modules;
use crate::pathcheck;
use crate::paths;
use crate::report;
use crate::snapshot;
use anyhow::{Context, Result};
use chrono::Local;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};
use std::{
    collections::VecDeque,
    io::{self, IsTerminal, Stdout},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

const LOG_LIMIT: usize = 80;

#[derive(Clone, Copy)]
struct UiPalette {
    unicode: bool,
    bg: Color,
    panel: Color,
    line: Color,
    muted: Color,
    white: Color,
    cyan: Color,
    pink: Color,
    acid: Color,
    purple: Color,
    orange: Color,
    red: Color,
}

impl UiPalette {
    fn detect() -> Self {
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok())
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let unicode = locale.contains("utf-8") || locale.contains("utf8");
        let true_color = std::env::var("COLORTERM")
            .map(|value| value.contains("truecolor") || value.contains("24bit"))
            .unwrap_or(false);
        if true_color {
            Self {
                unicode,
                bg: Color::Rgb(6, 8, 15),
                panel: Color::Rgb(12, 17, 32),
                line: Color::Rgb(32, 48, 92),
                muted: Color::Rgb(139, 151, 201),
                white: Color::Rgb(230, 238, 255),
                cyan: Color::Rgb(34, 230, 255),
                pink: Color::Rgb(255, 45, 158),
                acid: Color::Rgb(198, 233, 103),
                purple: Color::Rgb(181, 123, 255),
                orange: Color::Rgb(255, 145, 66),
                red: Color::Rgb(255, 59, 82),
            }
        } else {
            Self {
                unicode,
                bg: Color::Black,
                panel: Color::DarkGray,
                line: Color::Gray,
                muted: Color::Gray,
                white: Color::White,
                cyan: Color::Cyan,
                pink: Color::Magenta,
                acid: Color::Green,
                purple: Color::Blue,
                orange: Color::Yellow,
                red: Color::Red,
            }
        }
    }

    fn border(self) -> Style {
        Style::default().fg(self.line)
    }

    fn block(self, title: &str, accent: Color) -> Block<'static> {
        Block::default()
            .title(Span::styled(
                format!(" {title} "),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(self.border())
            .style(Style::default().bg(self.panel))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModuleState {
    Pending,
    Queued,
    Running,
    Complete,
    Failed,
}

impl ModuleState {
    fn label(self) -> &'static str {
        match self {
            Self::Pending => "READY",
            Self::Queued => "QUEUED",
            Self::Running => "RUNNING",
            Self::Complete => "DONE",
            Self::Failed => "FAILED",
        }
    }
}

struct ModuleView {
    name: &'static str,
    slug: &'static str,
    menu_label: &'static str,
    tools: &'static [&'static str],
    optional_tools: &'static [&'static str],
    requires_sudo: bool,
    selected: bool,
    state: ModuleState,
}

enum WorkerMessage {
    Health(HealthReport),
    Log(String),
    Started(usize),
    Completed { index: usize, report_path: String },
    Failed { index: usize, error: String },
    Finished { summary_path: String },
}

struct App {
    modules: Vec<ModuleView>,
    cursor: usize,
    health: Option<HealthReport>,
    logs: VecDeque<String>,
    report_paths: Vec<String>,
    summary_path: Option<String>,
    running: bool,
    finished: bool,
    show_details: bool,
    error: Option<String>,
    tick: u64,
}

impl App {
    fn new() -> Self {
        let modules = modules::all_modules()
            .iter()
            .map(|module| ModuleView {
                name: module.name(),
                slug: module.slug(),
                menu_label: module.menu_label(),
                tools: module.tools(),
                optional_tools: module.optional_tools(),
                requires_sudo: module.requires_sudo(),
                selected: false,
                state: ModuleState::Pending,
            })
            .collect();
        Self {
            modules,
            cursor: 0,
            health: None,
            logs: VecDeque::new(),
            report_paths: Vec::new(),
            summary_path: None,
            running: false,
            finished: false,
            show_details: false,
            error: None,
            tick: 0,
        }
    }

    fn selected_indices(&self) -> Vec<usize> {
        self.modules
            .iter()
            .enumerate()
            .filter_map(|(index, module)| module.selected.then_some(index))
            .collect()
    }

    fn selected_count(&self) -> usize {
        self.modules.iter().filter(|module| module.selected).count()
    }

    fn log(&mut self, message: impl Into<String>) {
        if self.logs.len() >= LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(message.into());
    }

    fn reset(&mut self) {
        for module in &mut self.modules {
            module.selected = false;
            module.state = ModuleState::Pending;
        }
        self.health = None;
        self.logs.clear();
        self.report_paths.clear();
        self.summary_path = None;
        self.running = false;
        self.finished = false;
        self.error = None;
        self.log("READY / select modules and press ENTER to begin");
    }
}

pub fn run() -> Result<()> {
    if !io::stdout().is_terminal() {
        anyhow::bail!("omniscient requires an interactive terminal");
    }

    let palette = UiPalette::detect();
    let mut stdout = io::stdout();
    enable_raw_mode().context("enabling raw terminal mode")?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("entering the omniscient terminal screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("creating the terminal renderer")?;
    terminal.clear().context("clearing the terminal renderer")?;

    let result = run_loop(&mut terminal, palette);
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, palette: UiPalette) -> Result<()> {
    let mut app = App::new();
    app.log("READY / select modules and press ENTER to begin");
    publish_snapshot(&app);
    let mut receiver: Option<Receiver<WorkerMessage>> = None;

    loop {
        terminal.draw(|frame| draw(frame, &app, palette))?;

        if let Some(rx) = &receiver {
            let messages: Vec<_> = rx.try_iter().collect();
            if !messages.is_empty() {
                for message in messages {
                    handle_message(&mut app, message);
                }
                publish_snapshot(&app);
            }
            if app.finished || app.error.is_some() {
                receiver = None;
            }
        }

        if event::poll(Duration::from_millis(100)).context("polling terminal input")? {
            if let Event::Key(key) = event::read().context("reading terminal input")? {
                let should_quit = handle_key(terminal, &mut app, key, &mut receiver)?;
                publish_snapshot(&app);
                if should_quit {
                    break;
                }
            }
        }
        app.tick = app.tick.wrapping_add(1);
    }

    Ok(())
}

fn handle_key(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    key: KeyEvent,
    receiver: &mut Option<Receiver<WorkerMessage>>,
) -> Result<bool> {
    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
        if app.running {
            app.log("RUNNING / finish the current audit before quitting");
            return Ok(false);
        }
        return Ok(true);
    }

    if app.running {
        if key.code == KeyCode::Tab {
            app.show_details = !app.show_details;
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.cursor = app.cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.cursor = (app.cursor + 1).min(app.modules.len().saturating_sub(1));
        }
        KeyCode::Char(' ') => {
            if let Some(module) = app.modules.get_mut(app.cursor) {
                module.selected = !module.selected;
                module.state = if module.selected {
                    ModuleState::Queued
                } else {
                    ModuleState::Pending
                };
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let select = app.selected_count() != app.modules.len();
            for module in &mut app.modules {
                module.selected = select;
                module.state = if select {
                    ModuleState::Queued
                } else {
                    ModuleState::Pending
                };
            }
            app.log(if select {
                "ALL MODULES QUEUED"
            } else {
                "SELECTION CLEARED"
            });
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.reset();
        }
        KeyCode::Tab => {
            app.show_details = !app.show_details;
        }
        KeyCode::Enter => {
            start_run(terminal, app, receiver)?;
        }
        _ => {}
    }
    Ok(false)
}

fn start_run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    receiver: &mut Option<Receiver<WorkerMessage>>,
) -> Result<()> {
    let selected = app.selected_indices();
    if selected.is_empty() {
        app.log("SELECT AT LEAST ONE MODULE");
        return Ok(());
    }

    let elevated = app
        .modules
        .iter()
        .enumerate()
        .any(|(index, module)| selected.contains(&index) && module.requires_sudo);

    if elevated && !elevation::uses_graphical() && !suspend_for_sudo(terminal)? {
        app.log("SUDO / authorization canceled; audit not started");
        return Ok(());
    }

    let full = selected.len() == app.modules.len();
    for (index, module) in app.modules.iter_mut().enumerate() {
        if selected.contains(&index) {
            module.state = ModuleState::Queued;
        }
    }
    app.running = true;
    app.finished = false;
    app.error = None;
    app.health = None;
    app.report_paths.clear();
    app.summary_path = None;
    app.logs.clear();
    if elevated {
        app.log(format!(
            "{} / authorization completed for elevated modules",
            elevation::label()
        ));
    }
    app.log(if full {
        "FULL SYSTEM AUDIT QUEUED"
    } else {
        "SELECTED MODULES QUEUED"
    });
    *receiver = Some(spawn_worker(selected, full));
    Ok(())
}

fn suspend_for_sudo(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<bool> {
    disable_raw_mode().context("pausing terminal input for sudo")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("leaving the dashboard for sudo")?;

    let result = Command::new("sudo").arg("-v").status();

    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )
    .context("returning to the dashboard after sudo")?;
    enable_raw_mode().context("restoring terminal input after sudo")?;
    terminal
        .clear()
        .context("clearing the dashboard after sudo")?;

    let result = result.context("starting sudo authorization")?;
    Ok(result.success())
}

fn spawn_worker(selected: Vec<usize>, full: bool) -> Receiver<WorkerMessage> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || worker(selected, full, tx));
    rx
}

fn worker(selected: Vec<usize>, full: bool, tx: Sender<WorkerMessage>) {
    let modules = modules::all_modules();
    let health = health::compute_selected(&modules, &selected);
    let _ = tx.send(WorkerMessage::Health(health.clone()));
    let _ = tx.send(WorkerMessage::Log(format!(
        "HEALTH / baseline score {}/100",
        health.score
    )));

    let base_dir = paths::report_root();
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let root = if full {
        base_dir.join(format!("full_system_audit-{timestamp}"))
    } else {
        base_dir.clone()
    };
    if let Err(error) = std::fs::create_dir_all(&root) {
        let _ = tx.send(WorkerMessage::Failed {
            index: usize::MAX,
            error: format!("creating report directory: {error}"),
        });
        return;
    }

    let mut reports: Vec<(String, String)> = Vec::new();
    for (position, index) in selected.iter().copied().enumerate() {
        let Some(module) = modules.get(index) else {
            continue;
        };
        let _ = tx.send(WorkerMessage::Started(index));
        let _ = tx.send(WorkerMessage::Log(format!(
            "[{}/{}] SCANNING {}",
            position + 1,
            selected.len(),
            module.name().to_uppercase()
        )));
        let dir = root.join(format!("{}-{timestamp}", module.slug()));
        let result = std::fs::create_dir_all(&dir)
            .and_then(|_| module.run(&dir).map_err(std::io::Error::other));
        match result {
            Ok(()) => {
                let relative = format!("{}-{}/{}.md", module.slug(), timestamp, module.slug());
                let absolute = dir.join(format!("{}.md", module.slug()));
                reports.push((module.name().to_string(), relative.clone()));
                let _ = tx.send(WorkerMessage::Completed {
                    index,
                    report_path: absolute.display().to_string(),
                });
                let _ = tx.send(WorkerMessage::Log(format!(
                    "COMPLETE / {}",
                    absolute.display()
                )));
            }
            Err(error) => {
                let _ = tx.send(WorkerMessage::Failed {
                    index,
                    error: error.to_string(),
                });
            }
        }
    }

    let report_refs = reports
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_str()))
        .collect::<Vec<_>>();
    match report::write_summary(&root, &timestamp, &health, &report_refs) {
        Ok(()) => {
            let summary_path = root.join("SUMMARY.md");
            let _ = tx.send(WorkerMessage::Finished {
                summary_path: summary_path.display().to_string(),
            });
        }
        Err(error) => {
            let _ = tx.send(WorkerMessage::Failed {
                index: usize::MAX,
                error: format!("writing summary: {error}"),
            });
        }
    }
}

fn handle_message(app: &mut App, message: WorkerMessage) {
    match message {
        WorkerMessage::Health(health) => app.health = Some(health),
        WorkerMessage::Log(message) => app.log(message),
        WorkerMessage::Started(index) => {
            if let Some(module) = app.modules.get_mut(index) {
                module.state = ModuleState::Running;
            }
        }
        WorkerMessage::Completed { index, report_path } => {
            if let Some(module) = app.modules.get_mut(index) {
                module.state = ModuleState::Complete;
            }
            app.report_paths.push(report_path);
        }
        WorkerMessage::Failed { index, error } => {
            if index == usize::MAX {
                app.running = false;
                app.error = Some(error.clone());
                app.log(format!("FATAL / {error}"));
            } else {
                if let Some(module) = app.modules.get_mut(index) {
                    module.state = ModuleState::Failed;
                }
                app.log(format!("FAILED / {error}"));
            }
        }
        WorkerMessage::Finished { summary_path } => {
            app.running = false;
            app.finished = true;
            app.summary_path = Some(summary_path.clone());
            app.log(format!("AUDIT COMPLETE / {summary_path}"));
        }
    }
}

fn publish_snapshot(app: &App) {
    let state = if app.error.is_some() {
        "error"
    } else if app.running {
        "running"
    } else if app.finished {
        "complete"
    } else {
        "ready"
    };
    let modules = app
        .modules
        .iter()
        .map(|module| snapshot::ModuleSnapshot {
            name: module.name.to_string(),
            slug: module.slug.to_string(),
            state: module.state.label().to_ascii_lowercase(),
            selected: module.selected,
            requires_sudo: module.requires_sudo,
        })
        .collect::<Vec<_>>();
    let completed_count = app
        .modules
        .iter()
        .filter(|module| module.selected && module.state == ModuleState::Complete)
        .count();
    let health = app.health.as_ref().map(|health| snapshot::HealthSnapshot {
        score: health.score,
        notes: health.notes.clone(),
    });
    let state = snapshot::AuditSnapshot {
        schema_version: 1,
        application: "omniscient",
        state: state.to_string(),
        updated_at: Local::now().to_rfc3339(),
        host: hostname(),
        selected_count: app.selected_count(),
        completed_count,
        health,
        modules,
        reports: app.report_paths.clone(),
        suggestions: vec![],
        suggestions_path: None,
        summary_path: app.summary_path.clone(),
        error: app.error.clone(),
        message: app.logs.back().cloned().unwrap_or_default(),
    };
    // Snapshot output is an integration surface, not a reason to interrupt
    // an interactive audit if a HUD path becomes unavailable.
    let _ = snapshot::write(&state);
}

fn draw(frame: &mut Frame, app: &App, palette: UiPalette) {
    let area = frame.size();
    frame.render_widget(
        Block::default().style(Style::default().bg(palette.bg)),
        area,
    );
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);
    draw_header(frame, vertical[0], app, palette);
    if area.width >= 110 {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(29),
                Constraint::Percentage(46),
                Constraint::Percentage(25),
            ])
            .split(vertical[1]);
        draw_modules(frame, body[0], app, palette);
        draw_live(frame, body[1], app, palette);
        draw_sidebar(frame, body[2], app, palette);
    } else {
        let body = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(8),
                Constraint::Length(8),
            ])
            .split(vertical[1]);
        draw_modules(frame, body[0], app, palette);
        draw_live(frame, body[1], app, palette);
        draw_sidebar(frame, body[2], app, palette);
    }
    draw_footer(frame, vertical[2], app, palette);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let header = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Length(2)])
        .split(area);
    let status = if app.running {
        (
            glyph(palette, "●", "*").to_owned() + " LIVE SCAN",
            palette.pink,
        )
    } else if app.finished {
        (
            glyph(palette, "●", "*").to_owned() + " COMPLETE",
            palette.acid,
        )
    } else {
        (glyph(palette, "●", "*").to_owned() + " READY", palette.cyan)
    };
    let score = app
        .health
        .as_ref()
        .map(|health| format!("{}/100", health.score))
        .unwrap_or_else(|| "—/100".to_string());
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", glyph(palette, "⟦◈⟧", "[*]")),
                Style::default().fg(palette.pink),
            ),
            Span::styled(
                "OMNISCIENT",
                Style::default()
                    .fg(palette.white)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " // CYBERDECK SYSTEM AUDIT",
                Style::default().fg(palette.cyan),
            ),
            Span::raw("  "),
            Span::styled(status.0, Style::default().fg(status.1)),
        ]),
        Line::from(vec![
            Span::styled(
                "FULL SYSTEM REALITY SCANNER",
                Style::default().fg(palette.purple),
            ),
            Span::styled("    HEALTH ", Style::default().fg(palette.muted)),
            Span::styled(
                score,
                Style::default()
                    .fg(palette.acid)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    MODULES ", Style::default().fg(palette.muted)),
            Span::styled(
                format!("{}/{}", app.selected_count(), app.modules.len()),
                Style::default().fg(palette.cyan),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(palette.block(" CYBERCORE // OMNISCIENT v3.0 ", palette.pink))
            .style(Style::default().bg(palette.panel))
            .wrap(Wrap { trim: false }),
        header[0],
    );

    let selected = app.selected_count();
    let completed = app
        .modules
        .iter()
        .filter(|module| module.selected && module.state == ModuleState::Complete)
        .count();
    let ratio = if selected == 0 {
        0.0
    } else {
        completed as f64 / selected as f64
    };
    let progress_width = header[1].width.saturating_sub(4) as usize;
    let filled = (progress_width as f64 * ratio).round() as usize;
    let (full, empty) = if palette.unicode {
        ("█", "░")
    } else {
        ("#", "-")
    };
    let progress = format!(
        "{}{}  {completed}/{selected}",
        full.repeat(filled),
        empty.repeat(progress_width.saturating_sub(filled))
    );
    let label = if app.running {
        "SCAN PROGRESS"
    } else if app.finished {
        "AUDIT COMPLETE"
    } else {
        "READY"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {label} "), Style::default().fg(palette.muted)),
            Span::styled(progress, Style::default().fg(palette.cyan)),
        ]))
        .style(Style::default().bg(palette.panel)),
        header[1],
    );
}

fn draw_modules(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let items = app
        .modules
        .iter()
        .map(|module| {
            let marker = if module.selected {
                glyph(palette, "◉", "[x]")
            } else {
                glyph(palette, "○", "[ ]")
            };
            let (state_color, state_marker) = match module.state {
                ModuleState::Pending => (palette.muted, glyph(palette, "·", ".")),
                ModuleState::Queued => (palette.orange, glyph(palette, "›", ">")),
                ModuleState::Running => (palette.pink, glyph(palette, "◆", "#")),
                ModuleState::Complete => (palette.acid, glyph(palette, "✓", "+")),
                ModuleState::Failed => (palette.red, glyph(palette, "×", "!")),
            };
            let label = if palette.unicode {
                module.menu_label
            } else {
                module.name
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().fg(if module.selected {
                        palette.cyan
                    } else {
                        palette.muted
                    }),
                ),
                Span::styled(format!("{label:<24}"), Style::default().fg(palette.white)),
                Span::styled(
                    format!(" {state_marker} {}", module.state.label()),
                    Style::default().fg(state_color),
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(Some(app.cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(palette.block(" MODULES // SPACE TO SELECT ", palette.cyan))
            .highlight_style(
                Style::default()
                    .bg(palette.line)
                    .fg(palette.white)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▌"),
        area,
        &mut state,
    );
}

fn draw_live(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let mut lines = app
        .logs
        .iter()
        .map(|line| {
            let color = if line.contains("FAILED") {
                palette.red
            } else if line.contains("COMPLETE") || line.contains("READY") {
                palette.acid
            } else if line.contains("SCANNING") || line.contains("QUEUED") {
                palette.pink
            } else {
                palette.muted
            };
            Line::from(Span::styled(
                format!("{} {line}", glyph(palette, "›", ">")),
                Style::default().fg(color),
            ))
        })
        .collect::<Vec<_>>();
    if app.running {
        let pulse = if palette.unicode {
            ["·", "·", ":", "∙", "•", "∙", ":", "·"][app.tick as usize % 8]
        } else {
            [".", ".", ":", "+", "*", "+", ":", "."][app.tick as usize % 8]
        };
        lines.push(Line::from(Span::styled(
            format!("  SCAN PULSE {pulse}"),
            Style::default().fg(palette.cyan),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(palette.block(" LIVE SCAN OUTPUT ", palette.pink))
            .style(Style::default().bg(palette.panel))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(5)])
        .split(area);
    let identity = vec![
        Line::from(Span::styled("HOST / ", Style::default().fg(palette.muted))),
        Line::from(Span::styled(
            hostname(),
            Style::default()
                .fg(palette.white)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "USER / {}",
                std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
            ),
            Style::default().fg(palette.cyan),
        )),
        Line::from(Span::styled(
            if app.show_details {
                "TAB / CAPABILITY MATRIX"
            } else {
                "TAB / MODULE DETAILS"
            },
            Style::default().fg(palette.purple),
        )),
    ];
    frame.render_widget(
        Paragraph::new(identity)
            .block(palette.block(" SYSTEM IDENTITY ", palette.acid))
            .style(Style::default().bg(palette.panel)),
        vertical[0],
    );
    if app.show_details {
        draw_details(frame, vertical[1], app, palette);
    } else {
        draw_capabilities(frame, vertical[1], app, palette);
    }
}

fn draw_capabilities(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let mut tools = Vec::new();
    for module in &app.modules {
        for tool in module.tools {
            if !tools.contains(tool) {
                tools.push(*tool);
            }
        }
    }
    let rows = tools.iter().map(|tool| {
        let present = pathcheck::exists(tool);
        let privileged = app.modules.iter().any(|module| {
            module.requires_sudo && module.tools.iter().any(|candidate| candidate == tool)
        });
        let optional = app.modules.iter().any(|module| {
            module
                .optional_tools
                .iter()
                .any(|candidate| candidate == tool)
        });
        let (marker, state, color) = if !present && optional {
            (glyph(palette, "◇", "~"), "OPTIONAL", palette.orange)
        } else if !present {
            (glyph(palette, "○", "-"), "MISSING", palette.red)
        } else if privileged {
            (glyph(palette, "◆", "!"), "PRIVILEGED", palette.orange)
        } else {
            (glyph(palette, "●", "+"), "AVAILABLE", palette.acid)
        };
        Row::new(vec![
            Cell::from(marker),
            Cell::from(*tool),
            Cell::from(state),
        ])
        .style(Style::default().fg(color))
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Length(14),
                Constraint::Min(10),
            ],
        )
        .header(
            Row::new(vec!["", "TOOL", "STATE"])
                .style(Style::default().fg(palette.muted))
                .bottom_margin(0),
        )
        .block(palette.block(" CAPABILITY MATRIX ", palette.orange))
        .column_spacing(1),
        area,
    );
}

fn draw_details(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let Some(module) = app.modules.get(app.cursor) else {
        return;
    };
    let tools = if module.tools.is_empty() {
        "none".to_string()
    } else {
        module.tools.join("  ")
    };
    let lines = vec![
        Line::from(Span::styled(
            module.name,
            Style::default()
                .fg(palette.white)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("SLUG / {}", module.slug),
            Style::default().fg(palette.cyan),
        )),
        Line::from(Span::styled(
            format!("TOOLS / {tools}"),
            Style::default().fg(palette.muted),
        )),
        Line::from(Span::styled(
            if module.requires_sudo {
                "PRIVILEGE / ELEVATED AUTH REQUIRED"
            } else {
                "PRIVILEGE / USER MODE"
            },
            Style::default().fg(if module.requires_sudo {
                palette.orange
            } else {
                palette.acid
            }),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(palette.block(" MODULE DETAILS ", palette.purple))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, palette: UiPalette) {
    let summary = app
        .summary_path
        .as_deref()
        .map(|path| format!("  SUMMARY {path}"))
        .unwrap_or_default();
    let line = if app.running {
        " ↑↓ MOVE   TAB DETAILS   Q LOCKED WHILE RUNNING"
    } else {
        " ↑↓ MOVE   SPACE SELECT   A ALL   ENTER RUN   TAB DETAILS   R RESET   Q QUIT"
    };
    let report_line = if app.finished {
        format!(
            "REPORTS {}  {}",
            app.report_paths.len(),
            summary.trim_start()
        )
    } else if let Some(error) = &app.error {
        format!("ERROR {error}")
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(line, Style::default().fg(palette.cyan))),
            Line::from(Span::styled(
                report_line,
                Style::default().fg(palette.muted),
            )),
        ])
        .style(Style::default().bg(palette.bg)),
        area,
    );
}

fn glyph(palette: UiPalette, unicode: &'static str, ascii: &'static str) -> &'static str {
    if palette.unicode {
        unicode
    } else {
        ascii
    }
}

fn hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_string())
}
