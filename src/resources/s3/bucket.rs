use crate::{telemetry, Context, Error, Result, FINALIZER};
use chrono::Utc;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client as KubeClient,
    core::object::HasSpec,
    core::ObjectMeta,
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
use std::sync::Arc;
use tokio::time::Duration;
use tracing::*;

/// Represents a bucket in s3.tjo.cloud.
///
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "Bucket",
    group = "s3.tjo.cloud",
    version = "v1",
    namespaced,
    shortname = "buc",
    status = "BucketStatus"
)]
pub struct BucketSpec {
    #[schemars(length(min = 3, max = 63), pattern(r"[a-z0-9.-_]+"))]
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct BucketRef {
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct BucketStatus {
    pub id: String,
    pub created: bool,
}

impl Bucket {
    fn was_created(&self) -> bool {
        self.status.as_ref().map(|s| s.created).unwrap_or(false)
    }

    pub fn get_id(&self) -> String {
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
        let buckets: Api<Bucket> = Api::namespaced(ctx.kube_client.clone(), &namespace);

        // If was already created, do nothing.
        if self.was_created() {
            return Ok(Action::requeue(Duration::from_secs(5 * 60)));
        }

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationRequested".into(),
                    note: Some(format!("Creating bucket for `{name}`")),
                    action: "Creating".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let bucket = garage_client
            .create_bucket(self.spec().name.clone())
            .await
            .map_err(Error::GarageClientError)?;

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "CreationCompleted".into(),
                    note: Some(format!("Created bucket for `{name}`")),
                    action: "Created".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let new_status = Patch::Apply(json!({
            "apiVersion": "s3.tjo.cloud/v1",
            "kind": "Bucket",
            "status": BucketStatus {
                created : true,
                id : bucket.id,
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        buckets
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
                    note: Some(format!("Deleting bucket for `{}`", self.name_any())),
                    action: "Deleting".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        garage_client
            .delete_bucket(self.get_id())
            .await
            .map_err(Error::GarageClientError)?;

        Ok(Action::await_change())
    }
}

#[instrument(skip(ctx, bucket), fields(trace_id))]
async fn reconcile(bucket: Arc<Bucket>, ctx: Arc<Context>) -> Result<Action> {
    let oref = bucket.object_ref(&());

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
    let ns = bucket.namespace().unwrap();
    let buckets: Api<Bucket> = Api::namespaced(ctx.kube_client.clone(), &ns);

    info!("Reconciling Bucket \"{}\" in {}", bucket.name_any(), ns);
    finalizer(&buckets, FINALIZER, bucket, |event| async {
        match event {
            Finalizer::Apply(bucket) => bucket.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(bucket) => bucket.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

fn error_policy(bucket: Arc<Bucket>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    let oref = bucket.object_ref(&());

    ctx.metrics.reconcile.set_failure(
        oref.api_version.unwrap(),
        oref.kind.unwrap(),
        bucket.name_any(),
        error,
    );
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(context: Arc<Context>, kube_client: KubeClient) -> Result<(), Error> {
    let buckets = Api::<Bucket>::all(kube_client.clone());
    if buckets.list(&ListParams::default().limit(1)).await.is_err() {
        return Err(Error::MissingCrds);
    }

    info!("Starting controller");

    Controller::new(buckets, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, context)
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;

    Ok(())
}
