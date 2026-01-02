use crate::{Context, Error, Result};
use kube::{
    api::{Api, Patch, PatchParams, ResourceExt},
    runtime::{
        controller::Action,
        events::{Event, EventType},
    },
    CustomResource, Resource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::time::Duration;

pub static DATABASE_FINALIZER: &str = "database.postgresql.tjo.cloud";

/// Database on the postgresql.tjo.cloud database platform
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "Database",
    group = "postgresql.tjo.cloud",
    version = "v1",
    namespaced,
    shortname = "dat",
    status = "DatabaseStatus"
)]
pub struct DatabaseSpec {
    #[schemars(length(min = 3, max = 63))]
    pub name: String,
    pub server: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct DatabaseStatus {
    pub created: bool,
}

impl Database {
    fn was_created(&self) -> bool {
        self.status.as_ref().map(|s| s.created).unwrap_or(false)
    }

    // Reconcile (for non-finalizer related changes)
    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        let oref = self.object_ref(&());
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let databases: Api<Database> = Api::namespaced(client, &ns);

        if !self.was_created() {
            // send an event once per hide
            ctx.recorder
                .publish(
                    &Event {
                        type_: EventType::Normal,
                        reason: "CreationRequested".into(),
                        note: Some(format!("Creating database for `{name}`")),
                        action: "Creating".into(),
                        secondary: None,
                    },
                    &oref,
                )
                .await
                .map_err(Error::KubeError)?;
        }
        if name == "illegal" {
            return Err(Error::PostgresqlIllegalDatabase); // error names show up in metrics
        }

        let object: Database = databases.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&object.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        ctx.postgresql_clients[&object.spec.server]
            .execute(&format!("CREATE DATABASE {}", object.spec.name), &[])
            .await?;

        // always overwrite status object with what we saw
        let new_status = Patch::Apply(json!({
            "apiVersion": "kube.rs/v1",
            "kind": "Database",
            "status": DatabaseStatus {
                created : true,
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        databases
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    pub async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        let oref = self.object_ref(&());
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let databases: Api<Database> = Api::namespaced(client, &ns);

        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "DeleteRequested".into(),
                    note: Some(format!("Dropping database for `{}`", self.name_any())),
                    action: "Deleting".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;

        let object: Database = databases.get(&name).await.map_err(Error::KubeError)?;

        if !ctx.postgresql_clients.contains_key(&object.spec.server) {
            return Err(Error::PostgresqlUnknownServer);
        }

        ctx.postgresql_clients[&object.spec.server]
            .execute(&format!("DROP DATABASE {}", object.spec.name), &[])
            .await?;

        Ok(Action::await_change())
    }
}
