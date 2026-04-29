//! Error types for the veilbreak core library.
//!
//! Each submodule defines its own error enum. The top-level [`Error`] wraps
//! them for callers that want a single error type.

use thiserror::Error;

/// Top-level error type for the core library.
#[derive(Debug, Error)]
pub enum Error {
    /// An error from the interface module.
    #[error(transparent)]
    Interface(#[from] InterfaceError),

    /// An error from the airodump module.
    #[error(transparent)]
    Airodump(#[from] AirodumpError),

    /// An error from the tshark module.
    #[error(transparent)]
    Tshark(#[from] TsharkError),

    /// An error from the aireplay module.
    #[error(transparent)]
    Aireplay(#[from] AireplayError),
}

/// Errors from wireless interface detection and management.
#[derive(Debug, Error)]
pub enum InterfaceError {
    /// Failed to execute `iw`.
    #[error("failed to execute `iw`: {0}")]
    IwExecution(#[source] std::io::Error),

    /// `iw` exited with a non-zero status.
    #[error("`iw` exited with status {status}: {stderr}")]
    IwFailed {
        /// Process exit code.
        status: i32,
        /// Captured stderr.
        stderr: String,
    },

    /// Failed to parse `iw` output.
    #[error("failed to parse `iw` output: {0}")]
    Parse(String),

    /// No wireless interfaces were found on this system.
    #[error("no wireless interfaces found")]
    NoInterfaces,

    /// A required tool is missing from PATH.
    #[error("required tool not found: {0}")]
    MissingTool(String),
}

/// Errors from airodump-ng operations.
#[derive(Debug, Error)]
pub enum AirodumpError {
    /// Failed to spawn airodump-ng.
    #[error("failed to spawn airodump-ng: {0}")]
    Spawn(#[source] std::io::Error),

    /// Failed to read or parse the CSV output file.
    #[error("failed to parse airodump CSV: {0}")]
    CsvParse(String),
}

/// Errors from tshark operations.
#[derive(Debug, Error)]
pub enum TsharkError {
    /// Failed to spawn tshark.
    #[error("failed to spawn tshark: {0}")]
    Spawn(#[source] std::io::Error),

    /// Tshark exited with a non-zero status.
    #[error("tshark exited with status {status}: {stderr}")]
    Failed {
        /// Process exit code.
        status: i32,
        /// Captured stderr (truncated, sanitized).
        stderr: String,
    },

    /// Failed to parse tshark output.
    #[error("failed to parse tshark output: {0}")]
    Parse(String),
}

/// Errors from aireplay-ng operations.
#[derive(Debug, Error)]
pub enum AireplayError {
    /// Failed to spawn aireplay-ng.
    #[error("failed to spawn aireplay-ng: {0}")]
    Spawn(#[source] std::io::Error),
}
