use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

use crate::app::{App, ViewMode};

pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn highlight_sql(sql: &str) -> Vec<Line<'static>> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ps
        .find_syntax_by_extension("sql")
        .unwrap_or_else(|| ps.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    let mut lines = Vec::new();

    // Pretty-format the SQL first
    let formatted_sql = format_sql(sql);

    for line_text in LinesWithEndings::from(&formatted_sql) {
        let ranges: Vec<(SyntectStyle, &str)> = h.highlight_line(line_text, &ps).unwrap();

        let spans: Vec<Span> = ranges
            .into_iter()
            .map(|(style, text)| {
                Span::styled(
                    text.to_string(),
                    Style::default().fg(syntect_to_ratatui_color(style.foreground)),
                )
            })
            .collect();

        lines.push(Line::from(spans));
    }

    lines
}

fn format_sql(sql: &str) -> String {
    // Clean up SQLite's DDL formatting
    let mut result = sql.to_string();

    // Ensure space after table name in CREATE TABLE statements
    // Replace patterns like "](" with "] ("
    result = result.replace("](", "] (");

    // Also handle case without brackets
    result = result.replace(")(", ") (");

    // Add space after column name brackets before type
    // e.g., [OrderId]INTEGER -> [OrderId] INTEGER
    result = result.replace("]INTEGER", "] INTEGER");
    result = result.replace("]REAL", "] REAL");
    result = result.replace("]TEXT", "] TEXT");
    result = result.replace("]BLOB", "] BLOB");
    result = result.replace("]NUMERIC", "] NUMERIC");

    result
}

