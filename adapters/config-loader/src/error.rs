//! Loader errors — clear and actionable: which layer/file, which key,
//! expected vs actual (docs/40-parameterisation.md §4). Invalid config
//! never reaches the deterministic core.

use std::fmt;
use std::path::PathBuf;

/// Everything that can go wrong between TOML text and validated params.
#[derive(Debug)]
pub enum ConfigError {
    /// No layers were supplied at all.
    NoLayers,
    /// A layer file could not be read.
    Io {
        /// The file that failed to read.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A layer is not valid TOML.
    Parse {
        /// The layer (file name) that failed to parse.
        layer: String,
        /// The underlying TOML error (includes position information).
        source: toml::de::Error,
    },
    /// The merged config does not match the schema (unknown key, missing
    /// key, or wrong type).
    Deserialize {
        /// The underlying TOML deserialisation error (names the field).
        source: toml::de::Error,
    },
    /// `meta.schema_version` does not match this loader — migration needed.
    SchemaVersion {
        /// The version the config file declared.
        found: u32,
        /// The version this loader supports.
        supported: u32,
    },
    /// Values are shaped correctly but semantically invalid (out of range,
    /// cross-key invariant broken).
    Validation {
        /// The garde report; paths name the offending keys.
        report: garde::Report,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLayers => write!(f, "no configuration layers supplied"),
            Self::Io { path, source } => {
                write!(f, "cannot read config file {}: {source}", path.display())
            }
            Self::Parse { layer, source } => {
                write!(f, "config layer {layer} is not valid TOML: {source}")
            }
            Self::Deserialize { source } => {
                write!(f, "merged config does not match the schema: {source}")
            }
            Self::SchemaVersion { found, supported } => write!(
                f,
                "config targets schema_version {found} but this build supports {supported}; \
                 a migration is required (docs/40-parameterisation.md §4)"
            ),
            Self::Validation { report } => {
                write!(f, "config values failed validation: {report}")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } | Self::Deserialize { source } => Some(source),
            _ => None,
        }
    }
}
