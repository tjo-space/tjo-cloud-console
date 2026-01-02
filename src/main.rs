#![allow(unused_imports, unused_variables)]
use actix_web::{
    get, middleware, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
pub use console::{self, telemetry, Settings, State};
use futures::future::try_join_all;
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use std::collections::HashMap;
use tokio_postgres::{Client, Connection};
use tracing::*;

#[get("/metrics")]
async fn metrics(c: Data<State>, _req: HttpRequest) -> impl Responder {
    let metrics = c.metrics();
    HttpResponse::Ok()
        .content_type("application/openmetrics-text; version=1.0.0; charset=utf-8")
        .body(metrics)
}

#[get("/health")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/")]
async fn index(c: Data<State>, _req: HttpRequest) -> impl Responder {
    let d = c.diagnostics().await;
    HttpResponse::Ok().json(&d)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init().await;

    // Read settings
    let settings = Settings::new().unwrap();

    let connector = TlsConnector::builder().build()?;
    let connector = MakeTlsConnector::new(connector);

    let postgresql_clients_vec: Vec<(String, Client)> =
        try_join_all(settings.postgresql().iter().map(|(k, v)| {
            let connector = connector.clone();
            let key = k.clone();

            async move {
                let (client, connection) = tokio_postgres::connect(
                    &format!(
                        "host={0} user={1} password={2} sslmode={3}",
                        v.host, v.user, v.password, v.sslmode
                    ),
                    connector.clone(),
                )
                .await?;

                info!(
                    "Connected to Postgresql Database {} at {} with user {}",
                    key, v.host, v.user
                );

                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("connection error: {}", e);
                    }
                });

                Ok::<(String, Client), tokio_postgres::Error>((key, client))
            }
        }))
        .await?;

    let postgresql_clients: HashMap<String, Client> = postgresql_clients_vec.into_iter().collect();

    // Initiatilize Kubernetes controller state
    let state = State::new(settings);
    let console = console::run(state.clone());

    // Start web server
    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(state.clone()))
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(index)
            .service(health)
            .service(metrics)
    })
    .bind("0.0.0.0:8080")?
    .shutdown_timeout(5);

    // Both runtimes implements graceful shutdown, so poll until both are done
    tokio::join!(console, server.run()).1?;
    Ok(())
}
