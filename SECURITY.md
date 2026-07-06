# Security

## Reporting a vulnerability

Please report security issues privately to **security@qaamgo.com**. Do not open a public issue for a
vulnerability. We aim to acknowledge reports promptly and coordinate a fix and disclosure.

## Secret-handling guarantees

The SDK sends three kinds of secret, all in custom `X-Oc-*` headers:

- the account **API key** (`X-Oc-Api-Key`) on account requests,
- the per-job **upload token** (`X-Oc-Token`) on uploads, and
- the **download password** (`X-Oc-Download-Password`) on protected downloads.

The SDK upholds the following, and each is covered by `tests/security.rs` (a black-box suite driving
the public API against real loopback servers):

1. **Secret-bearing requests never follow redirects.** HTTP clients forward custom headers across a
   cross-origin redirect, so an auto-following client could leak an `X-Oc-*` secret to a redirect
   target on another host. The default transport uses two `reqwest` clients: a **no-redirect** client
   for every secret-bearing request, and a redirect-following client used **only** for the
   self-contained, no-secret download path. A secret-bearing request that receives a redirect is
   surfaced as an error — never a silently-empty file, never a secret sent onward.

2. **Uploads use the per-job token, never the account key.** The multipart upload to the per-job
   server is authenticated with `X-Oc-Token` only; the account key is never attached to it.

3. **No secret ever appears in an error.** Error messages are built from the API response body or a
   status description — never from a header or a URL (a download URL may carry a signed token in its
   query, so transport errors never echo the URL).

4. **Downloads are written safely.** When saving into a directory, the API-supplied filename is
   reduced to a safe basename (path-traversal segments like `..` are stripped). A download that fails
   mid-stream removes the partial file, and `max_download_bytes` caps an unbounded response.

## Handling keys

Provide the API key via the `API2CONVERT_API_KEY` environment variable or a secret manager — never
hard-code it, commit it, or log it. Webhook signatures are verified with a constant-time HMAC-SHA256
comparison over the raw request body.
