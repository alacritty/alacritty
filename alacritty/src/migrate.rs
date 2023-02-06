//! Configuration file migration.

use std::fs;
use std::path::Path;

use toml::Value;

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
    let (prefix, suffix) = match path_str.rsplit_once(".") {
        Some((prefix, suffix)) => (prefix, suffix),
        None => return Err(format!("missing file extension")),
    };

    // Abort if config is already toml.
    if suffix == "toml" {
        return Err(format!("already in TOML format"));
    }

    // Try to parse the configuration file.
    let mut config = match config::deserialize_config(&path) {
        Ok(config) => config,
        Err(err) => return Err(format!("parsing error: {err}")),
    };

    // Migrate config imports.
    if !options.skip_imports {
        migrate_imports(options, &mut config, recursion_limit)?;
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
    let imports = match config::imports(&config, recursion_limit) {
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
