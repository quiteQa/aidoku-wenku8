#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")"
policy_tests=$(mktemp -d)
trap 'rm -f "$policy_tests/network-tests" "$policy_tests/url-tests"; rmdir "$policy_tests"' EXIT
rustc --edition=2021 --test src/network_policy.rs -o "$policy_tests/network-tests"
"$policy_tests/network-tests"
rustc --edition=2021 --test src/urls.rs -o "$policy_tests/url-tests"
"$policy_tests/url-tests"
cargo build --locked --release --target wasm32-unknown-unknown
node tests/wasm-network.mjs
aidoku package .
aidoku verify package.aix
