//! Configuration file migration.

use std::fmt::Debug;
use std::path::Path;
use std::{fs, mem};

use tempfile::NamedTempFile;
use toml_edit::{DocumentMut, Item};

use crate::cli::MigrateOptions;
use crate::config;

mod yaml;

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
        Ok(migration) => {
            if !options.silent {
                println!("{}", migration.success_message(false));
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
fn migrate_config<'a>(
    options: &MigrateOptions,
    path: &'a Path,
    recursion_limit: usize,
) -> Result<Migration<'a>, String> {
    // Ensure configuration file has an extension.
    let path_str = path.to_string_lossy();
    let (prefix, suffix) = match path_str.rsplit_once('.') {
        Some((prefix, suffix)) => (prefix, suffix),
        None => return Err("missing file extension".to_string()),
    };

    // Handle legacy YAML files.
    if suffix == "yml" {
        let new_path = yaml::migrate(options, path, recursion_limit, prefix)?;
        return Ok(Migration::Yaml((path, new_path)));
    }

    // TOML only does renames, so return early if they are disabled.
    if options.skip_renames {
        if options.dry_run {
            eprintln!("Ignoring TOML file {path:?} since `--skip-renames` was supplied");
        }
        return Ok(Migration::Toml(path));
    }

    // Read TOML file and perform all in-file migrations.
    let toml = fs::read_to_string(path).map_err(|err| format!("{err}"))?;
    let mut migrated = migrate_toml(toml)?;

    // Recursively migrate imports.
    migrate_imports(options, path, &mut migrated, recursion_limit)?;

    // Write migrated TOML file.
    write_results(options, path, &migrated.to_string())?;

    Ok(Migration::Toml(path))
}

/// Migrate TOML config to the latest version.
fn migrate_toml(toml: String) -> Result<DocumentMut, String> {
    // Parse TOML file.
    let mut document = match toml.parse::<DocumentMut>() {
        Ok(document) => document,
        Err(err) => return Err(format!("TOML parsing error: {err}")),
    };

    // Move `draw_bold_text_with_bright_colors` to its own section.
    move_value(&mut document, &["draw_bold_text_with_bright_colors"], &[
        "colors",
        "draw_bold_text_with_bright_colors",
    ])?;

    // Move bindings to their own section.
    move_value(&mut document, &["key_bindings"], &["keyboard", "bindings"])?;
    move_value(&mut document, &["mouse_bindings"], &["mouse", "bindings"])?;

    // Avoid warnings due to introduction of the new `general` section.
    move_value(&mut document, &["live_config_reload"], &["general", "live_config_reload"])?;
    move_value(&mut document, &["working_directory"], &["general", "working_directory"])?;
    move_value(&mut document, &["ipc_socket"], &["general", "ipc_socket"])?;
    move_value(&mut document, &["import"], &["general", "import"])?;
    move_value(&mut document, &["shell"], &["terminal", "shell"])?;

    Ok(document)
}

/// Migrate TOML imports to the latest version.
fn migrate_imports(
    options: &MigrateOptions,
    path: &Path,
    document: &mut DocumentMut,
    recursion_limit: usize,
) -> Result<(), String> {
    // Check if any imports need to be processed.
    let imports = match document
        .get("general")
        .and_then(|general| general.get("import"))
        .and_then(|import| import.as_array())
    {
        Some(array) if !array.is_empty() => array,
        _ => return Ok(()),
    };

    // Abort once recursion limit is exceeded.
    if recursion_limit == 0 {
        return Err("Exceeded maximum configuration import depth".into());
    }

    // Migrate each import.
    for import in imports.into_iter().filter_map(|item| item.as_str()) {
        let normalized_path = config::normalize_import(path, import);

        if !normalized_path.exists() {
            if options.dry_run {
                println!("Skipping migration for nonexistent path: {}", normalized_path.display());
            }
            continue;
        }

        let migration = migrate_config(options, &normalized_path, recursion_limit - 1)?;
        if options.dry_run {
            println!("{}", migration.success_message(true));
        }
    }

    Ok(())
}

