#![allow(unused_imports, unused_variables)]
use actix_web::{
    get, middleware, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
pub use console::{self, telemetry, Settings, State};
use futures::TryFutureExt;
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

    info!(
        "Starting console.tjo.cloud version={0}",
        env!("CARGO_PKG_VERSION")
    );

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Read settings
    let settings = Settings::new().unwrap();

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
    .shutdown_timeout(5)
    .run();

    // Both runtimes implements graceful shutdown, so poll until both are done
    let result = tokio::try_join!(console, server.map_err(console::Error::StdIoError));

    match result {
        Ok(_) => {
            info!("Shutdown completed.");
            Ok(())
        }
        Err(error) => {
            error!("Failure: {}", error);
            std::process::exit(1)
        }
    }
}
