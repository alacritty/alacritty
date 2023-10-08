//! Configuration file migration.

use std::fs;
use std::path::Path;

use toml::map::Entry;
use toml::{Table, Value};

use crate::cli::MigrateOptions;
use crate::config;

/// Handle migration.
pub fn migrate(options: MigrateOptions) {
    // Find configuration file path.
    let config_path = options
        .config_file
        .clone()
        .or_else(|| config::installed_config("toml"))
        .or_else(|| config::installed_config("yml"));

    // Abort if system has no installed configuration.
    let config_path = match config_path {
        Some(config_path) => config_path,
        None => {
            eprintln!("No configuration file found");
            std::process::exit(1);
        },
    };

    // If we're doing a wet run, perform a dry run first for safety.
    if !options.dry_run {
        #[allow(clippy::redundant_clone)]
        let mut options = options.clone();
        options.silent = true;
        options.dry_run = true;
        if let Err(err) = migrate_config(&options, &config_path, config::IMPORT_RECURSION_LIMIT) {
            eprintln!("Configuration file migration failed:");
            eprintln!("    {config_path:?}: {err}");
            std::process::exit(1);
        }
    }

    // Migrate the root config.
    match migrate_config(&options, &config_path, config::IMPORT_RECURSION_LIMIT) {
        Ok(new_path) => {
            if !options.silent {
                println!("Successfully migrated {config_path:?} to {new_path:?}");
            }
        },
        Err(err) => {
            eprintln!("Configuration file migration failed:");
            eprintln!("    {config_path:?}: {err}");
            std::process::exit(1);
        },
    }
}

/// Migrate a specific configuration file.
fn migrate_config(
    options: &MigrateOptions,
    path: &Path,
    recursion_limit: usize,
) -> Result<String, String> {
    // Ensure configuration file has an extension.
    let path_str = path.to_string_lossy();
    let (prefix, suffix) = match path_str.rsplit_once('.') {
        Some((prefix, suffix)) => (prefix, suffix),
        None => return Err("missing file extension".to_string()),
    };

    // Abort if config is already toml.
    if suffix == "toml" {
        return Err("already in TOML format".to_string());
    }

    // Try to parse the configuration file.
    let mut config = match config::deserialize_config(path) {
        Ok(config) => config,
        Err(err) => return Err(format!("parsing error: {err}")),
    };

    // Migrate config imports.
    if !options.skip_imports {
        migrate_imports(options, &mut config, recursion_limit)?;
    }

    // Migrate deprecated field names to their new location.
    if !options.skip_renames {
        migrate_renames(&mut config)?;
    }

    // Convert to TOML format.
    let toml = toml::to_string(&config).map_err(|err| format!("conversion error: {err}"))?;
    let new_path = format!("{prefix}.toml");

    if options.dry_run && !options.silent {
        // Output new content to STDOUT.
        println!(
            "\nv-----Start TOML for {path:?}-----v\n\n{toml}\n^-----End TOML for {path:?}-----^"
        );
    } else if !options.dry_run {
        // Write the new toml configuration.
        fs::write(&new_path, toml).map_err(|err| format!("filesystem error: {err}"))?;
    }

    Ok(new_path)
}

/// Migrate the imports of a config.
fn migrate_imports(
    options: &MigrateOptions,
    config: &mut Value,
    recursion_limit: usize,
) -> Result<(), String> {
    let imports = match config::imports(config, recursion_limit) {
        Ok(imports) => imports,
        Err(err) => return Err(format!("import error: {err}")),
    };

    // Migrate the individual imports.
    let mut new_imports = Vec::new();
    for import in imports {
        let import = match import {
            Ok(import) => import,
            Err(err) => return Err(format!("import error: {err}")),
        };
        let new_path = migrate_config(options, &import, recursion_limit - 1)?;
        new_imports.push(Value::String(new_path));
    }

    // Update the imports field.
    if let Some(import) = config.get_mut("import") {
        *import = Value::Array(new_imports);
    }

    Ok(())
}

/// Migrate deprecated fields.
fn migrate_renames(config: &mut Value) -> Result<(), String> {
    let config_table = match config.as_table_mut() {
        Some(config_table) => config_table,
        None => return Ok(()),
    };

    // draw_bold_text_with_bright_colors -> colors.draw_bold_text_with_bright_colors
    move_value(config_table, &["draw_bold_text_with_bright_colors"], &[
        "colors",
        "draw_bold_text_with_bright_colors",
    ])?;

    // key_bindings -> keyboard.bindings
    move_value(config_table, &["key_bindings"], &["keyboard", "bindings"])?;

    // mouse_bindings -> mouse.bindings
    move_value(config_table, &["mouse_bindings"], &["mouse", "bindings"])?;

    Ok(())
}

/// Move a toml value from one map to another.
fn move_value(config_table: &mut Table, origin: &[&str], target: &[&str]) -> Result<(), String> {
    if let Some(value) = remove_node(config_table, origin)? {
        if !insert_node_if_empty(config_table, target, value)? {
            return Err(format!(
                "conflict: both `{}` and `{}` are set",
                origin.join("."),
                target.join(".")
            ));
        }
    }

    Ok(())
}

/// Remove a node from a tree of tables.
fn remove_node(table: &mut Table, path: &[&str]) -> Result<Option<Value>, String> {
    if path.len() == 1 {
        Ok(table.remove(path[0]))
    } else {
        let next_table_value = match table.get_mut(path[0]) {
            Some(next_table_value) => next_table_value,
            None => return Ok(None),
        };

        let next_table = match next_table_value.as_table_mut() {
            Some(next_table) => next_table,
            None => return Err(format!("invalid `{}` table", path[0])),
        };

        remove_node(next_table, &path[1..])
    }
}

/// Try to insert a node into a tree of tables.
///
/// Returns `false` if the node already exists.
fn insert_node_if_empty(table: &mut Table, path: &[&str], node: Value) -> Result<bool, String> {
    if path.len() == 1 {
        match table.entry(path[0]) {
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(node);
                Ok(true)
            },
            Entry::Occupied(_) => Ok(false),
        }
    } else {
        let next_table_value = table.entry(path[0]).or_insert_with(|| Value::Table(Table::new()));

        let next_table = match next_table_value.as_table_mut() {
            Some(next_table) => next_table,
            None => return Err(format!("invalid `{}` table", path[0])),
        };

        insert_node_if_empty(next_table, &path[1..], node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_values() {
        let input = r#"
root_value = 3

[table]
table_value = 5

[preexisting]
not_moved = 9
        "#;

        let mut value: Value = toml::from_str(input).unwrap();
        let table = value.as_table_mut().unwrap();

        move_value(table, &["root_value"], &["new_table", "root_value"]).unwrap();
        move_value(table, &["table", "table_value"], &["preexisting", "subtable", "new_name"])
            .unwrap();

        let output = toml::to_string(table).unwrap();

        assert_eq!(
            output,
            "[new_table]\nroot_value = 3\n\n[preexisting]\nnot_moved = \
             9\n\n[preexisting.subtable]\nnew_name = 5\n\n[table]\n"
        );
    }
}
