use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    pub created: bool,
}
