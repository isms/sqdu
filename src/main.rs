mod app;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{env, io};

use app::{analyze_database, analyze_indexes, analyze_table_details, App, ViewMode};
use ui::ui;

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    let mut prev_view_mode: Option<String> = None;
    let mut prev_scroll_offset: u16 = 0;

    loop {
        // Clear terminal buffer when switching view modes or scrolling to prevent artifacts
        let current_view = format!("{:?}", std::mem::discriminant(&app.view_mode));
        let view_changed = prev_view_mode.as_ref() != Some(&current_view);
        let scroll_changed = prev_scroll_offset != app.scroll_offset;

        if view_changed || scroll_changed {
            terminal.clear()?;
            prev_view_mode = Some(current_view);
            prev_scroll_offset = app.scroll_offset;
        }

        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => match app.view_mode {
                    ViewMode::TableInfo(_) => app.scroll_down(),
                    _ => app.next(),
                },
                KeyCode::Up | KeyCode::Char('k') => match app.view_mode {
                    ViewMode::TableInfo(_) => app.scroll_up(),
                    _ => app.previous(),
                },
                KeyCode::Enter => {
                    // Drill down into selected table
                    if let ViewMode::Tables = app.view_mode {
                        if let Some(i) = app.list_state.selected() {
                            // Subtract 2 to account for header rows
                            if i >= 2 {
                                if let Some(table) = app.tables.get(i - 2) {
                                    // Analyze indexes for this table
                                    match analyze_indexes(&app.db_path, &table.name) {
                                        Ok(indexes) => {
                                            app.indexes = indexes;
                                            app.view_mode = ViewMode::Indexes(table.name.clone());
                                            app.list_state.select(if app.indexes.is_empty() {
                                                None
                                            } else {
                                                Some(2) // Start at first real item after headers
                                            });
                                        }
                                        Err(_) => {
                                            // Silently ignore errors for now
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('i') => {
                    // Show table info
                    match &app.view_mode {
                        ViewMode::Tables => {
                            if let Some(i) = app.list_state.selected() {
                                // Subtract 2 to account for header rows
                                if i >= 2 {
                                    if let Some(table) = app.tables.get(i - 2) {
                                        match analyze_table_details(&app.db_path, &table.name) {
                                            Ok(details) => {
                                                app.table_details = Some(details);
                                                app.view_mode =
                                                    ViewMode::TableInfo(table.name.clone());
                                                // Clear list state to avoid artifacts
                                                app.list_state.select(None);
                                                app.reset_scroll();
                                            }
                                            Err(_) => {
                                                // Silently ignore errors for now
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ViewMode::Indexes(table_name) => {
                            // From indexes view, show table info
                            match analyze_table_details(&app.db_path, table_name) {
                                Ok(details) => {
                                    app.table_details = Some(details);
                                    app.view_mode = ViewMode::TableInfo(table_name.clone());
                                    // Clear list state to avoid artifacts
                                    app.list_state.select(None);
                                    app.reset_scroll();
                                }
                                Err(_) => {
                                    // Silently ignore errors for now
                                }
                            }
                        }
                        ViewMode::TableInfo(_) => {
                            // Already in info view, do nothing
                        }
                    }
                }
                KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                    // Go back to tables view
                    match &app.view_mode {
                        ViewMode::Indexes(_) | ViewMode::TableInfo(_) => {
                            app.view_mode = ViewMode::Tables;
                            app.list_state.select(Some(2)); // Start at first real item after headers
                        }
                        ViewMode::Tables => {
                            // Already at top level
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <database.db>", args[0]);
        std::process::exit(1);
    }

    let db_path = &args[1];

    println!("Analyzing database: {}", db_path);
    let tables = analyze_database(db_path)?;

    if tables.is_empty() {
        println!("No tables found in database.");
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let app = App::new(db_path.to_string(), tables);
    let res = run_app(&mut terminal, app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}
