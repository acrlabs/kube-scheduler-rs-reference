use futures::{future, StreamExt};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::{
    runtime::controller::{Action, Controller},
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use std::sync::Arc;
use thiserror::Error;
use tokio::time::Duration;
use tracing::*;
type Result<T, E = MyError> = std::result::Result<T, E>;

struct Context {}

#[derive(Error, Debug)]
pub enum MyError {
    #[error("idkwtf")]
    SomeDumbError,
}

async fn reconcile(pod: Arc<Pod>, ctx: Arc<Context>) -> Result<Action> {
    info!("Got a pod: {}/{}", pod.name_any(), pod.namespace().unwrap());
    return Ok(Action::await_change());
}

fn error_policy(pod: Arc<Pod>, error: &MyError, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    Action::requeue(Duration::from_secs(5 * 60))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    let client = Client::try_default()
        .await
        .expect("failed to create kube client");
    let pods: Api<Pod> = Api::all(client.clone());
    let nodes: Api<Node> = Api::all(client.clone());

    Controller::new(pods, watcher::Config::default())
        .run(reconcile, error_policy, Arc::new(Context {}))
        .for_each(|_| future::ready(()))
        .await;
    Ok(())
}
