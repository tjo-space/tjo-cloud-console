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
