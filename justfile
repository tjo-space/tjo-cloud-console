# Always use devbox environment to run commands.
set shell := ["nix", "develop", "--command", "bash", "-c"]
# Load dotenv
set dotenv-load

default:
  @just --list

pre-commit: test lint format

run:
  @cargo run

crd-generate:
  @cargo run --bin crdgen > yaml/crd.yaml

crd-apply: crd-generate
  @if [ "$(kubectx --current)" != "kind-kind" ]; then echo "Please switch to kind-kind kubectl context"; exit 1; fi
  @kubectl apply -f yaml/crd.yaml

crd-examples: crd-apply
  @kubectl apply -f examples/s3.bucket.yaml

compile:
  @cargo build --release --bin console

test:
  @cargo test

lint:
  @cargo clippy
  @cargo fmt --check

format:
  @cargo fix
  @cargo fmt

cluster-up:
  @kind create cluster
  @kubectx kind-kind

cluster-down:
  @kind delete cluster
  @kubectx  - || true
