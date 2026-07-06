# Changelog

All notable changes to the API2Convert Rust SDK are documented here. This SDK versions together with
the sibling SDKs (PHP, Python, Java, Node.js, Go, Ruby) against the shared
[`SDK_CONTRACT.md`](SDK_CONTRACT.md).

## 10.2.0 — Initial release

Faithful, idiomatic Rust port at feature parity with the SDK family.

- **Blocking client** `Api2Convert` with a one-call `convert` / `convert_with` (create → upload |
  remote input → start → poll to completion → result) and `convert_async` / `convert_async_with`.
- **Downloads** via `ConversionResult` / `FileDownload`: `save`, `contents`, `url`, with
  download-password transparency and path-traversal-safe writes.
- **Full job lifecycle** through the `jobs()` resource (`create`, `get`, `list`, `update`, `start`,
  `cancel`, `add_input`, `upload`, `outputs`, `wait`), plus `conversions()`, `presets()`, `stats()`,
  `contracts()`.
- **Webhook verification** (`Api2Convert::webhooks()`): constant-time HMAC-SHA256 over the raw body;
  empty secret skips verification.
- **Typed errors** — a single `Api2ConvertError` enum with `status()` / `request_id()` / `body()` on
  HTTP variants and `retry_after` on rate limits.
- **Transport** — `X-Oc-Api-Key` auth; jittered exponential backoff honoring `Retry-After`; `429`
  retried for any method, `5xx`/network only for idempotent requests (a bare `POST` is never re-sent);
  poll interval floored and total wait capped.
- **Security** — secret-bearing requests never follow redirects; uploads use the per-job token;
  secrets never appear in errors. Proven by a black-box security suite against real loopback servers.
- **Seams** — `HttpSender` / `Sleeper` / `Rng` are public for bring-your-own-transport and testing.
