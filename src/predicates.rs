use k8s_openapi::api::core::v1 as corev1;
use kube::api::ListParams;
use kube::{
    Api,
    ResourceExt,
};

use crate::util::{
    total_pod_resources,
    Context,
    PodResources,
};

#[derive(Debug)]
pub enum InvalidNodeReason {
    NotEnoughResources,
    NodeSelectorMismatch,
}

async fn can_pod_fit(pod: &corev1::Pod, node: &corev1::Node, ctx: &Context) -> bool {
    let pod_querier: Api<corev1::Pod> = Api::all(ctx.client.clone());
    let list_params = ListParams {
        field_selector: Some(format!("spec.nodeName={}", node.name_any())),
        ..ListParams::default()
    };

    let mut available_resources = PodResources::new();
    if let Some(corev1::NodeStatus { allocatable: Some(allocatable), .. }) = &node.status {
        available_resources.cpu = allocatable["cpu"].clone().try_into().expect("invalid node spec: allocatable cpu");
        available_resources.memory =
            allocatable["memory"].clone().try_into().expect("invalid node spec: allocatable memory");
    }

    let pods_on_node = pod_querier.list(&list_params).await;

    for p in pods_on_node.expect("could not list nodes") {
        available_resources -= total_pod_resources(&p)
    }

    let pod_requests = total_pod_resources(pod);

    return pod_requests.cpu <= available_resources.cpu && pod_requests.memory <= available_resources.memory;
}

fn does_node_selector_match(pod: &corev1::Pod, node: &corev1::Node) -> bool {
    let mut matches = true;
    if let Some(corev1::PodSpec { node_selector: Some(node_selector), .. }) = &pod.spec {
        for (pk, pv) in node_selector.iter() {
            if let Some(labels) = &node.metadata.labels {
                if labels.get(pk) != Some(pv) {
                    matches = false;
                    break;
                }
            } else {
                matches = false;
                break;
            }
        }
    }
    return matches;
}

pub async fn check_node_validity(
    pod: &corev1::Pod,
    node: &corev1::Node,
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

#[cfg(test)]
mod test;
