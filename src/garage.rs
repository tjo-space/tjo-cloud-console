use awc::http::header;
use awc::http::StatusCode;
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub enum Error {
    #[error("StdIoError: {0}")]
    StdIoError(#[from] std::io::Error),

    #[error("RequestError: {0}")]
    RequestError(String),

    #[error("WrongResponseError: {0}")]
    WrongResponseError(#[from] awc::error::JsonPayloadError),

    #[error("BadStatusCodeError: {0}")]
    BadStatusCodeError(StatusCode),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone)]
pub struct GarageClient {
    token: String,
    address: String,
}

#[derive(Deserialize)]
pub struct Bucket {
    pub id: String,
}

#[derive(Deserialize)]
pub struct Key {
    pub name: String,
    #[serde(alias = "accessKeyId")]
    pub id: String,
    #[serde(alias = "secretAccessKey")]
    pub secret: String,
}

#[derive(Clone)]
pub struct BucketPermissions {
    pub owner: bool,
    pub read: bool,
    pub write: bool,
}

enum PermissionKind {
    Allow,
    Deny,
}

impl GarageClient {
    pub fn new(address: String, token: String) -> GarageClient {
        GarageClient { token, address }
    }

    fn http_client(&self) -> awc::Client {
        awc::ClientBuilder::new()
            .disable_redirects()
            .add_default_header((header::AUTHORIZATION, format!("Bearer {0}", self.token)))
            .add_default_header((header::CONTENT_TYPE, "application/json"))
            .add_default_header((header::ACCEPT, "application/json"))
            .finish()
    }

    pub async fn create_bucket(&self, global_alias: String) -> Result<Bucket, Error> {
        let body = json!({
            "global_alias" : global_alias,
            "localAlias" : {}
        });

        let response = self
            .http_client()
            .post(format!("{0}/v2/CreateBucket", self.address))
            .send_json(&body)
            .await;

        match response {
            Ok(mut res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCodeError(status_code));
                }
                res.json::<Bucket>()
                    .await
                    .map_err(Error::WrongResponseError)
            }
            Err(err) => Err(Error::RequestError(err.to_string())),
        }
    }

    pub async fn delete_bucket(&self, id: String) -> Result<(), Error> {
        let response = self
            .http_client()
            .post(format!("{0}/v2/DeleteBucket?id={1}", self.address, id))
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCodeError(status_code));
                }
                Ok(())
            }
            Err(err) => Err(Error::RequestError(err.to_string())),
        }
    }

    pub async fn create_key(&self, name: String) -> Result<Key, Error> {
        let body = json!({
            "allow" : {
                "createBucket": false,
            },
            "deny" : {
                "createBucket": true,
            },
            "neverExpires" : true,
            "name": name,
        });

        let response = self
            .http_client()
            .post(format!("{0}/v2/CreateBucket", self.address))
            .send_json(&body)
            .await;

        match response {
            Ok(mut res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCodeError(status_code));
                }
                res.json::<Key>().await.map_err(Error::WrongResponseError)
            }
            Err(err) => Err(Error::RequestError(err.to_string())),
        }
    }

    pub async fn delete_key(&self, id: String) -> Result<(), Error> {
        let response = self
            .http_client()
            .post(format!("{0}/v2/DeleteKey?id={1}", self.address, id))
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCodeError(status_code));
                }
                Ok(())
            }
            Err(err) => Err(Error::RequestError(err.to_string())),
        }
    }

    async fn bucket_key_permissions(
        &self,
        bucket_id: String,
        key_id: String,
        permissions: BucketPermissions,
        kind: PermissionKind,
    ) -> Result<(), Error> {
        let body = json!({
            "accessKeyId": key_id,
            "bucketId": bucket_id,
            "permissions": {
                "owner": permissions.owner,
                "read": permissions.read,
                "write": permissions.write,
            },
        });

        let path = match kind {
            PermissionKind::Allow => "AllowBucketKey",
            PermissionKind::Deny => "DenyBucketKey",
        };

        let response = self
            .http_client()
            .post(format!("{0}/v2/{1}", self.address, path))
            .send_json(&body)
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCodeError(status_code));
                }
                Ok(())
            }
            Err(err) => Err(Error::RequestError(err.to_string())),
        }
    }

    pub async fn set_bucket_permissions(
        &self,
        bucket_id: String,
        key_id: String,
        permissions: BucketPermissions,
    ) -> Result<(), Error> {
        let allow = self
            .bucket_key_permissions(
                bucket_id.clone(),
                key_id.clone(),
                permissions.clone(),
                PermissionKind::Allow,
            )
            .await;

        match allow {
            Ok(_) => {
                self.bucket_key_permissions(bucket_id, key_id, permissions, PermissionKind::Deny)
                    .await
            }
            Err(err) => Err(err),
        }
    }
}
