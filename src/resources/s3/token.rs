use crate::{
    BucketPermissions, Context, Error, FINALIZER, Result,
    resources::s3::bucket::{Bucket, BucketRef},
    telemetry,
};
use chrono::Utc;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    CustomResource, Resource,
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client as KubeClient,
    core::ObjectMeta,
    core::object::HasSpec,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType},
        finalizer::{Event as Finalizer, finalizer},
        watcher::Config,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::*;

/// Represents a token to access bucket in s3.tjo.cloud.
///
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "Token",
    group = "s3.tjo.cloud",
    version = "v1",
    namespaced,
    shortname = "tok",
    status = "TokenStatus"
)]
pub struct TokenSpec {
    #[allow(non_snake_case)]
    pub bucketRef: BucketRef,
    pub tokenSecretName: String,
    pub name: String,
    pub reader: bool,
    pub writer: bool,
    pub owner: bool,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct TokenStatus {
    pub created: bool,
    pub id: String,
}

impl Token {
    fn was_created(&self) -> bool {
        self.status.as_ref().map(|s| s.created).unwrap_or(false)
    }

    fn get_id(&self) -> String {
        self.status
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or("".to_string())
    }

    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let garage_client = ctx.garage_client.clone();
        let oref = self.object_ref(&());
        let namespace = self.namespace().unwrap();
        let name = self.name_any();
        let tokens: Api<Token> = Api::namespaced(ctx.kube_client.clone(), &namespace);
        let buckets: Api<Bucket> = Api::namespaced(ctx.kube_client.clone(), &namespace);
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
                    note: Some(format!("Creating token for `{name}`")),
                    action: "Creating".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let key = garage_client
            .create_key(self.spec().name.clone())
            .await
            .map_err(Error::GarageClientError)?;

        let secret = Secret {
            metadata: ObjectMeta {
                name: Some(self.spec().tokenSecretName.clone()),
                owner_references: Some(self.owner_ref(&()).into_iter().collect()),
                ..Default::default()
            },
            immutable: Some(true),
            string_data: Some(std::collections::BTreeMap::from([
                ("accessKeyId".to_string(), key.id.clone()),
                ("secretAccessKey".to_string(), key.secret.clone()),
            ])),
            ..Default::default()
        };

        secrets
            .create(&kube::api::PostParams::default(), &secret)
            .await
            .map_err(Error::KubeError)?;

        let bucket = buckets
            .get(&self.spec().bucketRef.name)
            .await
            .map_err(Error::KubeError)?;

        garage_client
            .clone()
            .set_bucket_permissions(
                bucket.get_id(),
                key.id.clone(),
                BucketPermissions {
                    read: self.spec().reader,
                    write: self.spec().writer,
                    owner: self.spec().owner,
                },
            )
            .await
            .map_err(Error::GarageClientError)?;

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationCompleted".into(),
                    note: Some(format!("Created token for `{name}`")),
                    action: "Created".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let new_status = Patch::Apply(json!({
            "apiVersion": "s3.tjo.cloud/v1",
            "kind": "Token",
            "status": TokenStatus {
                created : true,
                id : key.id,
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        tokens
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    pub async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let garage_client = ctx.garage_client.clone();
        let oref = self.object_ref(&());

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "DeleteRequested".into(),
                    note: Some(format!("Deleting token for `{}`", self.name_any())),
                    action: "Deleting".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        garage_client
            .delete_key(self.get_id())
            .await
            .map_err(Error::GarageClientError)?;

        Ok(Action::await_change())
    }
}

#[instrument(skip(ctx, token), fields(trace_id))]
async fn reconcile(token: Arc<Token>, ctx: Arc<Context>) -> Result<Action> {
    let oref = token.object_ref(&());

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
    let ns = token.namespace().unwrap();
    let tokens: Api<Token> = Api::namespaced(ctx.kube_client.clone(), &ns);

    info!("Reconciling Token \"{}\" in {}", token.name_any(), ns);
    finalizer(&tokens, FINALIZER, token, |event| async {
        match event {
            Finalizer::Apply(token) => token.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(token) => token.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

fn error_policy(token: Arc<Token>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    let oref = token.object_ref(&());

    ctx.metrics.reconcile.set_failure(
        oref.api_version.unwrap(),
        oref.kind.unwrap(),
        token.name_any(),
        error,
    );
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(context: Arc<Context>, kube_client: KubeClient) -> Result<(), Error> {
    let tokens = Api::<Token>::all(kube_client.clone());
    if tokens.list(&ListParams::default().limit(1)).await.is_err() {
        return Err(Error::MissingCrds);
    }

    info!("Starting controller");

    Controller::new(tokens, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, context)
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;

    Ok(())
}
