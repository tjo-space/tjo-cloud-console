# Always use devbox environment to run commands.
set shell := ["devbox", "run"]
# Load dotenv
set dotenv-load

default:
  @just --list

pre-commit: test lint format

run:
  @cargo run

generate:
  @cargo run --bin crdgen > yaml/crd.yaml

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
