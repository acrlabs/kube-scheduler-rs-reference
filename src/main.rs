use futures::{future, StreamExt};
use hyper::{Body, Request};
use k8s_openapi::{
    api::core::v1::{Binding, Node, ObjectReference, Pod},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
    CreateOptional,
};
use kube::{
    runtime::controller::{Action, Controller},
    runtime::reflector::Store,
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use rand::seq::SliceRandom;
use std::sync::Arc;
use thiserror::Error;
use tokio::time::Duration;
use tracing::*;

type Result<T, E = MyError> = std::result::Result<T, E>;

struct Context {
    client: Client,
    node_store: Store<Node>,
}

#[derive(Error, Debug)]
pub enum MyError {
    #[error("idkwtf")]
    SomeDumbError,
}

async fn reconcile(pod: Arc<Pod>, ctx: Arc<Context>) -> Result<Action> {
    let mut maybe_node_name: Option<String> = None;
    let mut maybe_pod_name: Option<String> = None;
    let mut maybe_pod_namespace: Option<String> = None;
    if let Some(chosen_node) = ctx.node_store.state().choose(&mut rand::thread_rng()) {
        maybe_node_name = Some(chosen_node.name_any().clone());
        maybe_pod_name = Some(pod.name_any().clone());
        maybe_pod_namespace = Some(pod.namespace().unwrap().clone());
    }

    if let (Some(node_name), Some(pod_name), Some(pod_namespace)) =
        (maybe_node_name, maybe_pod_name, maybe_pod_namespace)
    {
        let mut chosen_node_ref = ObjectReference::default();
        chosen_node_ref.name = Some(node_name.clone());

        let mut pod_meta = ObjectMeta::default();
        pod_meta.name = Some(pod_name.clone());
        pod_meta.namespace = Some(pod_namespace.clone());

        let binding = Binding {
            metadata: pod_meta,
            target: chosen_node_ref,
        };

        info!(
            "Binding pod {}/{} to {}: {:?}",
            pod_name, pod_namespace, node_name, binding,
        );
        let (req, resp_body) = Binding::create_pod(
            pod_name.as_str(),
            pod_namespace.as_str(),
            &binding,
            CreateOptional::default(),
        )
        .unwrap();

        let (parts, bytes) = req.into_parts();
        let req_body = Request::from_parts(parts, Body::from(bytes));
        let mut resp = ctx.client.send(req_body).await;
    }
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
    let (node_store, node_writer) = reflector::store();
    let watch = reflector(node_writer, watcher(nodes, Default::default()))
        .backoff(backoff::ExponentialBackoff::default())
        .touched_objects()
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|_| future::ready(()));

    let watch_pending_pods = watcher::Config::default().fields("status.phase=Pending");
    let ctrl = Controller::new(pods, watch_pending_pods)
        .run(
            reconcile,
            error_policy,
            Arc::new(Context {
                client: client.clone(),
                node_store,
            }),
        )
        .for_each(|_| future::ready(()));

    tokio::select!(
        _ = watch => warn!("watch exited"),
        _ = ctrl => info!("controller exited")
    );

    Ok(())
}