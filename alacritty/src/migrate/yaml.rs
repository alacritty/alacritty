//! Migration of legacy YAML files to TOML.

use std::path::Path;

use toml::Value;

use crate::cli::MigrateOptions;
use crate::config;
use crate::migrate::{migrate_config, migrate_toml, write_results};

/// Migrate a legacy YAML config to TOML.
pub fn migrate(
    options: &MigrateOptions,
    path: &Path,
    recursion_limit: usize,
    prefix: &str,
) -> Result<String, String> {
    // Try to parse the configuration file.
    let mut config = match config::deserialize_config(path, !options.dry_run) {
        Ok(config) => config,
        Err(err) => return Err(format!("YAML parsing error: {err}")),
    };

    // Migrate config imports.
    if !options.skip_imports {
        migrate_imports(options, &mut config, path, recursion_limit)?;
    }

    // Convert to TOML format.
    let mut toml = toml::to_string(&config).map_err(|err| format!("conversion error: {err}"))?;
    let new_path = format!("{prefix}.toml");

    // Apply TOML migration, without recursing through imports.
    toml = migrate_toml(toml)?.to_string();

    // Write migrated TOML config.
    write_results(options, &new_path, &toml)?;

    Ok(new_path)
}

/// Migrate the imports of a config.
fn migrate_imports(
    options: &MigrateOptions,
    config: &mut Value,
    base_path: &Path,
    recursion_limit: usize,
) -> Result<(), String> {
    let imports = match config::imports(config, base_path, recursion_limit) {
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

        // Keep yaml import if path does not exist.
        if !import.exists() {
            if options.dry_run {
                eprintln!("Keeping yaml config for nonexistent import: {import:?}");
            }
            new_imports.push(Value::String(import.to_string_lossy().into()));
            continue;
        }

        let migration = migrate_config(options, &import, recursion_limit - 1)?;

        // Print success message.
        if options.dry_run {
            println!("{}", migration.success_message(true));
        }

        new_imports.push(Value::String(migration.new_path()));
    }

    // Update the imports field.
    if let Some(import) = config.get_mut("import") {
        *import = Value::Array(new_imports);
    }

    Ok(())
}
