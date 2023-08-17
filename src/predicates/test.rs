use std::collections::BTreeMap;

use k8s_openapi::api::core::v1 as corev1;
use rstest::*;

use super::*;

const POD_NAMESPACE: &str = "test";
const POD_NAME: &str = "pod1";

const NODE_NAME: &str = "node1";

#[fixture]
fn test_pod(#[default(None)] selector_key: Option<(&str, &str)>) -> corev1::Pod {
    let mut pod = corev1::Pod::default();
    pod.metadata.namespace = Some(POD_NAMESPACE.to_string());
    pod.metadata.name = Some(POD_NAME.to_string());

    if let Some((k, v)) = selector_key {
        let mut pod_spec = corev1::PodSpec::default();
        let mut node_selector = BTreeMap::new();
        node_selector.insert(k.to_string(), v.to_string());
        pod_spec.node_selector = Some(node_selector);
        pod.spec = Some(pod_spec);
    }

    return pod;
}

#[fixture]
fn test_node() -> corev1::Node {
    let mut node = corev1::Node::default();
    node.metadata.name = Some(NODE_NAME.to_string());

    let mut node_labels = BTreeMap::new();
    node_labels.insert("name".to_string(), NODE_NAME.to_string());
    node.metadata.labels = Some(node_labels);

    return node;
}

#[rstest]
fn test_does_node_selector_match_no_selector(test_pod: corev1::Pod, test_node: corev1::Node) {
    assert_eq!(does_node_selector_match(&test_pod, &test_node), true);
}

#[rstest]
fn test_does_node_selector_match_false(#[with(Some(("foo", "bar")))] test_pod: corev1::Pod, test_node: corev1::Node) {
    assert_eq!(does_node_selector_match(&test_pod, &test_node), false);
}

#[rstest]
fn test_does_node_selector_match_true(
    #[with(Some(("name", NODE_NAME)))] test_pod: corev1::Pod,
    test_node: corev1::Node,
) {
    assert_eq!(does_node_selector_match(&test_pod, &test_node), true);
}
