use anyhow::{Context, Result};
use ratatui::widgets::ListState;
use rusqlite::{Connection, Result as SqliteResult};

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub size_bytes: u64,
    pub row_count: u64,
    pub index_count: u64,
    pub index_size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub size_bytes: u64,
    pub is_unique: bool,
    pub columns: String,
    pub partial_clause: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub col_type: String,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub is_pk: bool,
}

#[derive(Debug, Clone)]
pub struct ForeignKeyInfo {
    pub from_col: String,
    pub to_table: String,
    pub to_col: String,
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Debug, Clone)]
pub struct TableDetails {
    pub ddl: String,
    pub columns: Vec<ColumnInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub triggers: Vec<String>,
}

pub enum ViewMode {
    Tables,
    Indexes(String),   // table name
    TableInfo(String), // table name
}

pub struct App {
    pub tables: Vec<TableInfo>,
    pub indexes: Vec<IndexInfo>,
    pub table_details: Option<TableDetails>,
    pub list_state: ListState,
    pub scroll_offset: u16,
    pub db_path: String,
    pub total_size: u64,
    pub view_mode: ViewMode,
}

impl App {
    pub fn new(db_path: String, tables: Vec<TableInfo>) -> Self {
        let total_size = tables.iter().map(|t| t.size_bytes).sum();
        let mut list_state = ListState::default();
        if !tables.is_empty() {
            // Start at index 2 to skip header rows
            list_state.select(Some(2));
        }
        Self {
            tables,
            indexes: Vec::new(),
            table_details: None,
            list_state,
            scroll_offset: 0,
            db_path,
            total_size,
            view_mode: ViewMode::Tables,
        }
    }

    pub fn next(&mut self) {
        let len = match &self.view_mode {
            ViewMode::Tables => self.tables.len(),
            ViewMode::Indexes(_) => self.indexes.len(),
            ViewMode::TableInfo(_) => 0, // No navigation in info view
        };

        if len == 0 {
            return;
        }

        // Offset by 2 for header rows
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= len + 2 - 1 {
                    2
                } else {
                    i + 1
                }
            }
            None => 2,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let len = match &self.view_mode {
            ViewMode::Tables => self.tables.len(),
            ViewMode::Indexes(_) => self.indexes.len(),
            ViewMode::TableInfo(_) => 0, // No navigation in info view
        };

        if len == 0 {
            return;
        }

        // Offset by 2 for header rows
        let i = match self.list_state.selected() {
            Some(i) => {
                if i <= 2 {
                    len + 2 - 1
                } else {
                    i - 1
                }
            }
            None => 2,
        };
        self.list_state.select(Some(i));
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }
}

pub fn analyze_database(db_path: &str) -> Result<Vec<TableInfo>> {
    let conn = Connection::open(db_path).context("Failed to open database")?;

    let mut tables = Vec::new();

    // Get all table names
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    )?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<SqliteResult<Vec<String>>>()?;

    for table_name in table_names {
        // Get row count
        let row_count: u64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Get approximate size (number of pages * page size)
        let size_bytes: u64 = conn
            .query_row(
                "SELECT SUM(pgsize) FROM dbstat WHERE name = ?1",
                [&table_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count indexes for this table
        let index_count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name=?1 AND name NOT LIKE 'sqlite_%'",
                [&table_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Get total index size for this table
        let index_size_bytes: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name IN (SELECT name FROM sqlite_master WHERE type='index' AND tbl_name=?1)",
                [&table_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        tables.push(TableInfo {
            name: table_name,
            size_bytes,
            row_count,
            index_count,
            index_size_bytes,
        });
    }

    // Sort by size descending
    tables.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    Ok(tables)
}

pub fn analyze_indexes(db_path: &str, table_name: &str) -> Result<Vec<IndexInfo>> {
    let conn = Connection::open(db_path).context("Failed to open database")?;

    let mut indexes = Vec::new();

    // Get all indexes for this table
    let mut stmt = conn.prepare(
        "SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name=?1 AND name NOT LIKE 'sqlite_%'"
    )?;

    let index_data: Vec<(String, Option<String>)> = stmt
        .query_map([table_name], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<SqliteResult<Vec<(String, Option<String>)>>>()?;

    for (index_name, sql) in index_data {
        // Get index size
        let size_bytes: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name = ?1",
                [&index_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Parse columns, uniqueness, and partial clause from SQL
        let (is_unique, columns, partial_clause) = if let Some(sql_str) = sql {
            let is_unique = sql_str.to_uppercase().contains("UNIQUE");

            // Extract column names from CREATE INDEX ... ON table(col1, col2)
            let columns = if let Some(start) = sql_str.find('(') {
                // Find WHERE clause if it exists
                let end = if let Some(where_pos) = sql_str.to_uppercase().find(" WHERE ") {
                    where_pos.min(sql_str.len())
                } else {
                    sql_str.len()
                };

                // Find the closing paren before WHERE or end of string
                if let Some(close_paren) = sql_str[start..end].rfind(')') {
                    sql_str[start + 1..start + close_paren].to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Extract WHERE clause for partial indexes
            let partial_clause = if let Some(where_start) = sql_str.to_uppercase().find(" WHERE ") {
                Some(sql_str[where_start + 7..].trim().to_string())
            } else {
                None
            };

            (is_unique, columns, partial_clause)
        } else {
            // Automatic index (e.g., from PRIMARY KEY)
            (false, String::from("(auto)"), None)
        };

        indexes.push(IndexInfo {
            name: index_name,
            size_bytes,
            is_unique,
            columns,
            partial_clause,
        });
    }

    // Sort by size descending
    indexes.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    Ok(indexes)
}

pub fn analyze_table_details(db_path: &str, table_name: &str) -> Result<TableDetails> {
    let conn = Connection::open(db_path).context("Failed to open database")?;

    // Get DDL
    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
            [table_name],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| String::from("-- DDL not available"));

    // Get column info
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let columns: Vec<ColumnInfo> = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                name: row.get(1)?,
                col_type: row.get(2)?,
                not_null: row.get::<_, i32>(3)? != 0,
                default_value: row.get(4)?,
                is_pk: row.get::<_, i32>(5)? != 0,
            })
        })?
        .collect::<SqliteResult<Vec<ColumnInfo>>>()?;

    // Get foreign keys
    let mut stmt = conn.prepare(&format!("PRAGMA foreign_key_list(\"{}\")", table_name))?;
    let foreign_keys: Vec<ForeignKeyInfo> = stmt
        .query_map([], |row| {
            Ok(ForeignKeyInfo {
                from_col: row.get(3)?,
                to_table: row.get(2)?,
                to_col: row.get(4)?,
                on_update: row.get(5)?,
                on_delete: row.get(6)?,
            })
        })?
        .collect::<SqliteResult<Vec<ForeignKeyInfo>>>()?;

    // Get triggers
    let mut stmt =
        conn.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name=?1")?;
    let triggers: Vec<String> = stmt
        .query_map([table_name], |row| row.get(0))?
        .collect::<SqliteResult<Vec<String>>>()?;

    Ok(TableDetails {
        ddl,
        columns,
        foreign_keys,
        triggers,
    })
}
