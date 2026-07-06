# API2Convert SDK contract

The **language-agnostic** behavior contract every official API2Convert SDK implements. It is the
source of truth for the public surface and its semantics, and the spec that future ports
(TypeScript, Python, …) follow so the SDKs stay equivalent.

Two layers make up an SDK:

- **Derived layer** — typed models and one method per API operation. Tracks the OpenAPI spec
  (`openapi/api2convert.openapi.json`); an AI update may freely change it to match the spec.
- **Hand-authored layer** — the ergonomics below (`convert`, upload, polling, download, webhook
  verification). These flows are **not** in the spec. Their **public signatures and semantics are
  fixed**: change them only when this document changes, and bump the major version when you do.

> When the two disagree, this contract wins for the hand-authored layer; the spec wins for the
> derived layer.

## Protocol facts (the API the SDK speaks)

- Base URL `https://api.api2convert.com/v2`. Auth header `X-Oc-Api-Key: <key>` on account requests.
- **Create job** `POST /jobs` `{ conversion:[{category?,target,options?}], input?:[…], process:bool,
  callback?, notify_status?, download_passwords?:[…] }` → response includes `id`, per-job `token`,
  per-job upload `server`, and `status.code`. `download_passwords` protects every output of the job;
  any password in the list then unlocks its downloads. The API never returns the plaintext back.
- **Upload** (not in the spec): `POST {server}/upload-file/{job_id}`, `multipart/form-data` field
  `file`, authenticated with the per-job **`X-Oc-Token`** header — never the account key.
- **Add remote input**: `POST /jobs/{id}/input` `{ type:'remote', source:'https://…' }`.
- **Start**: `PATCH /jobs/{id}` `{ process:true }`.
- **Poll**: `GET /jobs/{id}` → terminal when `status.code ∈ {completed, failed, canceled}`
  (`failed`/`canceled` are unsuccessful terminals; non-terminal: `created`, `incomplete`,
  `downloading`, `queued`, `processing`, and any unknown code). Poll with backoff; clamp the
  interval to a floor (never busy-loop) and the total wait to a ceiling (never poll unbounded).
- **Download**: `GET output.uri` — self-contained, no auth; `X-Oc-Download-Password` header if set.
- **Discover options**: `GET /conversions?category=&target=`.
- **Errors**: HTTP body `{ "message": "…" }`; job-level `errors[]` / `warnings[]` of
  `{ source, id_source, code, message, details }`.

## Public surface (every SDK must provide)

### Client
- Construct with an API key (falling back to the `API2CONVERT_API_KEY` env var) and options
  (`baseUrl`, `timeout`, `maxRetries`, `pollInterval`, `pollMaxInterval`, `pollTimeout`).
- `convert(input, to, options?, {category?, timeout?, outputIndex?, filename?, downloadPassword?}) →
  ConversionResult` — create → (upload | remote input) → start → **poll to completion** → return.
  `to` is the target format string; `options` is the conversion-options map (passed 1:1 to the API's
  conversion `options`); the remaining controls are optional named/keyword arguments (never mixed
  into the options map, so open-ended API options can't collide with SDK keys). `input` is a local
  path, a URL (`^https?://`), or a stream. A URL is sent as a single started job with a `remote`
  input; anything else is staged, uploaded, then started. `downloadPassword`, when given, is sent as
  the job's `download_passwords` and **remembered on the returned result** so its downloads apply it
  automatically (see below).
- `convertAsync(input, to, options?, {callback?, category?, filename?, downloadPassword?}) → Job` —
  same, but returns once started without polling; sets `notify_status: true` when a `callback` is
  given. `downloadPassword` sets the job's `download_passwords` (a later download must supply it,
  since the returned `Job` is not a result wrapper).
- `download(output, downloadPassword?) → FileDownload`.
- `options(target, category?) → map` — discover the valid conversion options for a target
  (category optional). Sugar for `conversions().options(target, category?)`.
- Resource accessors: `jobs()`, `conversions()`, `presets()`, `stats()`, `contracts()`.
- `webhooks()` — usable without a configured client (static/standalone).

### ConversionResult / FileDownload
- `save(pathOrDir, downloadPassword?) → path` — streams to disk; a directory keeps the API filename.
- `contents(downloadPassword?) → binary`, `url() → string`, `output()`, `outputs()`.
- **Download-password transparency**: a password supplied at conversion time (`convert(...,
  downloadPassword)`) or to `download(output, downloadPassword)` is remembered and sent as the
  `X-Oc-Download-Password` header on every download from that result/handle — callers do not
  re-supply it. An explicit `downloadPassword` argument to `save()` / `contents()` overrides the
  remembered one for that call.

### Jobs resource
- `create(payload, idempotencyKey?)`, `get(id)`, `list(status?, page?)`, `update(id, payload)`,
  `start(id)`, `cancel(id)`, `addInput(id, input)`, `upload(job, file, filename?)`, `outputs(id)`.
- `wait(id, timeoutSeconds?, throwOnFailure=true)` — poll with backoff until terminal; raise
  `ConversionFailedException` on `failed`/`canceled` (unless disabled), `TimeoutException` past the
  deadline. Interval is floored and the total wait is capped, so no configuration can busy-loop or
  poll unbounded.

### Webhooks
- `constructEvent(rawBody, signature, secret) → WebhookEvent` — verify HMAC-SHA256 (matching the
  server's signed-webhooks scheme) then deserialize; raise `SignatureVerificationException` on a
  missing/wrong signature. Empty secret skips verification.
- `parse(rawBody) → WebhookEvent` — deserialize without verifying (pre-signed-webhooks).

## Cross-cutting semantics
- **Auth**: account key as `X-Oc-Api-Key`; uploads use the per-job token.
- **Retries**: automatically retry with capped, jittered exponential backoff, honoring `Retry-After`
  (delay-seconds or HTTP-date form, clamped to a ceiling). `429` is retried for every method; `5xx`
  and network errors are retried only for idempotent methods (`GET`/`HEAD`/`PUT`/`DELETE`/`OPTIONS`/
  `TRACE`) or a request carrying an `Idempotency-Key`, so a bare non-idempotent `POST` is never
  blindly re-sent (no duplicate jobs). Only replayable (seekable/empty) bodies are retried at all.
  Surface the failure as a typed exception once retries are exhausted.
- **Forward-compat headers**: send an `Idempotency-Key` on create when supplied; read `Retry-After`
  / `RateLimit-*`; capture `X-Request-Id` onto exceptions. All degrade gracefully if absent today.
- **Errors** map by status: 400/422 → validation, 401/403 → auth, 402 → payment, 404 → not-found,
  429 → rate-limit (with `retryAfter`), 5xx → server, other 4xx → generic API error. A failed job →
  conversion-failed (carrying the job and its `errors`).
- **Naming**: method names and option keys are identical across languages, adapted only to each
  language's idiom (camelCase / snake_case).
