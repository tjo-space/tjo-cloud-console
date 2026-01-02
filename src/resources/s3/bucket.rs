use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
