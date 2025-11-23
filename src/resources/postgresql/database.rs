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
    pub location: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct DatabaseStatus {
    pub created: bool,
}
