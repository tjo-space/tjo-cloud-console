use crate::{resources, Error, GarageClient, State};
use futures::future::try_join_all;
use kube::client::Client;
use std::collections::HashMap;
use std::sync::Arc;

pub static FINALIZER: &str = "console.tjo.cloud";

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(state: State) -> Result<(), Error> {
    let kube_client = Client::try_default()
        .await
        .expect("failed to create kube Client");

    let postgresql_clients: HashMap<String, resources::postgresql::Client> =
        try_join_all(state.settings().postgresql().iter().map(|(k, v)| async {
            let key = k.clone();
            let client = resources::postgresql::connect(
                key.clone(),
                v.host.clone(),
                v.database.clone(),
                v.user.clone(),
                v.password.clone(),
                v.sslmode.clone(),
                v.ssl_accept_invalid_cert.clone(),
            )
            .await
            .unwrap_or_else(|e| panic!("failed to connect to postgresql server {key}: {e:?}"));

            Ok::<(String, resources::postgresql::Client), Error>((key, client))
        }))
        .await
        .expect("failed to connect to postgresql servers")
        .into_iter()
        .collect();

    let postgresql_clients = Arc::new(postgresql_clients);

    let garage_client = Arc::new(
        GarageClient::new(
            state.settings().s3().url.clone(),
            state.settings().s3().token.clone(),
        )
        .expect("failed to create garage client"),
    );

    match tokio::try_join!(
        resources::postgresql::database::run(
            state
                .to_context(
                    kube_client.clone(),
                    postgresql_clients.clone(),
                    garage_client.clone()
                )
                .await,
            kube_client.clone(),
        ),
        resources::postgresql::user::run(
            state
                .to_context(
                    kube_client.clone(),
                    postgresql_clients.clone(),
                    garage_client.clone()
                )
                .await,
            kube_client.clone(),
        ),
        resources::s3::token::run(
            state
                .to_context(
                    kube_client.clone(),
                    postgresql_clients.clone(),
                    garage_client.clone()
                )
                .await,
            kube_client.clone(),
        ),
        resources::s3::bucket::run(
            state
                .to_context(
                    kube_client.clone(),
                    postgresql_clients.clone(),
                    garage_client.clone()
                )
                .await,
            kube_client.clone(),
        )
    ) {
        Ok((_, _, _, _)) => Ok(()),
        Err(err) => Err(err),
    }
}
