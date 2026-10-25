# GitHub Live Folders

Echo contacts `https://api.github.com` directly using the user's fine-grained
personal access token. `CredentialStore` is the existing OS-keychain interface;
only its opaque key ID, GitHub login and cached PR metadata enter the local
browser session. No browser cookies, Arc credentials, OAuth client IDs or Echo
server are used. Reassigning a Space clears its connection and cached private metadata;
cross-profile folder moves are rejected by the existing browser model.

Read-only calls: `/user`, `/search/issues` and (for Team Review Requests)
`/user/teams`. Repository access is limited to what the user grants their token;
team membership from a fine-grained token is limited to its single organization
resource owner. Organization approval may be required. Invalid or
expired credentials require reconnection. The client never sends a token to a
response-supplied host or follows redirects with it. Errors exclude response
bodies and credentials.

Polling uses per-account/per-query/per-page ETags, 304 reuse, endpoint polling
intervals and server rate-limit deadlines. Pagination must complete before the
model receives a snapshot. Incomplete search responses or more than GitHub's
1,000-result search ceiling preserve the previous snapshot. Unread state tracks
PR ID and update revision, independently of repository visibility.

References (consulted October 25, 2026):
- https://docs.github.com/en/rest/search/search#search-issues-and-pull-requests
- https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api
- https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens
- https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow

Device authorization is documented, but Echo has no registered GitHub OAuth app
ID configured. This implementation uses the explicitly supported PAT route.
Tests inject a mocked REST transport and an in-memory credential store; no real
account is required. UI evidence and limitations live in
`target/parity/live-folders`.
