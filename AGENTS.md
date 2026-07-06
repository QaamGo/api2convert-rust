# AGENTS — maintaining the API2Convert Rust SDK

This SDK is **hand-written** (not generated from OpenAPI) and kept in sync with the API by a human
**or an AI agent**. It is one of the official ports (PHP, Python, Java, Node.js, Go, Ruby, Rust) that
all implement the same language-agnostic contract in [`docs/SDK_CONTRACT.md`](docs/SDK_CONTRACT.md).

## Why hand-written

The conversion flow is multi-step (create → upload → poll → download) and the **upload step is not in
the OpenAPI spec at all**, so a generator cannot produce a usable client. We optimise for a
junior-friendly surface — one-call `convert()` — and use AI to keep it current.

## Repo layout

| Path | What it is |
| --- | --- |
| `src/client.rs`, `src/config.rs`, `src/convert_options.rs` | The client + `convert` / `convert_async` / `download` façade, `ClientBuilder`, and the option builders. **Hand-authored.** |
| `src/result.rs` | `ConversionResult` + `FileDownload` helpers. **Hand-authored.** |
| `src/upload.rs` | Streaming multipart upload to the per-job server. **Hand-authored** (not in the spec). |
| `src/webhook.rs` | Webhook HMAC verification + parsing. **Hand-authored.** |
| `src/resources.rs` | One type per API tag (Jobs, Conversions, Presets, Stats, Contracts). **Derived** from the spec. |
| `src/models.rs`, `src/enums.rs` | Typed structs (`from_value` factories) / enums. **Derived** from the spec. |
| `src/transport/` | Transport: auth, retries/backoff, error mapping, redirect policy, the `HttpSender` seam. |
| `src/errors.rs` | The typed `Api2ConvertError` enum. |
| `src/data.rs` | Tolerant JSON hydration helpers (never panic on a surprising payload). |
| `openapi/api2convert.openapi.json` | **Committed spec snapshot** the SDK targets — the diff baseline (keep md5-identical to siblings). |
| `docs/SDK_CONTRACT.md` | The fixed, language-agnostic public surface + semantics (keep md5-identical to siblings). |
| `src/**/#[cfg(test)]`, `tests/*.rs` | Unit + integration tests (fake `HttpSender`). **The guardrail.** |
| `tests/security.rs` | The black-box security suite (real loopback servers). **The redirect/leak guardrail.** |
| `tests/live.rs` | Live conformance (`#[ignore]`; auto-skips without `API2CONVERT_API_KEY`). |

## How to update the SDK to a new API version

1. **Refresh the snapshot.** Overwrite `openapi/api2convert.openapi.json` from
   `https://api.api2convert.com/v2/openapi.json` (or `/v2/schema`) and diff it. Keep it md5-identical
   to the sibling SDKs.
2. **Diff it** — new/removed/renamed operations, new fields, new enum values.
3. **Update the DERIVED layer to match the diff, and nothing else:**
   - New/changed fields → update the relevant struct in `models.rs` + its `from_value`.
   - New operation → add a method on the matching resource in `resources.rs` (mirror the style).
   - New input/output types → extend `enums.rs`.
4. **Do NOT change the hand-authored public API** (`convert`, `convert_async`, `download`, upload,
   polling, webhook verification, error types) unless `docs/SDK_CONTRACT.md` changes first. If a real
   product change requires it, update the contract in the same change and bump the **major** version.
5. **Format, lint and test (the guardrail):**
   ```sh
   make check     # fmt --check + clippy -D warnings + unit/offline/security tests
   ```
   Add or update a test for any new behavior. Keep the live conformance test runnable.
6. **Record + version.** Add a `docs/CHANGELOG.md` entry and bump `version` in `Cargo.toml` +
   `VERSION` in `src/version.rs` per SemVer (additive spec change → minor; breaking public-surface
   change → major). Tag `vX.Y.Z`.

## Guarantees to uphold (don't break these)

- **Never commit a real API key, token or secret** — not in source, tests, fixtures, examples, CI
  files or commit messages, and never publish one. Keys come only from environment variables
  (`API2CONVERT_API_KEY`) or masked/protected CI variables; tests use obvious fakes (`test-key`,
  `whsec_test`, …). The SDK must never log or expose a key/token in an error. Secret-scan before release.
- **The contract is law.** Public method names, signatures and semantics match `docs/SDK_CONTRACT.md`
  across every SDK language, adapted only to Rust idiom (see divergences below).
- **Upload uses the per-job `X-Oc-Token`, never the account key.** There is a test for this.
- **Secret-bearing requests never follow redirects.** The key/token/download-password ride in custom
  `X-Oc-*` headers that a redirect-following client could forward across hosts. Only the no-secret
  download path follows redirects (a second `reqwest::blocking::Client` with a limited redirect
  policy). `tests/security.rs` proves the guarantee with real loopback servers.
- **`convert()` stays one call** for the common case (path/URL/bytes/reader → `to` → `save()`).
- **Transient failures retry; failures surface as typed errors.** Never leak a raw transport error
  (wrap it in `Network`). A non-idempotent `POST` is never blindly retried.
- **Lean dependencies.** `reqwest`, `serde_json`, `hmac`, `sha2` only. Prefer hand-rolling small
  utilities (error impls, multipart body, hex, backoff jitter, HTTP-date) over adding a crate.

## Rust-idiom divergences from the contract

The contract fixes names and semantics; these are the only places Rust deviates, all for idiom:

- **Blocking / synchronous** (like Python/Ruby/PHP/Go/Java). No `context`/`async`.
- **The "extra" `convert` controls are builder structs** (`ConvertOptions` / `AsyncOptions`), kept
  separate from the open-ended conversion-options map exactly as the contract requires. `convert` /
  `convert_with` (and `convert_async` / `convert_async_with`) are the no-controls / with-controls pair.
- **Construction is fallible** — `Api2Convert::new(key) -> Result<_>` and `builder().build() -> Result<_>`.
- **Errors are one `Api2ConvertError` enum** matched with `match` / `matches!`; HTTP-error variants
  expose `status()` / `request_id()` / `body()`; `RateLimit` exposes `retry_after`. The timeout
  variant is `ConversionTimeout` (not `Timeout`), per the contract.
- **Nullable strings use `Option<String>`** (Rust has `Option`, unlike Go's `""`); `Job` keeps `raw`.
- **Job status predicates are methods** (`is_completed()` …); the poll method is `wait`.
- **The transport seam is public** — implement `HttpSender` to bring your own client / a test fake;
  `Sleeper` and `Rng` make backoff injectable.
- **The security suite is an integration test** (`tests/security.rs`) using hand-rolled loopback
  servers over `std::net::TcpListener` — the Rust analog of the siblings' isolated security suites.

## Conventions

- Models parse defensively via `data.rs` (tolerate missing/extra fields; never panic during
  hydration). `Job.raw` keeps the full response.
- Resource methods are thin: build the request, call the transport, hydrate a model.
- Keep the README quickstart copy-pasteable; if you change the happy path, update the README example.
