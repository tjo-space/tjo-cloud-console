use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),

    #[error("Kube Error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("CRDS are not installed in cluster")]
    MissingCrds,

    #[error("Finalizer Error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<Error>>),

    #[error("Controller Error: {0}")]
    // NB: Same as above?
    ControllerError(#[source] Box<kube::runtime::controller::Error<Error, Error>>),

    #[error("IllegalDocument")]
    IllegalDocument,

    #[error("postgresql client error")]
    PostgresqlClientError(#[from] tokio_postgres::Error),

    #[error("tls error")]
    TlsError(#[from] native_tls::Error),

    #[error("PostgresqlIllegalDatabase")]
    PostgresqlIllegalDatabase,

    #[error("PostgresqlIllegalUser")]
    PostgresqlIllegalUser,

    #[error("PostgresqlUnknownServer")]
    PostgresqlUnknownServer,

    #[error("PostgresqlUserAndDatabaseServerNotMatching")]
    PostgresqlUserAndDatabaseServerNotMatching,

    #[error("StdIoError")]
    StdIoError(#[from] std::io::Error),

    #[error("GarageClientError")]
    GarageClientError(#[from] crate::garage::Error),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn metric_label(&self) -> String {
        format!("{self:?}").to_lowercase()
    }
}

/// Expose all controller components used by main
pub mod controller;
pub use crate::controller::*;

/// Log and trace integrations
pub mod telemetry;

/// Metrics
mod metrics;
pub use metrics::Metrics;

/// Resources
pub mod resources;

/// Settings
mod settings;
pub use settings::Settings;

/// State
mod state;
pub use state::*;

/// Context
mod context;
pub use context::*;

/// Garage Client
mod garage;
pub use garage::*;
