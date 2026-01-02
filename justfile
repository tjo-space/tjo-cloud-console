# Always use nix environment to run commands.
set shell := ["nix", "develop", "--command", "bash", "-c"]

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

build-bin:
  @nix build .#bin
  @mkdir -p dist
  @mv result dist/console
  @ls -la dist/console

build-image:
  @nix build .#image
  @mkdir -p dist
  @mv result dist/image.tar.gz
  @ls -la dist/image.tar.gz

test:
  @cargo test

lint:
  @cargo clippy
  @cargo fmt --check

format:
  @cargo fix
  @cargo clippy --fix
  @cargo fmt

env-up:
  @kind create cluster
  @kubectx kind-kind
  @docker compose up -d
  @just crd-apply
  @docker compose logs --follow

env-down:
  @kind delete cluster
  @kubectx  - || true
  @docker compose down
