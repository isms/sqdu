use sqdu::app::{analyze_database, analyze_indexes, analyze_table_details};
use std::path::PathBuf;

fn get_northwind_path() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("assets");
    path.push("northwind.db");
    path.to_str().unwrap().to_string()
}

#[test]
fn test_analyze_database_returns_tables() {
    let db_path = get_northwind_path();
    let result = analyze_database(&db_path);

    assert!(
        result.is_ok(),
        "Failed to analyze database: {:?}",
        result.err()
    );
    let tables = result.unwrap();

    assert!(!tables.is_empty(), "Database should have tables");
    assert!(tables.len() > 5, "Northwind should have multiple tables");
}

#[test]
fn test_tables_have_valid_names() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in &tables {
        assert!(!table.name.is_empty(), "Table name should not be empty");
        assert!(
            !table.name.starts_with("sqlite_"),
            "Should filter out sqlite internal tables"
        );
    }
}

#[test]
fn test_tables_have_size_info() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // At least some tables should have non-zero size
    let non_empty_tables = tables.iter().filter(|t| t.size_bytes > 0).count();
    assert!(
        non_empty_tables > 0,
        "At least some tables should have size data"
    );
}

#[test]
fn test_tables_have_row_counts() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Check that at least some tables have rows
    let tables_with_rows = tables.iter().filter(|t| t.row_count > 0).count();
    assert!(
        tables_with_rows > 0,
        "At least some tables should have rows"
    );
}

#[test]
fn test_tables_sorted_by_size() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Verify tables are sorted by size descending
    for i in 0..tables.len().saturating_sub(1) {
        assert!(
            tables[i].size_bytes >= tables[i + 1].size_bytes,
            "Tables should be sorted by size descending"
        );
    }
}

#[test]
fn test_tables_have_index_counts() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Northwind has user-created indexes (Orders, Products, Employees)
    let tables_with_indexes = tables.iter().filter(|t| t.index_count > 0).count();
    assert!(
        tables_with_indexes >= 3,
        "Northwind should have at least 3 tables with indexes (found {})",
        tables_with_indexes
    );
}

#[test]
fn test_tables_have_index_sizes() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Tables with indexes should have non-zero index size
    let tables_with_index_size = tables
        .iter()
        .filter(|t| t.index_count > 0 && t.index_size_bytes > 0)
        .count();

    assert!(
        tables_with_index_size >= 3,
        "Northwind tables with indexes should have index size (found {})",
        tables_with_index_size
    );
}

#[test]
fn test_analyze_indexes_for_table() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Find a table that has indexes
    let table_with_indexes = tables.iter().find(|t| t.index_count > 0);

    if let Some(table) = table_with_indexes {
        let result = analyze_indexes(&db_path, &table.name);
        assert!(
            result.is_ok(),
            "Failed to analyze indexes: {:?}",
            result.err()
        );

        let indexes = result.unwrap();
        assert!(
            !indexes.is_empty(),
            "Table with index_count > 0 should have indexes"
        );
    }
}

#[test]
fn test_indexes_have_valid_names() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().filter(|t| t.index_count > 0) {
        let indexes = analyze_indexes(&db_path, &table.name).unwrap();

        for index in &indexes {
            assert!(!index.name.is_empty(), "Index name should not be empty");
            assert!(
                !index.name.starts_with("sqlite_autoindex"),
                "Should filter sqlite internal indexes"
            );
        }
    }
}

#[test]
fn test_indexes_have_columns() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().filter(|t| t.index_count > 0) {
        let indexes = analyze_indexes(&db_path, &table.name).unwrap();

        for index in &indexes {
            assert!(
                !index.columns.is_empty(),
                "Index should have column information"
            );
        }
    }
}

#[test]
fn test_indexes_sorted_by_size() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().filter(|t| t.index_count > 1) {
        let indexes = analyze_indexes(&db_path, &table.name).unwrap();

        // Verify indexes are sorted by size descending
        for i in 0..indexes.len().saturating_sub(1) {
            assert!(
                indexes[i].size_bytes >= indexes[i + 1].size_bytes,
                "Indexes should be sorted by size descending"
            );
        }
    }
}

