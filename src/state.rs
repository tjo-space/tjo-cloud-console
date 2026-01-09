use crate::{
    Context, GarageClient, Metrics, Settings, resources::postgresql::Client as PostgresqlClient,
};
use chrono::{DateTime, Utc};
use kube::{
    client::Client as KubeClient,
    runtime::events::{Recorder, Reporter},
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Diagnostics to be exposed by the web server
#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "console.tjo.cloud".into(),
        }
    }
}
impl Diagnostics {
    fn recorder(&self, client: KubeClient) -> Recorder {
        Recorder::new(client, self.reporter.clone())
    }
}

/// State shared between the controller and the web server
#[derive(Clone)]
pub struct State {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
    /// Metrics
    metrics: Arc<Metrics>,
    /// Settings
    settings: Arc<Settings>,
}

/// State wrapper around the controller outputs for the web server
impl State {
    pub fn new(settings: Settings) -> State {
        State {
            settings: Arc::new(settings),
            diagnostics: Arc::new(RwLock::new(Diagnostics::default())),
            metrics: Arc::new(Metrics::default()),
        }
    }

    /// Settings getter
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Metrics getter
    pub fn metrics(&self) -> String {
        let mut buffer = String::new();
        let registry = &*self.metrics.registry;
        prometheus_client::encoding::text::encode(&mut buffer, registry).unwrap();
        buffer
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }

    // Create a Controller Context that can update State
    pub async fn to_context(
        &self,
        kube_client: KubeClient,
        postgresql_clients: Arc<HashMap<String, PostgresqlClient>>,
        garage_client: Arc<GarageClient>,
    ) -> Arc<Context> {
        Arc::new(Context {
            kube_client: kube_client.clone(),
            recorder: self.diagnostics.read().await.recorder(kube_client),
            metrics: self.metrics.clone(),
            diagnostics: self.diagnostics.clone(),
            settings: self.settings.clone(),
            garage_client,
            postgresql_clients,
        })
    }
}
