# Always use devbox environment to run commands.
set shell := ["devbox", "run"]
# Load dotenv
set dotenv-load

pre-commit: lint format

default:
  @just --list

run:
  @cargo run

lint:
  @cargo clippy
  @cargo fmt --check

format:
  @cargo fix
  @cargo fmt
