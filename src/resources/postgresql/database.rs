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
    pub location: String,
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
        let docs: Api<Database> = Api::namespaced(client, &ns);

        if !self.was_created() {
            // send an event once per hide
            ctx.recorder
                .publish(
                    &Event {
                        type_: EventType::Normal,
                        reason: "CreationRequested".into(),
                        note: Some(format!("Creating `{name}`")),
                        action: "Creating".into(),
                        secondary: None,
                    },
                    &oref,
                )
                .await
                .map_err(Error::KubeError)?;
        }
        if name == "illegal" {
            return Err(Error::IllegalDatabase); // error names show up in metrics
        }
        // always overwrite status object with what we saw
        let new_status = Patch::Apply(json!({
            "apiVersion": "kube.rs/v1",
            "kind": "Database",
            "status": DatabaseStatus {
                created : true, // TODO: Actual logic
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        let _o = docs
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Finalizer cleanup (the object was deleted, ensure nothing is orphaned)
    pub async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let oref = self.object_ref(&());
        // Database doesn't have any real cleanup, so we just publish an event
        ctx.recorder
            .publish(
                &Event {
                    type_: EventType::Normal,
                    reason: "DeleteRequested".into(),
                    note: Some(format!("Delete `{}`", self.name_any())),
                    action: "Deleting".into(),
                    secondary: None,
                },
                &oref,
            )
            .await
            .map_err(Error::KubeError)?;
        Ok(Action::await_change())
    }
}
