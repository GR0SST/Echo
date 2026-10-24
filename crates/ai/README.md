# User-owned AI

`echo_ai` has a data-only default build for the core browser model. The browser
enables `http` and `native`: direct blocking HTTP/SSE transports and a credential
store backed by macOS Keychain, Windows Credential Manager, or Linux Secret
Service. All I/O runs on workers. Credentials are opaque references in browser
sessions; actual keys and OAuth token sets are stored through `CredentialStore`.

Anthropic keys not scoped to one workspace can select a Workspace ID in Max
settings; it is sent only to Anthropic on inference and model-list requests.

Browser-context URLs strip credentials, queries and fragments. Non-web URLs
provide only their scheme, excluding file paths and inline document bodies.
The browser retains the exact local URL separately for stale-response checks.

Providers: OpenAI Responses, Anthropic Messages, and an OpenAI-compatible
`/chat/completions` endpoint. Remote endpoints require HTTPS; local compatible
providers may use loopback HTTP. Credentials in URLs and cross-origin HTTP
redirects are rejected. Streams must reach their documented terminal event;
interruption, provider errors, limits and cancellation do not become successes.

ChatGPT uses OpenAI's documented OSS dynamic-registration entrypoint with Echo's
own name, a persistent random host ID, loopback callback, fresh PKCE/state/nonce,
and the client ID issued for this registration. It does not use Codex's client
ID or ChatGPT backend endpoints. JWT validation requires RS256, a matching
published key, issuer, audience, expiry and nonce. Identity-only consent cannot
perform inference. Access and rotating refresh tokens are replaced together in
the OS credential record. Session locks coordinate refresh/revocation across
threads and processes using non-secret lock files in the user's cache directory.
Sign-out disconnects inference before revocation and never silently falls back
to a saved API key. The UI reports unconfirmed remote revocation.

Primary contracts inspected on 2026-10-24:

- [OpenAI registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [OpenAI accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)
- [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)
- [Preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations)
- [Anthropic authentication and workspace selection](https://platform.claude.com/docs/en/manage-claude/authentication)
- [Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [Keyring platform backends](https://docs.rs/keyring/3.6.3/keyring/)

Offline tests cover real loopback HTTP authorization and SSE, interrupted/error
streams, cancellation, public registration/PKCE, callback/client isolation,
signed JWT validation, and configuration without secrets. RSA test keys are
generated in memory. The ignored `native_store_round_trip` test creates and
removes only an isolated generated OS credential; it passed on macOS. Live
provider billing/account consent and Windows/Linux stores require validation
on those systems; there is no test that uses the owner's provider credentials.


Public-link previews read the destination anonymously before making a direct
provider request. The reader has no browser cookie jar or authorization headers.
It validates each redirect, restricts ports to HTTP/HTTPS, pins connections to
validated public DNS answers and rejects private/special IP ranges (including
IPv4-mapped IPv6). It accepts only HTML/plain text, caps the response at 2 MiB,
and extracts at most 30,000 characters while excluding scripts, forms and
navigation. Document text is untrusted model context; model output is bounded
heading/fact text, never executable actions or replacement destination URLs.
Private/Blank windows do not request previews. Disabling the feature, changing
the source owner/document/provider or dismissing its panel cancels pending work.
The tested macOS fixture saved its synthetic key through Echo Settings, made
one loopback SSE request for a public Wikipedia destination and removed that
credential after clean native termination. No owner credential was used.
