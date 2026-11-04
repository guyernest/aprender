//! The pmcp server over one configuration (design §4): three tools, nothing loaded at build.

use std::fmt;
use std::sync::Arc;

use aprender_mcp_decide::TOOL_NAME;
use aprender_mcp_decide_lambda::ModelSource;
use pmcp::{Server, ServerCapabilities};

use crate::config::{self, Config};
use crate::load::{source_from_config, LoadFailure, ModelCell, MODEL};
use crate::store::ObjectStore;
use crate::tools;

/// Everything the tools share: the configuration, where the model comes from, where the
/// records live, the once-per-process cell, and the build identity.
#[derive(Clone)]
pub struct Runtime {
    pub config: Arc<Config>,
    pub source: ModelSource,
    pub store: Arc<dyn ObjectStore>,
    pub cell: &'static ModelCell,
    pub config_sha256: String,
    pub build_id: Option<String>,
}

impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("server", &self.config.id)
            .field("source", &self.source.kind())
            .field("config_sha256", &self.config_sha256)
            .field("build_id", &self.build_id)
            .finish_non_exhaustive()
    }
}

/// Why a server could not be built.
#[derive(Debug)]
pub enum BuildError {
    /// The configuration's artifact source could not be constructed.
    Source(LoadFailure),
    /// The pmcp server builder refused.
    Server(pmcp::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(e) => write!(f, "model source ({}): {}", e.kind, e.detail),
            Self::Server(e) => write!(f, "server: {e}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Build the server over an explicit runtime (tests use a local source and their own cell).
///
/// # Errors
///
/// `pmcp::Error` if the server builder refuses.
pub fn build_with(rt: Runtime, server_name: &str, version: &str) -> pmcp::Result<Server> {
    Server::builder()
        .name(server_name)
        .version(version)
        .capabilities(ServerCapabilities::tools_only())
        .tool(TOOL_NAME, tools::classify_tool(rt.clone()))
        .tool("status", tools::status_tool(rt))
        .build()
}

/// Build the deployed server: the S3 source the configuration names, the process-wide cell,
/// the sha of the embedded TOML. Nothing is loaded here.
///
/// # Errors
///
/// A [`BuildError`].
pub fn build(
    toml_text: &str,
    config: Config,
    server_name: &str,
    version: &str,
    build_id: Option<String>,
    store: Arc<dyn ObjectStore>,
) -> Result<Server, BuildError> {
    let source = source_from_config(&config.model).map_err(BuildError::Source)?;
    let rt = Runtime {
        config: Arc::new(config),
        source,
        store,
        cell: &MODEL,
        config_sha256: config::config_sha256(toml_text),
        build_id,
    };
    build_with(rt, server_name, version).map_err(BuildError::Server)
}
