use k8s_openapi::api::core::v1::Pod;
use kube_quantity::ParsedQuantity;
use std::collections::BTreeMap;

pub fn total_pod_resources(pod: &Pod) -> BTreeMap<&str, ParsedQuantity> {
    let mut resource_map = BTreeMap::new();
    let mut allocated_cpu: ParsedQuantity = "0".try_into().unwrap();
    let mut allocated_memory: ParsedQuantity = "0".try_into().unwrap();

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if let Some(resources) = &c.resources {
                if let Some(requests) = &resources.requests {
                    if let Some(cpu) = requests.get("cpu") {
                        allocated_cpu += cpu.try_into().unwrap();
                    }
                    if let Some(memory) = requests.get("memory") {
                        allocated_memory += memory.try_into().unwrap();
                    }
                }
            }
        }
    }

    resource_map.insert("cpu", allocated_cpu);
    resource_map.insert("memory", allocated_memory);
    resource_map
}