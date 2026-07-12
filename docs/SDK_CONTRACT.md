# API2Convert SDK contract

The **language-agnostic** behavior contract every official API2Convert SDK implements. It is the
source of truth for the public surface and its semantics, and the spec that future ports
(TypeScript, Python, …) follow so the SDKs stay equivalent.

Two layers make up an SDK:

- **Derived layer** — typed models and one method per API operation. Tracks the OpenAPI spec
  (`openapi/api2convert.openapi.json`); an AI update may freely change it to match the spec.
- **Hand-authored layer** — the ergonomics below (`convert`, upload, polling, download, webhook
  verification). These flows are **not** in the spec. Their **public signatures and semantics are
  fixed**: change them only when this document changes. Adding a new **optional** parameter or
  options-bag field that preserves every existing call site and behavior is an additive **minor**;
  changing or removing an existing parameter, return type, or documented semantic is a **major**.

> When the two disagree, this contract wins for the hand-authored layer; the spec wins for the
> derived layer.

## Protocol facts (the API the SDK speaks)

- Base URL `https://api.api2convert.com/v2`. Auth header `X-Api2convert-Api-Key: <key>` on account requests (canonical `X-Api2convert-*` names; the legacy `X-Oc-*` headers remain accepted by the API as permanent aliases).
- **Create job** `POST /jobs` `{ conversion:[{category?,target,options?}], input?:[…], process:bool,
  callback?, notify_status?, download_passwords?:[…] }` → response includes `id`, per-job `token`,
  per-job upload `server`, and `status.code`. `download_passwords` protects every output of the job;
  any password in the list then unlocks its downloads. The API never returns the plaintext back.
- **Upload** (not in the spec): `POST {server}/upload-file/{job_id}`, `multipart/form-data` field
  `file`, authenticated with the per-job **`X-Api2convert-Token`** header — never the account key.
- **Add remote input**: `POST /jobs/{id}/input` `{ type:'remote', source:'https://…' }`.
- **Cloud input** (import from customer storage): `POST /jobs/{id}/input` (or inline in `create`)
  `{ type:'cloud', source:<provider>, parameters:{…}, credentials:{…} }`, `<provider> ∈ {amazons3,
  azure, ftp, googlecloud}`; plus `{ type:'gdrive_picker', source:<drive-file-id>,
  credentials:{token}, content_type? }` for Google Drive. The API validates a cloud descriptor
  **asynchronously** — it accepts any descriptor on create (`201`) and later fails a bad one on the
  input (`status:failed`, generic `code 99`); it never echoes a credential value.
- **Cloud output** (deliver to customer storage): a `conversion[]` may carry
  `output_target:[{ type:<provider>, parameters:{…}, credentials:{…} }]`, `<provider> ∈ {amazons3,
  googlecloud, azure, ftp, youtube, gdrive}`. `status` (`waiting|uploading|completed|failed`) is
  server-set and read-only — never sent on create. The job reaches `completed` only after the upload
  succeeds (a failed upload → `failed`); an output-target conversion produces **no** local output.
- **Start**: `PATCH /jobs/{id}` `{ process:true }`.
- **Poll**: `GET /jobs/{id}` → terminal when `status.code ∈ {completed, failed, canceled}`
  (`failed`/`canceled` are unsuccessful terminals; non-terminal: `created`, `incomplete`,
  `downloading`, `queued`, `processing`, and any unknown code). Poll with backoff; clamp the
  interval to a floor (never busy-loop) and the total wait to a ceiling (never poll unbounded).
- **Download**: `GET output.uri` — self-contained, no auth; `X-Api2convert-Download-Password` header if set.
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
  `X-Api2convert-Download-Password` header on every download from that result/handle — callers do not
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

## Cloud storage connectors

The API imports inputs from and delivers outputs to customer-owned cloud storage. The SDK models the
wire descriptors above; per-provider keys are **not** validated synchronously server-side, so the
typed surface is the client's only pre-flight structure.

- **Provider vocabulary** — one shared `CloudProvider` concept (per-language spelling): `amazons3,
  azure, ftp, gdrive, googlecloud, youtube`. It is **build-side vocabulary only** — read models keep
  `source`/`type`/`status` as raw strings, and an unknown provider from the server round-trips
  untyped (never throws).
- **Cloud input** — a `CloudInput` builder emits `{ type:cloud, source, parameters, credentials }`
  and hands off to `addInput` / the create path. It ships per-provider named constructors whose
  signatures carry each provider's required keys **verbatim** (flat/lowercase, as the API expects):
  `amazonS3(bucket, file, accesskeyid, secretaccesskey)`,
  `azure(container, file, accountname, accountkey)`, `ftp(host, file, username, password)`,
  `googleCloud(projectid, bucket, file, keyfile)`. The required keys are **constructor arguments**,
  not a runtime gate — the builder never rejects a descriptor the permissive server would accept, and
  a generic `parameters`/`credentials` map stays reachable for optional/forward-compat keys. Google
  Drive input uses `type:gdrive_picker` (`source` = the Drive file id, token in `credentials.token`),
  carried by the generic `addInput` raw-map path this wave (no typed builder yet). `gdrive` and
  `youtube` are **output-only** — they validate as an input `source` but have no downloader.
- **Cloud output** — an `OutputTarget` model (`type` = a `CloudProvider` + free-form
  `parameters`/`credentials`) attaches to a conversion, both via `convert`/`convertAsync` (a new
  optional `outputTargets` control, never merged into the options map) and the raw `jobs().create`
  conversion map. It serializes `{ type, parameters, credentials }` and **omits `status`** on create.
  Per-provider output factories are **not** in this wave (their keys live in a separate service and
  diverge per provider). When any output target is set, `convert` returns the completed `Job` and
  does **not** download — the conversion has no local output.
- **Read semantics** — `parameters` and the per-target `status` round-trip on read; `credentials`
  are **never** surfaced (the API returns them empty; the SDK does not hydrate them).

## Cross-cutting semantics
- **Auth**: account key as `X-Api2convert-Api-Key`; uploads use the per-job token.
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
- **Credential redaction**: cloud `credentials` travel in the request body — never derive error text
  from the request body, and mask the **whole `credentials` object** to `[REDACTED]` wherever a value
  object could surface it: object inspection (`toString`/`repr`/`Debug`/`inspect`), any SDK-emitted
  request log, and the decoded error body (belt-and-suspenders — the API does not echo credential
  values). Also mask any `parameters` leaf whose key contains a sensitive token (`token, password,
  passwd, secret, key, keyfile, credential, passphrase, sas, sig, signature`, case-insensitive).
  Credentials ride in the plaintext body — user-attached request logging must redact its own.
