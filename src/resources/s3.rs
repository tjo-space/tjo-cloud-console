use awc::http::header;
use serde_json::json;
use thiserror::Error;

pub mod bucket;
pub mod token;

#[derive(Error, Debug)]
pub enum Error {
    #[error("StdIoError")]
    StdIoError(#[from] std::io::Error),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

struct Client {
    address: String,

    http_client: awc::Client,
}

struct Bucket {
    pub id: String,
}

struct Key {
    pub name: String,
    pub id: String,
    pub secret: String,
}

struct BucketPermissions {
    pub owner: bool,
    pub read: bool,
    pub write: bool,
}

impl Client {
    fn new(address: String, token: String) -> Client {
        let http_client = awc::ClientBuilder::new()
            .disable_redirects()
            .add_default_header((header::AUTHORIZATION, format!("Bearer {token}")))
            .add_default_header((header::CONTENT_TYPE, "application/json"))
            .add_default_header((header::ACCEPT, "application/json"))
            .finish();

        Client {
            address,
            http_client,
        }
    }

    async fn create_bucket(&self, global_alias: String) -> Result<Bucket, Error> {
        let response = self
            .http_client
            .post(format!("{0}/v2/CreateBucket", self.address))
            .send_json(json!({}))
            .await;

        match response {
            Ok() => Ok(Bucket {}),
            Err(err) => Err(Error::StdIoError(err)),
        }
    }

    async fn delete_bucket(id: String) -> Result<(), Error> {
        todo!("implement api")
    }

    async fn create_key(name: String) -> Result<Key, Error> {
        // deny bucket creation
        // never expire
        todo!("implement api")
    }

    async fn delete_key(id: String) -> Result<(), Error> {
        todo!("implement api")
    }

    async fn set_bucket_permissions(
        bucket_id: String,
        key_id: String,
        permissions: BucketPermissions,
    ) -> Result<(), Error> {
        todo!("implement api")
    }
}

pub async fn connect(address: String, token: String) -> Result<Client, Error> {
    Ok(Client::new(address, token))
}
