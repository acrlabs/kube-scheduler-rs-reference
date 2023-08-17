# kube-scheduler-rs reference implementation

A "reference" implementation for building a Kubernetes scheduler in Rust.  This is not intended as a replacement for the
real Kubernetes scheduler, nor should it be used in any sort of production environment, but it does do an _extremely
basic_ job of scheduling pods onto nodes in a Kubernetes cluster.

## How does it work?

The reference implementation relies on the [kube-rs/kube](https://github.com/kube-rs/kube) crate to interact with the
Kubernetes API, as well as the [k8s-openapi](https://github.com/Arnavion/k8s-openapi) crate for the various Kubernetes
API objects.  It sets up a controller that watches `Unschedulable` Pods and all Nodes in the cluster.  When an
unschedulable pod arrives, it will randomly try to pick (up to) five nodes in the cluster, checking CPU and memory
constraints, as well as `nodeSelector` constraints for each.  If it finds one that succeeds, it will create a binding
for the pod to the node.  Otherwise, it will return an error and try again later.

Note that because the scheduler is checking only a _tiny fraction_ of the possible scheduling constraints that
Kubernetes provides, so it is _highly likely_ that the pod binding will fail in any sort of "real" cluster.

## Why, though?

Mainly this was intended as an exercise in learning more about the `kube` Rust crate and the surrounding ecosystem, as
well as to demonstrate that building a Kubernetes scheduler in Rust is _possible_.  Whether it's a good idea remains to
be seen.

## Development

To run the scheduler: `cargo run`.  Note that you don't need to run this in a pod inside your cluster as long as you
have a kubeconfig in a standard location with cluster admin privileges.

To run the tests: `cargo test`.  Note that we have laughably low test coverage.

We do use `pre-commit` in this repo to check formatting and linting.  Install
[pre-commit](https://pre-commit.com/#plugins) here, and then run `pre-commit install` to configure the hooks.

## Contributing

I don't really expect to do any more on this project, but if you want to extend it, or you find things that I've done
that could be better, I'll happily accept PRs.
