# Guardrail targets for the API2Convert Rust SDK. `cargo` is the real driver;
# this mirrors the sibling SDKs' `make check` ergonomics.

.PHONY: check fmt fmt-check lint test test-security test-live build examples

# The full guardrail: formatting, lints, and the offline + security tests.
check: fmt-check lint test

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Unit + offline + security tests (hermetic; no network, no API key).
test:
	cargo test

# The redirect/leak guarantees only (real loopback servers).
test-security:
	cargo test --test security

# Live conformance — needs API2CONVERT_API_KEY in the environment and consumes quota.
test-live:
	cargo test --test live -- --ignored

build:
	cargo build

examples:
	cargo build --examples