pub fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Header
    let header_text = match &app.view_mode {
        ViewMode::Tables => format!("sqdu - SQLite Disk Usage Analyzer - {}", app.db_path),
        ViewMode::Indexes(table_name) => {
            format!("sqdu - Indexes for table: {} - {}", table_name, app.db_path)
        }
        ViewMode::TableInfo(table_name) => {
            format!("sqdu - Table Info: {} - {}", table_name, app.db_path)
        }
    };

    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(header, chunks[0]);

    // Main content area - either tables or indexes
    match &app.view_mode {
        ViewMode::Tables => {
            // Add header row explaining columns
            let mut all_items = vec![
                ListItem::new("     Size      %          Rows  Idx   Idx Size  Table Name")
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ListItem::new("─────────────────────────────────────────────────────────────────────────────────")
                    .style(Style::default().fg(Color::DarkGray)),
            ];

            let table_items: Vec<ListItem> = app
                .tables
                .iter()
                .map(|table| {
                    let percentage = if app.total_size > 0 {
                        (table.size_bytes as f64 / app.total_size as f64) * 100.0
                    } else {
                        0.0
                    };

                    let content = format!(
                        "{:>9}  {:>5.1}%  {:>10} rows  {:>2} idx  {:>9} idx size  {}",
                        format_bytes(table.size_bytes),
                        percentage,
                        format_number(table.row_count),
                        table.index_count,
                        format_bytes(table.index_size_bytes),
                        table.name
                    );
                    ListItem::new(content)
                })
                .collect();

            all_items.extend(table_items);

            let list = List::new(all_items)
                .block(Block::default().borders(Borders::ALL).title("Tables"))
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, chunks[1], &mut app.list_state);
        }
        ViewMode::Indexes(table_name) => {
            // Add header row explaining columns
            let mut all_items = vec![
                ListItem::new("    Size      Type    Columns                                   Name")
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ListItem::new("─────────────────────────────────────────────────────────────────────────────────")
                    .style(Style::default().fg(Color::DarkGray)),
            ];

            let index_items: Vec<ListItem> = app
                .indexes
                .iter()
                .map(|index| {
                    let type_marker = if index.is_unique { "UNIQUE" } else { "INDEX " };
                    let partial_marker = if index.partial_clause.is_some() {
                        " [PARTIAL]"
                    } else {
                        ""
                    };
                    let content = format!(
                        "{:>9}  {}  {:<40}  {}{}",
                        format_bytes(index.size_bytes),
                        type_marker,
                        index.columns,
                        index.name,
                        partial_marker
                    );
                    ListItem::new(content)
                })
                .collect();

            all_items.extend(index_items);

            let list = List::new(all_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Indexes for {}", table_name)),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, chunks[1], &mut app.list_state);
        }
        ViewMode::TableInfo(table_name) => {
            if let Some(details) = &app.table_details {
                let mut all_lines = vec![];

                // DDL Section with syntax highlighting
                all_lines.push(Line::from(Span::styled(
                    "━━━ CREATE TABLE Statement ━━━",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )));

                // Add syntax highlighted SQL
                all_lines.extend(highlight_sql(&details.ddl));
                all_lines.push(Line::from(""));

                // Columns Section
                all_lines.push(Line::from(Span::styled(
                    "━━━ Columns ━━━",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )));

                for col in &details.columns {
                    let pk_marker = if col.is_pk { " [PK]" } else { "" };
                    let not_null = if col.not_null { " NOT NULL" } else { "" };
                    let default = if let Some(d) = &col.default_value {
                        format!(" DEFAULT {}", d)
                    } else {
                        String::new()
                    };

                    all_lines.push(Line::from(vec![
                        Span::raw("  • "),
                        Span::styled(&col.name, Style::default().fg(Color::Cyan)),
                        Span::styled(pk_marker, Style::default().fg(Color::Yellow)),
                        Span::raw(": "),
                        Span::styled(&col.col_type, Style::default().fg(Color::Green)),
                        Span::styled(not_null, Style::default().fg(Color::Red)),
                        Span::raw(default),
                    ]));
                }
                all_lines.push(Line::from(""));

                // Foreign Keys Section
                if !details.foreign_keys.is_empty() {
                    all_lines.push(Line::from(Span::styled(
                        "━━━ Foreign Keys ━━━",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )));

                    for fk in &details.foreign_keys {
                        all_lines.push(Line::from(format!(
                            "  • {} -> {}.{} (UPDATE: {}, DELETE: {})",
                            fk.from_col, fk.to_table, fk.to_col, fk.on_update, fk.on_delete
                        )));
                    }
                    all_lines.push(Line::from(""));
                }

                // Triggers Section
                if !details.triggers.is_empty() {
                    all_lines.push(Line::from(Span::styled(
                        "━━━ Triggers ━━━",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )));

                    for trigger in &details.triggers {
                        all_lines.push(Line::from(format!("  • {}", trigger)));
                    }
                }

                let paragraph = Paragraph::new(all_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("Table Info: {}", table_name)),
                    )
                    .scroll((app.scroll_offset, 0));
                // Don't wrap - let long lines scroll off screen

                f.render_widget(paragraph, chunks[1]);
            } else {
                let paragraph = Paragraph::new("Loading table details...")
                    .block(Block::default().borders(Borders::ALL))
                    .style(Style::default().bg(Color::Reset));
                f.render_widget(paragraph, chunks[1]);
            }
        }
    }

    // Footer
    let (selected_info, nav_hint) = match &app.view_mode {
        ViewMode::Tables => {
            let info = if let Some(i) = app.list_state.selected() {
                // Subtract 2 to account for header rows
                if i >= 2 {
                    if let Some(table) = app.tables.get(i - 2) {
                        format!(
                            "Selected: {} ({}, {} rows)",
                            table.name,
                            format_bytes(table.size_bytes),
                            format_number(table.row_count)
                        )
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            (info, "Enter: indexes | i: info | q: quit | ↑↓: navigate")
        }
        ViewMode::Indexes(_) => {
            let info = if let Some(i) = app.list_state.selected() {
                // Subtract 2 to account for header rows
                if i >= 2 {
                    if let Some(index) = app.indexes.get(i - 2) {
                        let mut info_parts = vec![format!(
                            "Selected: {} ({})",
                            index.name,
                            format_bytes(index.size_bytes)
                        )];
                        if let Some(partial) = &index.partial_clause {
                            info_parts.push(format!("WHERE {}", partial));
                        }
                        info_parts.join(" | ")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            (
                info,
                "Backspace: back to tables | i: info | q: quit | ↑↓: navigate",
            )
        }
        ViewMode::TableInfo(_) => (String::new(), "Backspace: back to tables | q: quit"),
    };

    let footer_text = if selected_info.is_empty() {
        format!("Total: {} | {}", format_bytes(app.total_size), nav_hint)
    } else {
        format!(
            "{} | Total: {} | {}",
            selected_info,
            format_bytes(app.total_size),
            nav_hint
        )
    };

    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);
}
