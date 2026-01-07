use reqwest::{header, redirect, Client, StatusCode};
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Request: {0}")]
    Request(reqwest::Error),

    #[error("BadStatusCode: {0}")]
    BadStatusCode(StatusCode),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone)]
pub struct GarageClient {
    token: String,
    url: String,
    http_client: Client,
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

static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

impl GarageClient {
    pub fn new(url: String, token: String) -> Result<GarageClient, Error> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "Content-Type",
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "Accept",
            header::HeaderValue::from_static("application/json"),
        );

        let http_client = reqwest::ClientBuilder::new()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .redirect(redirect::Policy::none())
            .build()
            .map_err(Error::Request)?;

        Ok(GarageClient {
            token,
            url,
            http_client,
        })
    }

    pub async fn create_bucket(&self, global_alias: String) -> Result<Bucket, Error> {
        let body = json!({
            "global_alias" : global_alias,
            "localAlias" : {}
        });

        let response = self
            .http_client
            .clone()
            .post(format!("{0}/v2/CreateBucket", self.url))
            .bearer_auth(self.token.clone())
            .json(&body)
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCode(status_code));
                }
                res.json::<Bucket>().await.map_err(Error::Request)
            }
            Err(err) => Err(Error::Request(err)),
        }
    }

    pub async fn delete_bucket(&self, id: String) -> Result<(), Error> {
        let response = self
            .http_client
            .clone()
            .post(format!("{0}/v2/DeleteBucket?id={1}", self.url, id))
            .bearer_auth(self.token.clone())
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCode(status_code));
                }
                Ok(())
            }
            Err(err) => Err(Error::Request(err)),
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
            .http_client
            .clone()
            .post(format!("{0}/v2/CreateBucket", self.url))
            .bearer_auth(self.token.clone())
            .json(&body)
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCode(status_code));
                }
                res.json::<Key>().await.map_err(Error::Request)
            }
            Err(err) => Err(Error::Request(err)),
        }
    }

    pub async fn delete_key(&self, id: String) -> Result<(), Error> {
        let response = self
            .http_client
            .clone()
            .post(format!("{0}/v2/DeleteKey?id={1}", self.url, id))
            .bearer_auth(self.token.clone())
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCode(status_code));
                }
                Ok(())
            }
            Err(err) => Err(Error::Request(err)),
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
            .http_client
            .clone()
            .post(format!("{0}/v2/{1}", self.url, path))
            .bearer_auth(self.token.clone())
            .json(&body)
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status();
                if !status_code.is_success() {
                    return Err(Error::BadStatusCode(status_code));
                }
                Ok(())
            }
            Err(err) => Err(Error::Request(err)),
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