/// Move a TOML value from one map to another.
fn move_value(document: &mut DocumentMut, origin: &[&str], target: &[&str]) -> Result<(), String> {
    // Find and remove the original item.
    let (mut origin_key, mut origin_item) = (None, document.as_item_mut());
    for element in origin {
        let table = match origin_item.as_table_like_mut() {
            Some(table) => table,
            None => panic!("Moving from unsupported TOML structure"),
        };

        let (key, item) = match table.get_key_value_mut(element) {
            Some((key, item)) => (key, item),
            None => return Ok(()),
        };

        origin_key = Some(key);
        origin_item = item;

        // Ensure no empty tables are left behind.
        if let Some(table) = origin_item.as_table_mut() {
            table.set_implicit(true)
        }
    }

    let origin_key_decor =
        origin_key.map(|key| (key.leaf_decor().clone(), key.dotted_decor().clone()));
    let origin_item = mem::replace(origin_item, Item::None);

    // Create all dependencies for the new location.
    let mut target_item = document.as_item_mut();
    for (i, element) in target.iter().enumerate() {
        let table = match target_item.as_table_like_mut() {
            Some(table) => table,
            None => panic!("Moving into unsupported TOML structure"),
        };

        if i + 1 == target.len() {
            table.insert(element, origin_item);
            // Move original key decorations.
            if let Some((leaf, dotted)) = origin_key_decor {
                let mut key = table.key_mut(element).unwrap();
                *key.leaf_decor_mut() = leaf;
                *key.dotted_decor_mut() = dotted;
            }

            break;
        } else {
            // Create missing parent tables.
            target_item = target_item[element].or_insert(toml_edit::table());
        }
    }

    Ok(())
}

/// Write migrated TOML to its target location.
fn write_results<P>(options: &MigrateOptions, path: P, toml: &str) -> Result<(), String>
where
    P: AsRef<Path> + Debug,
{
    let path = path.as_ref();
    if options.dry_run && !options.silent {
        // Output new content to STDOUT.
        println!(
            "\nv-----Start TOML for {path:?}-----v\n\n{toml}\n^-----End TOML for {path:?}-----^\n"
        );
    } else if !options.dry_run {
        // Atomically replace the configuration file.
        let tmp = NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|err| format!("could not create temporary file: {err}"))?;
        fs::write(tmp.path(), toml).map_err(|err| format!("filesystem error: {err}"))?;
        tmp.persist(path).map_err(|err| format!("atomic replacement failed: {err}"))?;
    }
    Ok(())
}

/// Performed migration mode.
enum Migration<'a> {
    /// In-place TOML migration.
    Toml(&'a Path),
    /// YAML to TOML migration.
    Yaml((&'a Path, String)),
}

impl Migration<'_> {
    /// Get the success message for this migration.
    fn success_message(&self, import: bool) -> String {
        match self {
            Self::Yaml((original_path, new_path)) if import => {
                format!("Successfully migrated import {original_path:?} to {new_path:?}")
            },
            Self::Yaml((original_path, new_path)) => {
                format!("Successfully migrated {original_path:?} to {new_path:?}")
            },
            Self::Toml(original_path) if import => {
                format!("Successfully migrated import {original_path:?}")
            },
            Self::Toml(original_path) => format!("Successfully migrated {original_path:?}"),
        }
    }

    /// Get the file path after migration.
    fn new_path(&self) -> String {
        match self {
            Self::Toml(path) => path.to_string_lossy().into(),
            Self::Yaml((_, path)) => path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_values() {
        let input = r#"
# This is a root_value.
#
# Use it with care.
root_value = 3

[table]
table_value = 5

[preexisting]
not_moved = 9
        "#;

        let mut document = input.parse::<DocumentMut>().unwrap();

        move_value(&mut document, &["root_value"], &["new_table", "root_value"]).unwrap();
        move_value(&mut document, &["table", "table_value"], &[
            "preexisting",
            "subtable",
            "new_name",
        ])
        .unwrap();

        let output = document.to_string();

        let expected = r#"
[preexisting]
not_moved = 9

[preexisting.subtable]
new_name = 5

[new_table]

# This is a root_value.
#
# Use it with care.
root_value = 3
        "#;

        assert_eq!(output, expected);
    }

    #[test]
    fn migrate_empty() {
        assert!(migrate_toml(String::new()).unwrap().to_string().is_empty());
    }
}
