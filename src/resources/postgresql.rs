use crate::{resources, telemetry, Diagnostics, Error, Metrics, Result, Settings, State};
use chrono::Utc;
use futures::future::try_join_all;
use futures::StreamExt;
use kube::{
    api::{Api, ListParams, ResourceExt},
    runtime::{
        controller::{Action, Controller},
        events::Recorder,
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
};
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Duration};
use tracing::*;

pub mod database;
pub mod user;

pub use tokio_postgres::Client;

pub async fn connect(
    name: String,
    host: String,
    user: String,
    password: String,
    sslmode: String,
) -> Result<Client, Error> {
    let connector = TlsConnector::builder().build()?;
    let connector = MakeTlsConnector::new(connector);

    let (client, connection) = tokio_postgres::connect(
        &format!("host={host} user={user} password={password} sslmode={sslmode}",),
        connector.clone(),
    )
    .await?;

    info!("Connected to Postgresql name={name} host={host} user={user}",);

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            panic!("connection error: {}", e);
        }
    });

    Ok(client)
}
