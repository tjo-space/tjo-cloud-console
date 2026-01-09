default:
  @just --list


export SOPS_AGE_KEY_FILE := if os() == "linux" {`echo "$HOME/.config/sops/age/keys.txt"`} else { `echo "$HOME/Library/Application Support/sops/age/keys.txt"` }

import 'secrets.justfile'

encrypt-all: kubernetes-secrets-encrypt
decrypt-all: kubernetes-secrets-decrypt

post-pull: decrypt-all
pre-commit: test lint format encrypt-all seal-secrets

VERSION_NUMBER := `cargo pkgid | cut -d# -f2`
VERSION_REF := `git describe --dirty="-dev" --always`
VERSION := VERSION_NUMBER + "-" + VERSION_REF

get-version:
  @echo {{VERSION}}

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
  nix build .#bin
  @mkdir -p dist
  @mv result dist/console
  @ls -la dist/console

build-image:
  nix build .#image
  @mkdir -p dist
  @mv result dist/image.{{ VERSION }}.tar.gz
  @ls -la dist/image.{{ VERSION }}.tar.gz

push-image:
  skopeo copy docker-archive:dist/image.{{ VERSION }}.tar.gz:console:latest docker://code.tjo.space/tjo-cloud/console:{{ VERSION }}

test:
  @cargo test

lint:
  @cargo clippy
  @cargo fmt --check

format:
  cargo fix --allow-dirty
  cargo clippy --fix --allow-dirty
  cargo fmt

env-up: env-down
  #!/usr/bin/env bash
  kind create cluster
  kubectx kind-kind
  docker compose up -d

  # Wait for Garage node to be ready
  while ! docker compose exec garage /garage node id --quiet &>/dev/null; do sleep 1; done

  # Setup Garage node
  NODE_ID=$(docker compose exec garage /garage node id --quiet | cut -d "@" -f 1)
  docker compose exec garage /garage layout assign -z docker -c 1G $NODE_ID
  docker compose exec garage /garage layout apply --version 1

  just crd-apply
  docker compose logs --follow

env-down:
  @kind delete cluster
  @kubectx  - || true
  @docker compose down

seal-secrets:
  #!/usr/bin/env bash

  for secret in $(find kubernetes/ -type f -name "*secret.yaml")
  do
    sealedSecret=$(echo $secret | sed 's/secret.yaml/secret.sealed.yaml/')
    echo "Sealing secret ${secret} to file ${sealedSecret}"
    kubeseal \
      --controller-namespace kube-system \
      --controller-name sealed-secrets \
      --secret-file $secret \
      --sealed-secret-file $sealedSecret
  done
