# Echo releases

The static hosting infrastructure, server configuration, TLS/cache notes,
provisioning, deploy-user setup and rollback documentation live in the private
repository [GR0SST/echo-update-host](https://github.com/GR0SST/echo-update-host).
No accounts, telemetry, synchronization or application backend are involved.

Production base URL: `https://updates.echo-browser.app/`.
Manifest URL: `https://updates.echo-browser.app/manifest.json` (404 until the
first real signed release is published). The isolated smoke channel is
`https://updates.echo-browser.app/test/manifest.json`; never use its test key
as the production trust root. The host transports signatures; client-side
verification remains mandatory. Prefer a signed envelope; separate manifest
and signature requests can straddle a deployment and must retry on mismatch.

Publish a signed release directory with `scripts/publish-release.sh <release-dir>`.
Configure the `echo-updates-deploy` SSH alias as described in that repository;
the account is `echo-deploy`, with no sudo. Set `ECHO_UPDATE_CHANNEL=test` for
smoke fixtures; production defaults to `stable`. `ECHO_UPDATE_KEEP` defaults to
5 (minimum 2); the host also retains at least seven days of releases.

The directory must contain `manifest.json` (signed envelope or accompanied by
`manifest.json.sig` / `manifest.sig`) and all full/delta artifacts at their
manifest-relative paths. Artifact paths are immutable: include a version or
content hash; changing bytes at an existing path is rejected. Dotfiles and
symlinks are rejected. Only the three root metadata filenames are mutable.
Upload completes before an atomic manifest/signature generation switch.
