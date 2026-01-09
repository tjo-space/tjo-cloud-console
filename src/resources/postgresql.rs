use crate::{Error, Result};
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use tracing::*;

pub mod database;
pub mod user;

pub use tokio_postgres::Client;

pub async fn connect(
    name: String,
    host: String,
    database: String,
    user: String,
    password: String,
    sslmode: String,
) -> Result<Client, Error> {
    let connector = TlsConnector::builder().build()?;
    let connector = MakeTlsConnector::new(connector);

    let (client, connection) = tokio_postgres::connect(
        &format!(
            "host={host} user={user} password={password} database={database} sslmode={sslmode}",
        ),
        connector.clone(),
    )
    .await?;

    info!("Connected to Postgresql name={name} host={host} user={user} database={database} sslmode={sslmode}",);

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            panic!("connection error: {}", e);
        }
    });

    Ok(client)
}
