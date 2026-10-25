//! Authenticated, anonymous static updates. No Velopack HTTP source is used:
//! it adds staged-user query parameters which Echo deliberately never sends.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{mpsc::Sender, Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use velopack::{
    sources::UpdateSource, UpdateInfo, UpdateManager, VelopackAsset, VelopackAssetFeed,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const DEFAULT_URL: &str = match option_env!("ECHO_UPDATE_URL") {
    Some(url) => url,
    None => "https://updates.echo-browser.app/",
};
pub const PUBLIC_KEY: &str = match option_env!("ECHO_UPDATE_PUBLIC_KEY") {
    Some(key) => key,
    None => include_str!("../../../config/update-public-key.txt"),
};
pub const CHANNEL: &str = match option_env!("ECHO_UPDATE_CHANNEL") {
    Some(channel) => channel,
    None => "stable",
};
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn channel_valid(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn file_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

/// Signature covers the exact decoded payload bytes, before JSON parsing.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub channel: String,
    pub platform: String,
    pub expires: u64,
    pub feed: VelopackAssetFeed,
    /// Ed25519 signatures over `artifact_statement`, including installers.
    pub signatures: BTreeMap<String, String>,
    /// Canonical ZIP entry digests of full packages, authenticated by the manifest.
    pub package_hashes: BTreeMap<String, String>,
}
/// One signed root catalog; platform artifact directories remain immutable.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub channel: String,
    pub platforms: BTreeMap<String, Manifest>,
}
pub fn read_catalog(bytes: &[u8], public_key: &VerifyingKey) -> Result<Catalog> {
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    let payload = STANDARD.decode(envelope.payload)?;
    verify(&payload, &envelope.signature, public_key)?;
    let catalog: Catalog = serde_json::from_slice(&payload)?;
    if catalog.schema != 1 || !channel_valid(&catalog.channel) {
        return Err("Invalid catalog scope".into());
    }
    for (target, manifest) in &catalog.platforms {
        if !file_name(target)
            || manifest.schema != 1
            || manifest.platform != *target
            || manifest.channel != catalog.channel
        {
            return Err("Invalid platform catalog entry".into());
        }
    }
    Ok(catalog)
}
pub fn artifact_statement(hash: &str, size: u64) -> Vec<u8> {
    format!(
        "echo-artifact-v1\n{}\n{}\n",
        hash.to_ascii_lowercase(),
        size
    )
    .into_bytes()
}
pub fn key(value: &str) -> Result<VerifyingKey> {
    Ok(VerifyingKey::from_bytes(
        &STANDARD
            .decode(value.trim())?
            .try_into()
            .map_err(|_| "Invalid public key length")?,
    )?)
}
fn verify(bytes: &[u8], signature: &str, key: &VerifyingKey) -> Result<()> {
    key.verify_strict(bytes, &Signature::from_slice(&STANDARD.decode(signature)?)?)?;
    Ok(())
}
pub fn parse(
    bytes: &[u8],
    public_key: &VerifyingKey,
    channel: &str,
    target: &str,
    clock: u64,
) -> Result<Manifest> {
    let catalog = read_catalog(bytes, public_key)?;
    if catalog.channel != channel {
        return Err("Catalog channel mismatch".into());
    }
    let manifest = catalog
        .platforms
        .get(target)
        .ok_or("Platform is not published")?
        .clone();
    if manifest.schema != 1
        || manifest.channel != channel
        || manifest.platform != target
        || !channel_valid(channel)
    {
        return Err("Manifest scope mismatch".into());
    }
    if manifest.expires <= clock || manifest.expires > clock.saturating_add(366 * 86400) {
        return Err("Manifest expired or clock incorrect".into());
    }
    let mut names = std::collections::HashSet::new();
    for asset in &manifest.feed.Assets {
        let version = semver::Version::parse(&asset.Version)?;
        if asset.PackageId != "Echo"
            || !file_name(&asset.FileName)
            || !names.insert(&asset.FileName)
            || !matches!(asset.Type.as_str(), "Full" | "Delta")
            || asset.Size == 0
            || asset.Size > 4 * 1024 * 1024 * 1024
            || hex::decode(&asset.SHA256)?.len() != 32
            || (channel == "stable" && !version.pre.is_empty())
        {
            return Err("Invalid release asset".into());
        }
        if asset.Type == "Full"
            && manifest
                .package_hashes
                .get(&asset.FileName)
                .and_then(|hash| hex::decode(hash).ok())
                .is_none_or(|hash| hash.len() != 32)
        {
            return Err("Missing package content proof".into());
        }
        let signature = manifest
            .signatures
            .get(&asset.FileName)
            .ok_or("Unsigned release asset")?;
        verify(
            &artifact_statement(&asset.SHA256, asset.Size),
            signature,
            public_key,
        )?;
    }
    Ok(manifest)
}
pub fn digest(path: &Path) -> Result<(String, u64)> {
    let mut reader = File::open(path)?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 65536];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((hex::encode(hash.finalize()), size))
}
pub fn verify_artifact(
    path: &Path,
    asset: &VelopackAsset,
    manifest: &Manifest,
    public_key: &VerifyingKey,
) -> Result<()> {
    let (hash, size) = digest(path)?;
    if size != asset.Size || !hash.eq_ignore_ascii_case(&asset.SHA256) {
        return Err("Artifact checksum mismatch".into());
    }
    verify(
        &artifact_statement(&hash, size),
        manifest
            .signatures
            .get(&asset.FileName)
            .ok_or("Unsigned artifact")?,
        public_key,
    )
}

