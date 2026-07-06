//! Config-key checks (docs/40-parameterisation.md §2, §6.3–6.4, ADR 0008).
//!
//! - **Validity:** every shipped layer and the merged whole must load,
//!   deserialise, and pass semantic validation (the real loader is used).
//! - **Namespaces:** every key sits under a registered root and follows the
//!   `lower_snake_case` dotted-path convention.
//! - **Integrity:** the default layer defines exactly the schema's keys
//!   (no orphans on either side); other layers only override known keys.
//!   Code↔schema correspondence is structural under types-first (ADR 0008):
//!   code reads typed params that deserialisation guarantees match the schema.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Registered namespace roots (docs/40-parameterisation.md §2.2). Adding a
/// root is an architectural change → ADR.
const REGISTERED_ROOTS: &[&str] = &["meta", "sim", "content", "ai", "render", "input", "runtime"];

/// Load + validate the shipped config through the real loader.
pub fn check_config_validates() -> Result<(), String> {
    let config_dir = crate::workspace_root().join("config");
    providence_config_loader::load_dir(&config_dir)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Namespace conformance + file↔schema key integrity.
pub fn check_keys() -> Result<(), String> {
    let root = crate::workspace_root();
    let schema_keys = schema_leaf_keys(&root)?;
    let mut violations = Vec::new();

    for (path, is_default_layer) in config_files(&root)? {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let value: toml::Value = toml::from_str(&text)
            .map_err(|error| format!("{} is not valid TOML: {error}", path.display()))?;
        let file_keys = dotted_leaf_keys(&value);
        let display = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into());

        for key in &file_keys {
            let mut segments = key.split('.');
            let key_root = segments.next().unwrap_or_default();
            if !REGISTERED_ROOTS.contains(&key_root) {
                violations.push(format!(
                    "{display}: key `{key}` is outside the registered namespace roots \
                     {REGISTERED_ROOTS:?} (40-parameterisation §2.2)"
                ));
            }
            for segment in key.split('.') {
                if !is_lower_snake_case(segment) {
                    violations.push(format!(
                        "{display}: key `{key}` segment `{segment}` is not lower_snake_case \
                         (40-parameterisation §2.3)"
                    ));
                }
            }
            if !schema_keys.contains(key) {
                violations.push(format!(
                    "{display}: key `{key}` does not exist in the schema (40-parameterisation §6.3)"
                ));
            }
        }

        if is_default_layer {
            for schema_key in &schema_keys {
                if !file_keys.contains(schema_key) {
                    violations.push(format!(
                        "{display}: schema key `{schema_key}` has no default value \
                         (orphan schema key, 40-parameterisation §6.3)"
                    ));
                }
            }
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n    "))
    }
}

/// All shipped `config/*.toml` files, flagging the default layer.
fn config_files(root: &std::path::Path) -> Result<Vec<(PathBuf, bool)>, String> {
    let config_dir = root.join("config");
    let mut files = Vec::new();
    let entries = fs::read_dir(&config_dir)
        .map_err(|error| format!("cannot read {}: {error}", config_dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot list config dir: {error}"))?
            .path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            let is_default = path.file_name().is_some_and(|name| name == "default.toml");
            files.push((path, is_default));
        }
    }
    if files.iter().any(|(_, is_default)| *is_default) {
        Ok(files)
    } else {
        Err("config/default.toml is missing (the built-in defaults layer, ADR 0008)".into())
    }
}

/// Leaf keys of the committed JSON Schema as dotted paths. The schema is
/// generated with inlined subschemas, so nesting is plain `properties`.
fn schema_leaf_keys(root: &std::path::Path) -> Result<BTreeSet<String>, String> {
    let schema_path = root.join(crate::schema::SCHEMA_ARTIFACT);
    let text = fs::read_to_string(&schema_path)
        .map_err(|error| format!("cannot read {}: {error}", schema_path.display()))?;
    let schema: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("{} is not valid JSON: {error}", schema_path.display()))?;
    let mut keys = BTreeSet::new();
    collect_schema_keys(&schema, String::new(), &mut keys);
    if keys.is_empty() {
        return Err("schema artifact yielded no keys — is it inlined correctly?".into());
    }
    Ok(keys)
}

fn collect_schema_keys(node: &serde_json::Value, prefix: String, keys: &mut BTreeSet<String>) {
    match node
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        Some(properties) => {
            for (name, child) in properties {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                collect_schema_keys(child, path, keys);
            }
        }
        None => {
            if !prefix.is_empty() {
                keys.insert(prefix);
            }
        }
    }
}

/// Leaf keys of a TOML document as dotted paths. Arrays are treated as
/// leaves for now; keyed content tables get richer handling when
/// `content.*` lands (40-parameterisation §3).
fn dotted_leaf_keys(value: &toml::Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    collect_toml_keys(value, String::new(), &mut keys);
    keys
}

fn collect_toml_keys(value: &toml::Value, prefix: String, keys: &mut BTreeSet<String>) {
    match value {
        toml::Value::Table(table) => {
            for (name, child) in table {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                collect_toml_keys(child, path, keys);
            }
        }
        _ => {
            if !prefix.is_empty() {
                keys.insert(prefix);
            }
        }
    }
}

fn is_lower_snake_case(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
