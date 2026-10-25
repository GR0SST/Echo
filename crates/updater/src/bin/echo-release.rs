//! Offline release signing. Private key bytes are never logged or copied into output.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use echo_updater::{artifact_statement, digest, Catalog, Envelope, Manifest, Result};
use ed25519_dalek::{Signer, SigningKey};
use std::{collections::BTreeMap, fs, path::Path};
fn main() {
    if let Err(error) = run() {
        eprintln!("Release signing failed: {error}");
        std::process::exit(1);
    }
}
fn in_repository(path: &Path) -> bool {
    path.ancestors()
        .any(|directory| directory.join(".git").exists())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") if args.len() == 4 => {
            let path = Path::new(&args[2]);
            let repo = std::env::current_dir()?.canonicalize()?;
            let parent = path.parent().ok_or("Key needs an absolute parent directory")?.canonicalize()?;
            if !path.is_absolute() || parent.starts_with(&repo) || in_repository(&parent) { return Err("Private key must be outside the repository".into()); }
            let key = SigningKey::generate(&mut rand::rngs::OsRng);
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
            use std::io::Write;
            options.open(path)?.write_all(&key.to_bytes())?;
            fs::write(&args[3], format!("{}\n", STANDARD.encode(key.verifying_key().as_bytes())))?;
            println!("Signing key created outside the repo. Public key written to {}. Back up the private key offline before releasing.", args[3]);
        }
        Some("sign") if args.len() == 7 => {
            let private = Path::new(&args[2]).canonicalize()?;
            if in_repository(&private) || private.starts_with(std::env::current_dir()?.canonicalize()?) { return Err("Private key must be outside the repository".into()); }
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; if fs::metadata(&private)?.permissions().mode() & 0o077 != 0 { return Err("Private key permissions must be 0600".into()); } }
            let key_bytes = fs::read(private)?;
            let key = SigningKey::from_bytes(&key_bytes.try_into().map_err(|_| "Expected a 32-byte signing key")?);
            if key.verifying_key() != echo_updater::key(echo_updater::PUBLIC_KEY)? { return Err("Signing key does not match the embedded public key".into()); }
            let dir = Path::new(&args[3]);
            let channel = &args[4];
            let platform = &args[5];
            if !echo_updater::channel_valid(channel) || platform.is_empty() || !platform.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) { return Err("Invalid channel/platform".into()); }
            let output = Path::new(&args[6]);
            if output.file_name().and_then(|n| n.to_str()) != Some("manifest.json") { return Err("Output must be a root manifest.json".into()); }
            let root = output.parent().ok_or("Output directory required")?;
            let exported = root.join(platform);
            fs::create_dir_all(&exported)?;
            let feed: velopack::VelopackAssetFeed = serde_json::from_slice(&fs::read(dir.join(format!("releases.{channel}.json")))?)?;
            let version = feed.Assets.iter().filter(|a| a.Type == "Full").map(|a| semver::Version::parse(&a.Version)).collect::<std::result::Result<Vec<_>, _>>()?.into_iter().max().ok_or("Empty release feed")?.to_string();
            let mut signatures = BTreeMap::new();
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                let Some(extension) = path.extension().and_then(|v| v.to_str()) else { continue; };
                if !path.is_file() || !matches!(extension, "nupkg" | "pkg" | "zip" | "exe" | "AppImage") { continue; }
                let file = path.file_name().unwrap().to_str().ok_or("Non-UTF8 filename")?;
                // vpk's installer/portable names are mutable; publish versioned copies.
                let name = if extension == "nupkg" { file.to_owned() } else { format!("{}-{version}.{extension}", path.file_stem().unwrap().to_str().ok_or("Non-UTF8 filename")?) };
                let (hash, size) = digest(&path)?;
                let signature = STANDARD.encode(key.sign(&artifact_statement(&hash, size)).to_bytes());
                copy_immutable(&path, &exported.join(&name))?;
                signatures.insert(name, signature);
            }
            let mut package_hashes = BTreeMap::new();
            for asset in feed.Assets.iter().filter(|asset| asset.Type == "Full") {
                package_hashes.insert(asset.FileName.clone(), echo_updater::package_digest(&dir.join(&asset.FileName))?);
            }
            let manifest = Manifest { package_hashes, schema: 1, channel: channel.clone(), platform: platform.clone(), expires: echo_updater::now() + 365 * 86400, feed, signatures };
            for asset in &manifest.feed.Assets { echo_updater::verify_artifact(&exported.join(&asset.FileName), asset, &manifest, &key.verifying_key())?; }
            let mut catalog = if output.exists() { echo_updater::read_catalog(&fs::read(output)?, &key.verifying_key())? }
                else { Catalog { schema: 1, channel: channel.clone(), platforms: BTreeMap::new() } };
            if catalog.channel != *channel { return Err("Cannot merge different channels into one catalog".into()); }
            catalog.platforms.insert(platform.clone(), manifest);
            let payload = serde_json::to_vec(&catalog)?;
            let envelope = Envelope { signature: STANDARD.encode(key.sign(&payload).to_bytes()), payload: STANDARD.encode(payload) };
            let bytes = serde_json::to_vec_pretty(&envelope)?;
            echo_updater::parse(&bytes, &key.verifying_key(), channel, platform, echo_updater::now())?;
            fs::write(output.with_extension("tmp"), &bytes)?;
            fs::rename(output.with_extension("tmp"), output)?;
        }
        _ => return Err("Usage: echo-release keygen ABSOLUTE_PRIVATE_PATH PUBLIC_PATH | sign PRIVATE_PATH VPK_CACHE_DIR CHANNEL PLATFORM OUTPUT_ROOT/manifest.json".into()),
    }
    Ok(())
}

fn copy_immutable(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        if destination.is_symlink() || digest(source)? != digest(destination)? {
            return Err(format!(
                "Refusing to replace immutable artifact {}",
                destination.display()
            )
            .into());
        }
        return Ok(());
    }
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_export_rejects_repackaging_without_touching_published_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("published");
        fs::write(&source, b"signed release").unwrap();
        copy_immutable(&source, &destination).unwrap();
        copy_immutable(&source, &destination).unwrap();
        fs::write(&source, b"different bytes at same version").unwrap();
        assert!(copy_immutable(&source, &destination).is_err());
        assert_eq!(fs::read(destination).unwrap(), b"signed release");
    }
}
