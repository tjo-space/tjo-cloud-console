use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    pub bucketRef: String, // Reference to bucket object, to which this token will have access to.
    pub reader: bool,
    pub writer: bool,
    pub owner: bool,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct TokenStatus {
    pub created: bool,
}
