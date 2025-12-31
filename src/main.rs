#![allow(unused_imports, unused_variables)]
use actix_web::{
    get, middleware, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
pub use console::{self, telemetry, Settings, State};
use futures::future::join_all;
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use tokio_postgres::{Client, Connection};

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

    // FIXME: This should be a map not list? Learn how to do maps in Rust.
    let postgresql_clients: Vec<Client> = join_all(settings.postgresql().iter().map(|p| async {
        let (client, connection) = tokio_postgres::connect(
            &format!(
                "host={0} user={1} password={2} sslmode=require",
                p.host, p.user, p.password
            ),
            connector.clone(),
        )
        .await
        .unwrap();

        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }

        client
    }))
    .await;

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