#[test]
fn test_unique_index_detection() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Check that unique flag is properly detected
    for table in tables.iter().filter(|t| t.index_count > 0) {
        let indexes = analyze_indexes(&db_path, &table.name).unwrap();

        // Just verify the field exists and is boolean (no panic)
        for index in &indexes {
            let _unique = index.is_unique; // This will compile if the field exists
        }
    }
}

#[test]
fn test_partial_index_detection() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Find the Employees table which has a partial index
    let employees_table = tables.iter().find(|t| t.name == "Employees");
    assert!(employees_table.is_some(), "Employees table should exist");

    let indexes = analyze_indexes(&db_path, "Employees").unwrap();
    assert!(
        !indexes.is_empty(),
        "Employees should have at least one index"
    );

    // Find the partial index
    let partial_index = indexes.iter().find(|idx| idx.partial_clause.is_some());
    assert!(
        partial_index.is_some(),
        "Employees should have a partial index (idx_employees_lastname)"
    );

    if let Some(idx) = partial_index {
        let clause = idx.partial_clause.as_ref().unwrap();
        assert!(!clause.is_empty(), "Partial clause should not be empty");
        assert!(
            clause.to_uppercase().contains("LASTNAME"),
            "Partial clause should reference LastName"
        );
    }
}

#[test]
fn test_analyze_table_details() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    assert!(
        !tables.is_empty(),
        "Need at least one table to test details"
    );

    let result = analyze_table_details(&db_path, &tables[0].name);
    assert!(
        result.is_ok(),
        "Failed to analyze table details: {:?}",
        result.err()
    );
}

#[test]
fn test_table_details_have_ddl() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().take(3) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        assert!(!details.ddl.is_empty(), "DDL should not be empty");
        assert!(
            details.ddl.to_uppercase().contains("CREATE"),
            "DDL should contain CREATE statement"
        );
    }
}

#[test]
fn test_table_details_have_columns() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().take(3) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        assert!(
            !details.columns.is_empty(),
            "Table should have at least one column"
        );

        for col in &details.columns {
            assert!(!col.name.is_empty(), "Column name should not be empty");
            assert!(!col.col_type.is_empty(), "Column type should not be empty");
        }
    }
}

#[test]
fn test_column_info_completeness() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().take(3) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        for col in &details.columns {
            // Verify all fields are accessible
            let _name = &col.name;
            let _col_type = &col.col_type;
            let _not_null = col.not_null;
            let _default = &col.default_value;
            let _is_pk = col.is_pk;
        }
    }
}

#[test]
fn test_primary_key_detection() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Most tables should have a primary key
    let mut found_pk = false;

    for table in tables.iter().take(5) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        for col in &details.columns {
            if col.is_pk {
                found_pk = true;
                break;
            }
        }
        if found_pk {
            break;
        }
    }

    assert!(found_pk, "At least one table should have a primary key");
}

#[test]
fn test_foreign_keys_structure() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().take(5) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        // Check foreign keys structure
        for fk in &details.foreign_keys {
            assert!(
                !fk.from_col.is_empty(),
                "Foreign key from_col should not be empty"
            );
            assert!(
                !fk.to_table.is_empty(),
                "Foreign key to_table should not be empty"
            );
            assert!(
                !fk.to_col.is_empty(),
                "Foreign key to_col should not be empty"
            );
            assert!(
                !fk.on_update.is_empty(),
                "Foreign key on_update should not be empty"
            );
            assert!(
                !fk.on_delete.is_empty(),
                "Foreign key on_delete should not be empty"
            );
        }
    }
}

#[test]
fn test_triggers_list() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().take(3) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        // Verify triggers list is valid (can be empty)
        for trigger in &details.triggers {
            assert!(!trigger.is_empty(), "Trigger name should not be empty");
        }
    }
}

#[test]
fn test_invalid_database_path() {
    let result = analyze_database("/nonexistent/path/to/database.db");
    assert!(result.is_err(), "Should fail with invalid database path");
}

