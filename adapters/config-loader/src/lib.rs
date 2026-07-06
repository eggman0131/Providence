//! Config-loading adapter (ADR 0008, refined by ADR 0009).
//!
//! Pipeline (docs/40-parameterisation.md §4–5): parse each TOML layer →
//! deep-merge by key (defaults → scenario/content pack → user/local) →
//! check `meta.schema_version` → deserialise the merged whole into the
//! authoring structs (`deny_unknown_fields` rejects stray keys) → `garde`
//! semantic validation → map into the immutable `no_std`
//! [`providence_config::Params`] the core consumes.
//!
//! The authoring structs here are the single source of truth for the
//! machine-readable JSON Schema (`docs/contracts/config.schema.json`),
//! generated via [`schema_json`] and kept drift-free by the gate's
//! regenerate-and-diff check.

#![forbid(unsafe_code)]

mod authoring;
mod error;
mod merge;

use std::fs;
use std::path::Path;

use garde::Validate;
use providence_config::Params;

pub use crate::authoring::ConfigRoot;
pub use crate::error::ConfigError;

/// The schema version this loader supports. A `meta.schema_version` mismatch
/// is a defined migration point, never a silent misread
/// (docs/40-parameterisation.md §4).
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// One configuration layer: a display name (for error messages) and its
/// TOML text. Layers merge in slice order; later overrides earlier by key.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Where this layer came from (file name or description) — used in errors.
    pub name: String,
    /// The layer's TOML source text.
    pub text: String,
}

/// The generated JSON Schema for the full configuration, pretty-printed.
///
/// Committed at `docs/contracts/config.schema.json`; the gate regenerates
/// and diffs it so the schema can never drift from these types (ADR 0008).
#[must_use]
pub fn schema_json() -> String {
    // Subschemas are inlined (no `$ref`) so the artifact is directly
    // walkable by the gate's key-integrity check and by editors.
    let generator = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    let schema = generator.into_root_schema_for::<ConfigRoot>();
    let mut text = serde_json::to_string_pretty(&schema)
        .expect("schema serialisation cannot fail for a schemars-derived type");
    text.push('\n');
    text
}

/// Load, merge, and fully validate the layers, returning immutable params.
pub fn params_from_layers(layers: &[Layer]) -> Result<Params, ConfigError> {
    let root = config_root_from_layers(layers)?;
    Ok(root.into_params())
}

/// Load, merge, and fully validate the layers, returning the authoring root
/// (used by the gate's config checks; games use [`params_from_layers`]).
pub fn config_root_from_layers(layers: &[Layer]) -> Result<ConfigRoot, ConfigError> {
    if layers.is_empty() {
        return Err(ConfigError::NoLayers);
    }
    let mut merged: Option<toml::Value> = None;
    for layer in layers {
        let value: toml::Value =
            toml::from_str(&layer.text).map_err(|source| ConfigError::Parse {
                layer: layer.name.clone(),
                source,
            })?;
        match merged.as_mut() {
            Some(base) => merge::deep_merge(base, value),
            None => merged = Some(value),
        }
    }
    let merged = merged.expect("at least one layer checked above");

    let root: ConfigRoot = merged
        .try_into()
        .map_err(|source| ConfigError::Deserialize { source })?;

    if root.meta.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ConfigError::SchemaVersion {
            found: root.meta.schema_version,
            supported: SUPPORTED_SCHEMA_VERSION,
        });
    }

    root.validate()
        .map_err(|report| ConfigError::Validation { report })?;
    Ok(root)
}

/// Load params from a config directory: `default.toml` (required) overlaid
/// by `local.toml` (optional user layer).
pub fn load_dir(dir: &Path) -> Result<Params, ConfigError> {
    let mut layers = Vec::new();
    let default_path = dir.join("default.toml");
    let default_text = fs::read_to_string(&default_path).map_err(|source| ConfigError::Io {
        path: default_path.clone(),
        source,
    })?;
    layers.push(Layer {
        name: default_path.display().to_string(),
        text: default_text,
    });

    let local_path = dir.join("local.toml");
    if local_path.exists() {
        let local_text = fs::read_to_string(&local_path).map_err(|source| ConfigError::Io {
            path: local_path.clone(),
            source,
        })?;
        layers.push(Layer {
            name: local_path.display().to_string(),
            text: local_text,
        });
    }

    params_from_layers(&layers)
}

#[cfg(test)]
mod tests {
    use super::{Layer, SUPPORTED_SCHEMA_VERSION, params_from_layers};

    fn default_layer() -> Layer {
        Layer {
            name: "default.toml".into(),
            text: format!(
                "[meta]\nschema_version = {SUPPORTED_SCHEMA_VERSION}\n\n\
                 [sim.placeholder]\ntick_increment = 1\n"
            ),
        }
    }

    #[test]
    fn default_layer_alone_loads() {
        let params = params_from_layers(&[default_layer()]).expect("default layer must load");
        assert_eq!(params.sim.placeholder.tick_increment, 1);
    }

    #[test]
    fn later_layer_overrides_by_key() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.placeholder]\ntick_increment = 5\n".into(),
        };
        let params =
            params_from_layers(&[default_layer(), overlay]).expect("overlay merge must load");
        assert_eq!(
            params.sim.placeholder.tick_increment, 5,
            "later layers override earlier"
        );
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.placeholder]\nnot_a_real_key = 3\n".into(),
        };
        let err = params_from_layers(&[default_layer(), overlay])
            .expect_err("unknown keys must fail validation (40-parameterisation §2.4)");
        assert!(
            err.to_string().contains("not_a_real_key"),
            "error must name the offending key"
        );
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.placeholder]\ntick_increment = 0\n".into(),
        };
        params_from_layers(&[default_layer(), overlay])
            .expect_err("tick_increment below its minimum must fail garde validation");
    }

    #[test]
    fn schema_version_mismatch_is_a_migration_error() {
        let bad = Layer {
            name: "default.toml".into(),
            text: "[meta]\nschema_version = 999\n\n[sim.placeholder]\ntick_increment = 1\n".into(),
        };
        let err = params_from_layers(&[bad]).expect_err("version mismatch must be an error");
        assert!(
            err.to_string().contains("999"),
            "error must state found vs supported versions"
        );
    }
}
