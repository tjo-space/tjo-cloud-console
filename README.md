# console.tjo.cloud

Cloud Console to manage resources, projects.

This is a Kubernetes Controller to provision resources for:
  - s3.tjo.cloud
  - postgresql.tjo.cloud
  - ingress.tjo.cloud (?)
  - argocd.k8s.tjo.cloud (?)

This resources would be defined as custom CRDS. Which would be provisioned by this service.

It should allow provisioning non-kubernetes "stuff" using kubernetes resources. Via Terraform, kubectl
or anything else that speaks kubernetes.

## Web UI?

Maybe. Maybe not needed if we can use some of the shelf kubernetes ui instead. All resources should be just
kubernetes resources anyways.

Candidates:
- https://headlamp.dev
- https://github.com/zxh326/kite

## Dependencies

- [actix/actix-web](https://github.com/actix/actix-web)
  - HTTP Server (todo: might not be needed?)
- [kube-rs/kube](https://github.com/kube-rs/kube)
  - Kubernetes Client/Tooling
- [postgres](https://github.com/rust-postgres/rust-postgres)
  - Postgresql Client
- [awc](https://lib.rs/crates/awc)
  - HTTP Client
- [opentelemetry](https://github.com/open-telemetry/opentelemetry-rust)
  - Monitoring/Observability. TODO: Currently copy-paste mess.