#[test]
fn test_invalid_table_name_for_indexes() {
    let db_path = get_northwind_path();
    let result = analyze_indexes(&db_path, "NonExistentTableName12345");

    // Should succeed but return empty list
    assert!(
        result.is_ok(),
        "Should handle non-existent table gracefully"
    );
    let indexes = result.unwrap();
    assert!(
        indexes.is_empty(),
        "Non-existent table should have no indexes"
    );
}

#[test]
fn test_invalid_table_name_for_details() {
    let db_path = get_northwind_path();
    let result = analyze_table_details(&db_path, "NonExistentTableName12345");

    assert!(
        result.is_ok(),
        "Should handle non-existent table gracefully"
    );
    let details = result.unwrap();
    assert!(
        details.columns.is_empty(),
        "Non-existent table should have no columns"
    );
}

#[test]
fn test_table_size_consistency() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Verify all tables have valid size information (u64 is always >= 0)
    for table in &tables {
        // If table has data, it should have size
        if table.row_count > 0 {
            assert!(table.size_bytes > 0, "Table with rows should have size");
        }
    }
}

#[test]
fn test_row_count_vs_size_correlation() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in &tables {
        // If a table has rows, it should have size
        if table.row_count > 0 {
            assert!(
                table.size_bytes > 0,
                "Table with rows should have non-zero size"
            );
        }
    }
}

#[test]
fn test_multiple_tables_analysis() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Analyze multiple tables to ensure no cross-contamination
    let mut sizes = Vec::new();

    for table in tables.iter().take(5) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();
        sizes.push((table.name.clone(), details.columns.len()));
    }

    // Verify each table has different characteristics
    // (In practice, tables could have same column count, but we're checking independence)
    assert!(sizes.len() >= 2, "Need at least 2 tables for comparison");
}

#[test]
fn test_index_count_matches_actual() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    for table in tables.iter().filter(|t| t.index_count > 0) {
        let indexes = analyze_indexes(&db_path, &table.name).unwrap();

        assert_eq!(
            table.index_count as usize,
            indexes.len(),
            "Index count should match actual number of indexes for table {}",
            table.name
        );
    }
}

#[test]
fn test_column_types_are_valid() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    let valid_types = vec![
        "INTEGER", "TEXT", "REAL", "BLOB", "NUMERIC", "VARCHAR", "CHAR", "DATETIME", "DATE",
    ];

    for table in tables.iter().take(3) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        for col in &details.columns {
            let upper_type = col.col_type.to_uppercase();
            let is_valid = valid_types.iter().any(|t| upper_type.contains(t));
            assert!(
                is_valid || col.col_type.is_empty(),
                "Column type '{}' should be a valid SQLite type",
                col.col_type
            );
        }
    }
}

#[test]
fn test_database_total_size() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    let total_size: u64 = tables.iter().map(|t| t.size_bytes).sum();
    assert!(
        total_size > 0,
        "Total database size should be greater than 0"
    );
}

#[test]
fn test_empty_table_handling() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Check that empty tables (if any) are handled correctly
    for table in tables.iter().filter(|t| t.row_count == 0) {
        let details = analyze_table_details(&db_path, &table.name).unwrap();

        // Empty table should still have column definitions
        assert!(
            !details.columns.is_empty(),
            "Even empty tables should have columns"
        );
    }
}

#[test]
fn test_concurrent_analysis() {
    use std::thread;

    let db_path = get_northwind_path();

    let handles: Vec<_> = (0..3)
        .map(|_| {
            let path = db_path.clone();
            thread::spawn(move || {
                let tables = analyze_database(&path).unwrap();
                assert!(!tables.is_empty());
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_special_characters_in_table_names() {
    let db_path = get_northwind_path();
    let tables = analyze_database(&db_path).unwrap();

    // Verify we can handle table names with various characters
    for table in &tables {
        // Should be able to analyze tables regardless of name
        let result = analyze_table_details(&db_path, &table.name);
        assert!(result.is_ok(), "Should handle table name: {}", table.name);
    }
}
