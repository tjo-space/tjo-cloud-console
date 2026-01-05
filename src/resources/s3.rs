use crate::{Error, GarageClient};

pub mod bucket;
pub mod token;

pub async fn connect(address: String, token: String) -> Result<GarageClient, Error> {
    Ok(GarageClient::new(address, token))
}
