use crate::{resources, Error, State};
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
                v.user.clone(),
                v.password.clone(),
                v.sslmode.clone(),
            )
            .await?;

            Ok::<(String, resources::postgresql::Client), Error>((key, client))
        }))
        .await
        .expect("failed to connect to postgresql server")
        .into_iter()
        .collect();

    let postgresql_clients = Arc::new(postgresql_clients);

    match tokio::try_join!(
        resources::postgresql::database::run(
            state.clone(),
            kube_client.clone(),
            postgresql_clients.clone(),
        ),
        resources::postgresql::user::run(
            state.clone(),
            kube_client.clone(),
            postgresql_clients.clone(),
        )
    ) {
        Ok((_, _)) => Ok(()),
        Err(err) => Err(err),
    }
}
