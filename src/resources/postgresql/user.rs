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

pub static USER_FINALIZER: &str = "user.postgresql.tjo.cloud";

/// User on the postgresql.tjo.cloud database platform
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "User",
    group = "postgresql.tjo.cloud",
    version = "v1",
    namespaced,
    shortname = "dat",
    status = "UserStatus"
)]
pub struct UserSpec {
    #[schemars(length(min = 3, max = 63))]
    pub name: String,
    pub databaseRef: String, // TODO: Link to existing Database resource?
    pub secretRef: String,   // TODO: Link to existing Secret where the password will be stored in.
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct UserStatus {
    pub created: bool,
}
