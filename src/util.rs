use std::ops::SubAssign;

use k8s_openapi::api::core::v1 as corev1;
use kube::api::{
    Resource,
    ResourceExt,
};
use kube::runtime::reflector;
use kube::Client;
use kube_quantity::ParsedQuantity;

pub struct Context {
    pub client: Client,
    pub node_store: reflector::Store<corev1::Node>,
}

pub struct PodResources {
    pub cpu: ParsedQuantity,
    pub memory: ParsedQuantity,
}

impl PodResources {
    pub fn new() -> PodResources {
        return PodResources {
            cpu: "0".try_into().unwrap(),
            memory: "0".try_into().unwrap(),
        };
    }
}

impl SubAssign for PodResources {
    fn sub_assign(&mut self, other: Self) {
        self.cpu -= other.cpu;
        self.memory -= other.memory;
    }
}

pub fn is_pod_bound(pod: &corev1::Pod) -> bool {
    if let Some(spec) = &pod.spec {
        if spec.node_name.is_some() {
            return true;
        }
    }
    return false;
}

pub fn full_name(obj: &impl Resource) -> String {
    return match obj.namespace() {
        Some(ns) => format!("{}/{}", ns, obj.name_any()),
        None => obj.name_any().clone(),
    };
}

pub fn total_pod_resources(pod: &corev1::Pod) -> PodResources {
    let mut pod_resources = PodResources::new();

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if let corev1::Container {
                resources: Some(corev1::ResourceRequirements { requests: Some(requests), .. }),
                ..
            } = c
            {
                if let Some(cpu) = requests.get("cpu") {
                    pod_resources.cpu += cpu.try_into().expect("invalid pod spec: cpu request");
                }
                if let Some(memory) = requests.get("memory") {
                    pod_resources.memory += memory.try_into().expect("invalid pod spec: memory request");
                }
            }
        }
    }

    return pod_resources;
}
