use crate::{
    resources::postgresql::Client as PostgresqlClient, telemetry, Context, Error, Result, State,
};
use chrono::Utc;
use futures::StreamExt;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client as KubeClient,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType},
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
    CustomResource, Resource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::*;

pub static DATABASE_FINALIZER: &str = "database.postgresql.tjo.cloud";

/// Database on the postgresql.tjo.cloud database platform
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "Database",
    group = "postgresql.tjo.cloud",
    version = "v1",
    namespaced,
    shortname = "dat",
    status = "DatabaseStatus"
)]
pub struct DatabaseSpec {
    #[schemars(length(min = 3, max = 63))]
    pub name: String,
    pub server: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct DatabaseStatus {
    pub created: bool,
}

impl Database {
    fn was_created(&self) -> bool {
        self.status.as_ref().map(|s| s.created).unwrap_or(false)
    }

    // Reconcile (for non-finalizer related changes)
    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.kube_client.clone();
        let oref = self.object_ref(&());
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let databases: Api<Database> = Api::namespaced(client, &ns);

        // If was already created, do nothing.
        if self.was_created() {
            return Ok(Action::requeue(Duration::from_secs(5 * 60)));
        }

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationRequested".into(),
                    note: Some(format!("Creating database for `{name}`")),
                    action: "Creating".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        if name == "illegal" {
            return Err(Error::PostgresqlIllegalDatabase); // error names show up in metrics
        }

        let object: Database = databases.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&object.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        ctx.postgresql_clients[&object.spec.server]
            .execute(&format!("CREATE DATABASE {}", object.spec.name), &[])
            .await?;

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationCompleted".into(),
                    note: Some(format!("Created database for `{name}`")),
                    action: "Created".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        // always overwrite status object with what we saw
        let new_status = Patch::Apply(json!({
            "apiVersion": "postgresql.tjo.cloud/v1",
            "kind": "Database",
            "status": DatabaseStatus {
                created : true,
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        databases
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    pub async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.kube_client.clone();
        let oref = self.object_ref(&());
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let databases: Api<Database> = Api::namespaced(client, &ns);

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "DeleteRequested".into(),
                    note: Some(format!("Dropping database for `{}`", self.name_any())),
                    action: "Deleting".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let object: Database = databases.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&object.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        ctx.postgresql_clients[&object.spec.server]
            .execute(&format!("DROP DATABASE {}", object.spec.name), &[])
            .await?;

        Ok(Action::await_change())
    }
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
    let databases: Api<Database> = Api::namespaced(ctx.kube_client.clone(), &ns);

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
pub async fn run(
    state: State,
    kube_client: KubeClient,
    postgresql_clients: HashMap<String, PostgresqlClient>,
) -> Result<(), Error> {
    let databases = Api::<Database>::all(kube_client.clone());
    if databases
        .list(&ListParams::default().limit(1))
        .await
        .is_err()
    {
        return Err(Error::MissingCrds);
    }

    info!("Starting postgresql::database controller");

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

    Ok(())
}
