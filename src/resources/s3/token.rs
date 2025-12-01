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

pub static TOKEN_FINALIZER: &str = "token.s3.tjo.cloud";

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
    #[schemars(length(min = 3, max = 63))]
    pub name: String,
    pub location: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct TokenStatus {
    pub created: bool,
}