/// Canonical archive content: sorted UTF-8 entry names, modes, lengths, hashes.
/// Velopack delta reconstruction changes ZIP compression bytes, not this digest.
pub fn package_digest(path: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    if archive.len() > 100_000 {
        return Err("Too many package entries".into());
    }
    let mut entries = BTreeMap::new();
    let mut total = 0u64;
    let mut buffer = [0; 65536];
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.enclosed_name().is_none() {
            return Err("Unsafe package path".into());
        }
        let name = std::str::from_utf8(entry.name_raw())?.to_owned();
        let mode = entry.unix_mode().unwrap_or_default();
        let mut hash = Sha256::new();
        let mut size = 0u64;
        loop {
            let count = entry.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total.checked_add(count as u64).ok_or("Oversized package")?;
            if total > 8 * 1024 * 1024 * 1024 {
                return Err("Oversized package".into());
            }
            size += count as u64;
            hash.update(&buffer[..count]);
        }
        if entries
            .insert(name, (mode, size, hash.finalize()))
            .is_some()
        {
            return Err("Duplicate package entry".into());
        }
    }
    let mut hash = Sha256::new();
    hash.update(b"echo-package-v1\n");
    for (name, (mode, size, digest)) in entries {
        hash.update((name.len() as u64).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update(mode.to_be_bytes());
        hash.update(size.to_be_bytes());
        hash.update(digest);
    }
    Ok(hex::encode(hash.finalize()))
}
pub fn verify_package(path: &Path, asset: &VelopackAsset, manifest: &Manifest) -> Result<()> {
    let expected = manifest
        .package_hashes
        .get(&asset.FileName)
        .ok_or("Missing package proof")?;
    if !package_digest(path)?.eq_ignore_ascii_case(expected) {
        return Err("Reconstructed package content mismatch".into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct SignedSource {
    base: String,
    channel: String,
    target: String,
    key: VerifyingKey,
    manifest: Arc<Mutex<Option<Manifest>>>,
}
impl SignedSource {
    pub fn new(base: String, channel: String, target: String, key: VerifyingKey) -> Result<Self> {
        let parsed = url::Url::parse(&base)?;
        if parsed.scheme() != "https"
            && !(cfg!(debug_assertions)
                && parsed.scheme() == "http"
                && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        {
            return Err("Updates require HTTPS (debug builds allow loopback HTTP)".into());
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !channel_valid(&channel)
            || !file_name(&target)
        {
            return Err("Invalid update URL or channel".into());
        }
        Ok(Self {
            base: format!("{}/", base.trim_end_matches('/')),
            channel,
            target,
            key,
            manifest: Default::default(),
        })
    }
    fn fetch(&self, name: &str, output: &mut dyn Write, limit: u64) -> Result<()> {
        if name.split('/').any(|part| !file_name(part)) {
            return Err("Invalid artifact filename".into());
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(600))
            .timeout_connect(Duration::from_secs(15))
            .redirects(0)
            .build();
        let response = agent.get(&format!("{}{name}", self.base)).call()?;
        let copied = std::io::copy(&mut response.into_reader().take(limit + 1), output)?;
        if copied > limit {
            return Err("Update response too large".into());
        }
        Ok(())
    }
    pub fn load(&self) -> Result<Manifest> {
        let mut bytes = Vec::new();
        self.fetch("manifest.json", &mut bytes, 2 * 1024 * 1024)?;
        let manifest = parse(&bytes, &self.key, &self.channel, &self.target, now())?;
        *self.manifest.lock().map_err(|_| "Update lock poisoned")? = Some(manifest.clone());
        Ok(manifest)
    }
    fn pinned(&self) -> Result<Manifest> {
        self.manifest
            .lock()
            .map_err(|_| "Update lock poisoned")?
            .clone()
            .ok_or_else(|| "No authenticated manifest".into())
    }
}
fn vpk_error(error: impl std::fmt::Display) -> velopack::Error {
    velopack::Error::Other(error.to_string())
}
impl UpdateSource for SignedSource {
    fn get_release_feed(
        &self,
        channel: &str,
        _: &velopack::bundle::Manifest,
        _: &str,
    ) -> std::result::Result<VelopackAssetFeed, velopack::Error> {
        if channel != self.channel {
            return Err(vpk_error("Channel mismatch"));
        }
        self.load().map(|m| m.feed).map_err(vpk_error)
    }
    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        path: &Path,
        _: Option<Sender<i16>>,
    ) -> std::result::Result<(), velopack::Error> {
        let result = (|| -> Result<()> {
            let manifest = self.pinned()?;
            let signed = manifest
                .feed
                .Assets
                .iter()
                .find(|a| a.FileName == asset.FileName)
                .ok_or("Unknown artifact")?;
            if serde_json::to_vec(signed)? != serde_json::to_vec(asset)? {
                return Err("Artifact metadata changed".into());
            }
            let mut output = File::create(path)?;
            self.fetch(
                &format!("{}/{}", self.target, asset.FileName),
                &mut output,
                asset.Size,
            )?;
            output.sync_all()?;
            verify_artifact(path, asset, &manifest, &self.key)
        })();
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result.map_err(vpk_error)
    }
}

/// Verified pending update. Never uses `apply_updates_and_restart` (process::exit).
pub struct Ready {
    manager: UpdateManager,
    update: UpdateInfo,
    source: SignedSource,
    packages: PathBuf,
}
impl Ready {
    pub fn version(&self) -> &str {
        &self.update.TargetFullRelease.Version
    }
    pub fn arm_after_shutdown(&self) -> Result<()> {
        // Recheck cached/reconstructed packages immediately before handing to the installer.
        verify_package(
            &self.packages.join(&self.update.TargetFullRelease.FileName),
            &self.update.TargetFullRelease,
            &self.source.pinned()?,
        )?;
        self.manager.wait_exit_then_apply_updates(
            &self.update,
            true,
            true,
            std::iter::empty::<String>(),
        )?;
        Ok(())
    }
}
/// Select a monotonic release and a complete, bounded delta chain without
/// calling Velopack's staged-user/rollout APIs or creating a .betaId file.
fn choose_update(
    manifest: &Manifest,
    current: &str,
    packages: &Path,
) -> Result<Option<UpdateInfo>> {
    let mut full = manifest
        .feed
        .Assets
        .iter()
        .filter(|a| a.Type == "Full")
        .map(|a| Ok((semver::Version::parse(&a.Version)?, a)))
        .collect::<Result<Vec<_>>>()?;
    full.sort_by(|a, b| a.0.cmp_precedence(&b.0));
    let Some((_, target)) = full.last() else {
        return Ok(None);
    };
    if !newer(current, &target.Version, &manifest.channel)? {
        return Ok(None);
    }
    let mut update = UpdateInfo {
        TargetFullRelease: (*target).clone(),
        BaseRelease: None,
        DeltasToTarget: Vec::new(),
        IsDowngrade: false,
    };
    let Some((base_version, base)) = full.iter().find(|(_, a)| a.Version == current) else {
        return Ok(Some(update));
    };
    if verify_package(&packages.join(&base.FileName), base, manifest).is_err() {
        return Ok(Some(update));
    }
    let mut deltas = Vec::new();
    for (_, asset) in full
        .iter()
        .filter(|(version, _)| version.cmp_precedence(base_version).is_gt())
    {
        let Some(delta) = manifest
            .feed
            .Assets
            .iter()
            .find(|d| d.Type == "Delta" && d.Version == asset.Version)
        else {
            return Ok(Some(update));
        };
        deltas.push(delta.clone());
    }
    if !deltas.is_empty()
        && deltas.len() <= 10
        && deltas.iter().map(|d| d.Size).sum::<u64>() < target.Size
    {
        update.BaseRelease = Some((*base).clone());
        update.DeltasToTarget = deltas;
    }
    Ok(Some(update))
}

pub fn prepare() -> Result<Option<Ready>> {
    let base = std::env::var("ECHO_UPDATE_URL").unwrap_or_else(|_| DEFAULT_URL.into());
    let source = SignedSource::new(base, CHANNEL.into(), platform(), key(PUBLIC_KEY)?)?;
    // Check the authenticated endpoint even for local, unpackaged dogfood builds.
    // Installation still requires a valid Velopack locator below.
    let manifest = source.load()?;
    let locator = velopack::locator::auto_locate_app_manifest(
        velopack::locator::LocationContext::FromCurrentExe,
    )?;
    if locator.get_manifest_channel() != CHANNEL {
        return Err("Installed update channel does not match this build".into());
    }
    let manager = UpdateManager::new(
        source.clone(),
        Some(velopack::UpdateOptions {
            ExplicitChannel: Some(CHANNEL.into()),
            AllowVersionDowngrade: false,
            ..Default::default()
        }),
        None,
    )?;
    // SDK check_for_updates creates a persistent rollout identifier even with
    // a custom source. Select locally from our signed feed instead.
    let Some(update) = choose_update(
        &manifest,
        &locator.get_manifest_version().to_string(),
        &locator.get_packages_dir(),
    )?
    else {
        return Ok(None);
    };
    manager.download_updates(&update, None)?;
    let ready = Ready {
        manager,
        update,
        source,
        packages: locator.get_packages_dir(),
    };
    verify_package(
        &ready
            .packages
            .join(&ready.update.TargetFullRelease.FileName),
        &ready.update.TargetFullRelease,
        &ready.source.pinned()?,
    )?;
    Ok(Some(ready))
}

#[cfg(test)]
mod tests;

/// Disable implicit startup installation: only Echo's completed quit barrier can arm it.
pub fn startup() {
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();
}

/// Semver comparison never permits lateral moves, downgrades or stable prereleases.
pub fn newer(current: &str, candidate: &str, channel: &str) -> Result<bool> {
    if !channel_valid(channel) {
        return Err("Invalid channel".into());
    }
    let current = semver::Version::parse(current)?;
    let candidate = semver::Version::parse(candidate)?;
    Ok(candidate.cmp_precedence(&current).is_gt()
        && (channel != "stable" || candidate.pre.is_empty()))
}
