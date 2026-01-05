use crate::{
    resources::postgresql::user::{User, UserRef},
    telemetry, Context, Error, Result, FINALIZER,
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
use tokio::time::Duration;
use tracing::*;

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
    #[schemars(length(min = 3, max = 63), pattern(r"[a-z0-9._]+"))]
    pub name: String,
    pub server: String,
    pub connectionLimit: i32,
    pub ownerRef: UserRef,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct DatabaseRef {
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct DatabaseStatus {
    pub created: bool,
}

impl Database {
    fn was_created(&self) -> bool {
        self.status.as_ref().map(|s| s.created).unwrap_or(false)
    }

    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let oref = self.object_ref(&());
        let namespace = self.namespace().unwrap();
        let name = self.name_any();
        let databases: Api<Database> = Api::namespaced(ctx.kube_client.clone(), &namespace);
        let users: Api<User> = Api::namespaced(ctx.kube_client.clone(), &namespace);

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
            return Err(Error::PostgresqlIllegalDatabase);
        }

        let database: Database = databases.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&database.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        let user: User = users
            .get(&database.spec.ownerRef.name)
            .await
            .map_err(Error::KubeError)?;

        if user.spec.server != database.spec.server {
            return Err(Error::PostgresqlUserAndDatabaseServerNotMatching);
        }

        ctx.postgresql_clients[&database.spec.server]
            .execute(
                &format!(
                    "CREATE DATABASE {} WITH OWNER '{}' CONNECTION LIMIT {}",
                    database.spec.name.clone(),
                    user.spec.name,
                    database.spec.connectionLimit
                ),
                &[],
            )
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

        let database: Database = databases.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&database.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        ctx.postgresql_clients[&database.spec.server]
            .execute(
                &format!("DROP DATABASE {}", database.spec.name.clone()),
                &[],
            )
            .await?;

        Ok(Action::await_change())
    }
}

#[instrument(skip(ctx, database), fields(trace_id))]
async fn reconcile(database: Arc<Database>, ctx: Arc<Context>) -> Result<Action> {
    let oref = database.object_ref(&());

    let trace_id = telemetry::get_trace_id();
    if trace_id != opentelemetry::trace::TraceId::INVALID {
        Span::current().record("trace_id", field::display(&trace_id));
    }
    let _timer = ctx.metrics.reconcile.count_and_measure(
        oref.api_version.unwrap(),
        oref.kind.unwrap(),
        &trace_id,
    );
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = database.namespace().unwrap();
    let databases: Api<Database> = Api::namespaced(ctx.kube_client.clone(), &ns);

    info!("Reconciling Database \"{}\" in {}", database.name_any(), ns);
    finalizer(&databases, FINALIZER, database, |event| async {
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
    let oref = database.object_ref(&());

    ctx.metrics.reconcile.set_failure(
        oref.api_version.unwrap(),
        oref.kind.unwrap(),
        database.name_any(),
        error,
    );
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(context: Arc<Context>, kube_client: KubeClient) -> Result<(), Error> {
    let databases = Api::<Database>::all(kube_client.clone());
    if databases
        .list(&ListParams::default().limit(1))
        .await
        .is_err()
    {
        return Err(Error::MissingCrds);
    }

    info!("Starting controller");

    Controller::new(databases, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, context)
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;

    Ok(())
}
