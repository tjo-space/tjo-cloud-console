use crate::{resources::postgresql::Client as PostgresqlClient, Diagnostics, Metrics, Settings};
use kube::runtime::events::Recorder;
use kube::Client as KubeClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub kube_client: KubeClient,
    /// Event recorder
    pub recorder: Recorder,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Arc<Metrics>,
    /// Settings
    pub settings: Arc<Settings>,
    /// Postgresql Clients
    pub postgresql_clients: Arc<HashMap<String, PostgresqlClient>>,
}
