use futures::{future, StreamExt};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::{
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use tracing::*;
type Result<T, E = anyhow::Error> = std::result::Result<T, E>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::all(client.clone());
    let nodes: Api<Node> = Api::all(client.clone());

    let (_, pod_writer) = reflector::store();
    let (_, node_writer) = reflector::store();
    let pod_watcher = reflector(pod_writer, watcher(pods, Default::default()))
        .backoff(backoff::ExponentialBackoff::default())
        .applied_objects()
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|o| {
            debug!("Saw {} in {}", o.name_any(), o.namespace().unwrap());
            future::ready(())
        });

    let node_watcher = reflector(node_writer, watcher(nodes, Default::default()))
        .backoff(backoff::ExponentialBackoff::default())
        .applied_objects()
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|o| {
            debug!("Saw {}", o.name_any());
            future::ready(())
        });
    tokio::select! {
        _ = pod_watcher => warn!("watch exited"),
        _ = node_watcher => warn!("watch exited"),
    };
    Ok(())
}
