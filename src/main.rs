mod error;
mod predicates;
mod util;

use std::ops::Deref;
use std::sync::Arc;

use futures::{
    future,
    StreamExt,
};
use hyper::{
    body,
    Body,
    Request,
};
use k8s_openapi::api::core::v1 as corev1;
use k8s_openapi::CreateOptional;
use kube::runtime::controller::{
    Action,
    Controller,
};
use kube::runtime::{
    reflector,
    watcher,
    WatchStreamExt,
};
use kube::{
    Api,
    Client,
    ResourceExt,
};
use rand::seq::SliceRandom;
use tokio::time::Duration;
use tracing::*;

use crate::error::{
    ReconcileError,
    ReconcileResult,
};
use crate::predicates::check_node_validity;
use crate::util::{
    full_name,
    is_pod_bound,
    Context,
};

const ATTEMPTS: u32 = 5;

async fn select_node_for_pod(pod: &corev1::Pod, ctx: &Context) -> Option<corev1::Node> {
    let mut node: Option<corev1::Node> = None;
    for _ in 0..ATTEMPTS {
        let mut maybe_candidate: Option<corev1::Node> = None;
        {
            if let Some(candidate) = ctx.node_store.state().choose(&mut rand::thread_rng()) {
                maybe_candidate = Some((**candidate).clone());
            }
        }
        if let Some(candidate) = maybe_candidate {
            if let Err(e) = check_node_validity(pod, &candidate, ctx).await {
                warn!("Node {} failed validity check for pod {}: {:?}", candidate.name_any(), full_name(pod), e);
            } else {
                node = Some(candidate.clone());
                break;
            }
        }
    }

    return node;
}

async fn reconcile(pod: Arc<corev1::Pod>, ctx: Arc<Context>) -> ReconcileResult<Action> {
    if is_pod_bound(&pod) {
        return Ok(Action::await_change());
    }

    if let Some(chosen_node) = select_node_for_pod(&pod, &ctx).await {
        let pod_name = pod.name_any();
        let pod_namespace = pod.namespace().unwrap(); // pods always have a namespace
        let node_name = chosen_node.name_any().clone();

        let mut chosen_node_ref = corev1::ObjectReference::default();
        chosen_node_ref.name = Some(node_name.clone());

        let binding = corev1::Binding {
            metadata: pod.metadata.clone(),
            target: chosen_node_ref,
        };

        info!("Binding pod {} to {}", full_name(pod.deref()), node_name);
        match corev1::Binding::create_pod(
            pod_name.as_str(),
            pod_namespace.as_str(),
            &binding,
            CreateOptional::default(),
        ) {
            Ok((req, _)) => {
                let (parts, bytes) = req.into_parts();
                let req_body = Request::from_parts(parts, Body::from(bytes));
                match ctx.client.send(req_body).await {
                    Ok(resp) => info!("got response: {:?}", hyper::body::to_bytes(resp.into_body()).await.unwrap()),
                    Err(e) => {
                        error!("failed to create binding: {}", e);
                        return Err(ReconcileError::CreateBindingFailed);
                    },
                }
            },
            Err(e) => {
                error!("failed to create binding object: {}", e);
                return Err(ReconcileError::CreateBindingObjectFailed);
            },
        }
    } else {
        return Err(ReconcileError::NoNodeFound);
    }
    return Ok(Action::await_change());
}

fn error_policy(pod: Arc<corev1::Pod>, error: &ReconcileError, _: Arc<Context>) -> Action {
    warn!("reconcile failed on pod {}: {:?}", full_name(pod.deref()), error);
    return Action::requeue(Duration::from_secs(5 * 60));
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let client = Client::try_default().await.expect("failed to create kube client");
    let pods: Api<corev1::Pod> = Api::all(client.clone());

    let nodes: Api<corev1::Node> = Api::all(client.clone());
    let (node_store, node_writer) = reflector::store();
    let watch = reflector(node_writer, watcher(nodes, Default::default()))
        .backoff(backoff::ExponentialBackoff::default())
        .touched_objects()
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|_| future::ready(()));

    let watch_pending_pods = watcher::Config::default().fields("status.phase=Pending");
    let ctrl = Controller::new(pods, watch_pending_pods)
        .run(reconcile, error_policy, Arc::new(Context { client: client.clone(), node_store }))
        .for_each(|_| future::ready(()));

    tokio::select!(
        _ = watch => warn!("watch exited"),
        _ = ctrl => info!("controller exited")
    );

    return Ok(());
}
