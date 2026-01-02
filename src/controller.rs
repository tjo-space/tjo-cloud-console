use crate::{resources, telemetry, Diagnostics, Error, Metrics, Result, Settings, State};
use chrono::Utc;
use futures::future::try_join_all;
use futures::StreamExt;
use kube::{
    api::{Api, ListParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        events::Recorder,
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Duration};
use tracing::*;

use resources::postgresql::{database::*, user::*};

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Event recorder
    pub recorder: Recorder,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Arc<Metrics>,
    /// Settings
    pub settings: Arc<Settings>,
    /// Postgresql Clients
    pub postgresql_clients: Arc<HashMap<String, resources::postgresql::Client>>,
}

#[instrument(skip(ctx, database), fields(trace_id))]
async fn reconcile(database: Arc<Database>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    if trace_id != opentelemetry::trace::TraceId::INVALID {
        Span::current().record("trace_id", field::display(&trace_id));
    }
    let _timer = ctx.metrics.reconcile.count_and_measure(&trace_id);
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = database.namespace().unwrap(); // database is namespace scoped
    let databases: Api<Database> = Api::namespaced(ctx.client.clone(), &ns);

    info!("Reconciling Database \"{}\" in {}", database.name_any(), ns);
    finalizer(&databases, DATABASE_FINALIZER, database, |event| async {
        match event {
            Finalizer::Apply(database) => database.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(database) => database.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

fn error_policy(database: Arc<Database>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics
        .reconcile
        .set_failure(database.name_any(), error);
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
/// FIXME(tine): move this logic to resources/postgresql
///              and create a copy for resources/s3.
pub async fn run(state: State) {
    let kube_client = Client::try_default()
        .await
        .expect("failed to create kube Client");

    let databases = Api::<Database>::all(kube_client.clone());
    if let Err(e) = databases.list(&ListParams::default().limit(1)).await {
        error!("CRD is not queryable; {e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }

    let postgresql_clients: HashMap<String, resources::postgresql::Client> =
        try_join_all(state.settings().postgresql().iter().map(|(k, v)| async {
            let key = k.clone();
            let client = resources::postgresql::connect(
                key.clone(),
                v.host.clone(),
                v.user.clone(),
                v.password.clone(),
                v.sslmode.clone(),
            )
            .await?;

            Ok::<(String, resources::postgresql::Client), Error>((key, client))
        }))
        .await
        .expect("failed to connect to postgresql server")
        .into_iter()
        .collect();

    Controller::new(databases, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(
            reconcile,
            error_policy,
            state.to_context(kube_client, postgresql_clients).await,
        )
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}
