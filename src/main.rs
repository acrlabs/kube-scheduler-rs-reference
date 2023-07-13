mod util;

use futures::{future, StreamExt};
use hyper::{Body, Request};
use k8s_openapi::{
    api::core::v1::{Binding, Node, ObjectReference, Pod},
    apimachinery::pkg::api::resource::Quantity,
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
    CreateOptional,
};
use kube::{
    api::ListParams,
    runtime::controller::{Action, Controller},
    runtime::reflector::Store,
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use kube_quantity::ParsedQuantity;
use rand::seq::SliceRandom;
use std::sync::Arc;
use thiserror::Error;
use tokio::time::Duration;
use tracing::*;

type Result<T, E = MyError> = std::result::Result<T, E>;

const ATTEMPTS: u32 = 5;

struct Context {
    client: Client,
    node_store: Store<Node>,
}

#[derive(Error, Debug)]
pub enum MyError {
    #[error("idkwtf")]
    SomeDumbError,
}

#[derive(Debug)]
pub enum InvalidNodeReason {
    NotEnoughResources,
    NodeSelectorMismatch,
}

async fn can_pod_fit(pod: &Pod, node: &Node, ctx: &Context) -> bool {
    let pod_querier: Api<Pod> = Api::all(ctx.client.clone());
    let mut list_params = ListParams::default();
    list_params.field_selector = Some(format!("spec.nodeName={}", node.name_any()));

    let mut available_cpu: ParsedQuantity = "0".try_into().unwrap();
    let mut available_memory: ParsedQuantity = "0".try_into().unwrap();
    if let Some(status) = &node.status {
        if let Some(allocatable) = &status.allocatable {
            available_cpu = allocatable["cpu"].clone().try_into().unwrap();
            available_memory = allocatable["memory"].clone().try_into().unwrap();
        }
    }

    let pods_on_node = pod_querier.list(&list_params).await;

    for p in pods_on_node.unwrap() {
        let res = util::total_pod_resources(&p);
        available_cpu -= res["cpu"].clone();
        available_memory -= res["memory"].clone();
    }

    let pod_requests = util::total_pod_resources(pod);

    return pod_requests["cpu"] <= available_cpu && pod_requests["memory"] <= available_memory;
}

fn does_node_selector_match(pod: &Pod, node: &Node) -> bool {
    return true;
}

async fn check_node_validity(
    pod: &Pod,
    node: &Node,
    ctx: &Context,
) -> Result<(), InvalidNodeReason> {
    if !can_pod_fit(pod, node, ctx).await {
        return Err(InvalidNodeReason::NotEnoughResources);
    }

    if !does_node_selector_match(pod, node) {
        return Err(InvalidNodeReason::NodeSelectorMismatch);
    }

    return Ok(());
}

async fn select_node_for_pod(pod: &Pod, ctx: &Context) -> Option<Node> {
    let mut node: Option<Node> = None;
    for _ in 0..ATTEMPTS {
        let mut maybe_candidate: Option<Node> = None;
        {
            if let Some(candidate) = ctx.node_store.state().choose(&mut rand::thread_rng()) {
                maybe_candidate = Some((**candidate).clone());
            }
        }
        if let Some(candidate) = maybe_candidate {
            if let Err(e) = check_node_validity(pod, &candidate, ctx).await {
                warn!(
                    "Node {} failed validity check for pod {}/{}: {:?}",
                    candidate.name_any(),
                    pod.namespace().unwrap(),
                    pod.name_any(),
                    e
                );
            } else {
                node = Some(candidate.clone());
                break;
            }
        }
    }

    return node;
}

fn is_pod_bound(pod: &Pod) -> bool {
    if let Some(spec) = &pod.spec {
        if spec.node_name.is_some() {
            return true;
        }
    }
    return false;
}

async fn reconcile(pod: Arc<Pod>, ctx: Arc<Context>) -> Result<Action> {
    if is_pod_bound(&pod) {
        return Ok(Action::await_change());
    }

    let mut maybe_node_name: Option<String> = None;
    let mut maybe_pod_name: Option<String> = None;
    let mut maybe_pod_namespace: Option<String> = None;
    if let Some(chosen_node) = select_node_for_pod(&pod, &ctx).await {
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
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
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
        .for_each(|n| future::ready(()));

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
