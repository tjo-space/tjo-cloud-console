use crate::{
    resources::postgresql::Client as PostgresqlClient, telemetry, Context, Error, Result, State,
    FINALIZER,
};
use chrono::Utc;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client as KubeClient,
    core::ObjectMeta,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType},
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
    CustomResource, Resource,
};
use rand::distr::{Alphanumeric, SampleString};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::*;

/// User on the postgresql.tjo.cloud database platform
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "User",
    group = "postgresql.tjo.cloud",
    version = "v1",
    namespaced,
    shortname = "user",
    status = "UserStatus"
)]
pub struct UserSpec {
    pub server: String,
    /// Name of the secret that will be created and contain the generated password.
    pub password_secret_name: String,
    pub connection_limit: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct UserRef {
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct UserStatus {
    pub created: bool,
    pub name: String,
}

impl User {
    fn was_created(&self) -> bool {
        self.status.as_ref().map(|s| s.created).unwrap_or(false)
    }

    fn name(&self) -> String {
        let namespace = self.namespace().unwrap();
        let name = self.name_any();

        format!("{}_{}", namespace, name)
    }

    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let oref = self.object_ref(&());
        let namespace = self.namespace().unwrap();
        let name = self.name_any();
        let users: Api<User> = Api::namespaced(ctx.kube_client.clone(), &namespace);
        let secrets: Api<Secret> = Api::namespaced(ctx.kube_client.clone(), &namespace);

        // If was already created, do nothing.
        if self.was_created() {
            return Ok(Action::requeue(Duration::from_secs(5 * 60)));
        }

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationRequested".into(),
                    note: Some(format!("Creating user for `{name}`")),
                    action: "Creating".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        if name == "illegal" {
            return Err(Error::PostgresqlIllegalUser);
        }

        let user: User = users.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&user.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        let password = Alphanumeric.sample_string(&mut rand::rng(), 16);

        ctx.postgresql_clients[&user.spec.server]
            .execute(
                &format!(
                    "CREATE USER {} WITH PASSWORD '{}' CONNECTION LIMIT {}",
                    user.name(),
                    password,
                    user.spec.connection_limit
                ),
                &[],
            )
            .await?;

        let server_host_name = ctx.settings.postgresql[&user.spec.server].host.clone();

        let secret = Secret {
            metadata: ObjectMeta {
                name: Some(user.spec.password_secret_name.clone()),
                ..Default::default()
            },
            immutable: Some(true),
            string_data: Some(std::collections::BTreeMap::from([
                ("password".to_string(), password),
                ("username".to_string(), user.name()),
                ("host".to_string(), server_host_name),
                ("port".to_string(), "5432".to_string()),
            ])),
            ..Default::default()
        };

        secrets
            .create(&kube::api::PostParams::default(), &secret)
            .await
            .map_err(Error::KubeError)?;

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationCompleted".into(),
                    note: Some(format!("Created user for `{name}`")),
                    action: "Created".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let new_status = Patch::Apply(json!({
            "apiVersion": "postgresql.tjo.cloud/v1",
            "kind": "User",
            "status": UserStatus {
                created : true,
                name: user.name(),
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        users
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
        let users: Api<User> = Api::namespaced(client, &ns);

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "DeleteRequested".into(),
                    note: Some(format!("Dropping user for `{}`", self.name_any())),
                    action: "Deleting".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let user: User = users.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&user.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        ctx.postgresql_clients[&user.spec.server]
            .execute(&format!("DROP USER {}", user.name()), &[])
            .await?;

        Ok(Action::await_change())
    }
}

#[instrument(skip(ctx, user), fields(trace_id))]
async fn reconcile(user: Arc<User>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    if trace_id != opentelemetry::trace::TraceId::INVALID {
        Span::current().record("trace_id", field::display(&trace_id));
    }
    let _timer = ctx.metrics.reconcile.count_and_measure(&trace_id);
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = user.namespace().unwrap();
    let users: Api<User> = Api::namespaced(ctx.kube_client.clone(), &ns);

    info!("Reconciling User \"{}\" in {}", user.name_any(), ns);
    finalizer(&users, FINALIZER, user, |event| async {
        match event {
            Finalizer::Apply(user) => user.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(user) => user.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

fn error_policy(user: Arc<User>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.reconcile.set_failure(user.name_any(), error);
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(
    state: State,
    kube_client: KubeClient,
    postgresql_clients: Arc<HashMap<String, PostgresqlClient>>,
) -> Result<(), Error> {
    let users = Api::<User>::all(kube_client.clone());
    if users.list(&ListParams::default().limit(1)).await.is_err() {
        return Err(Error::MissingCrds);
    }

    info!("Starting postgresql::user controller");

    Controller::new(users, Config::default().any_semantic())
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
