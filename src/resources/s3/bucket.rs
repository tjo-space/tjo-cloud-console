use futures::{StreamExt, TryStreamExt};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{Api, DeleteParams, Patch, PatchParams, ResourceExt},
    core::CustomResourceExt,
    runtime::{
        wait::{await_condition, conditions},
        watcher, WatchStreamExt,
    },
    Client, CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub static BUCKET_FINALIZER: &str = "bucket.s3.tjo.cloud";

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
pub struct BucketSpec {}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct BucketStatus {
    pub created: bool,
}
