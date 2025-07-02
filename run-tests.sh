#!/usr/bin/env bash
set -e

echo "Running unit and integration tests..."
cargo test --all -- --nocapture 