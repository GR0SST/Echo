use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn fixture() -> (SigningKey, Manifest, Vec<u8>) {
    let key = SigningKey::from_bytes(&[17; 32]);
    let bytes = b"new release".to_vec();
    let hash = hex::encode(Sha256::digest(&bytes));
    let asset = VelopackAsset {
        PackageId: "Echo".into(),
        Version: "0.2.0".into(),
        Type: "Full".into(),
        FileName: "Echo-0.2.0-full.nupkg".into(),
        SHA256: hash.clone(),
        Size: bytes.len() as u64,
        ..Default::default()
    };
    let signature = STANDARD.encode(key.sign(&artifact_statement(&hash, asset.Size)).to_bytes());
    let manifest = Manifest {
        package_hashes: BTreeMap::from([(asset.FileName.clone(), "ab".repeat(32))]),
        schema: 1,
        channel: "stable".into(),
        platform: platform(),
        expires: now() + 86400,
        signatures: BTreeMap::from([(asset.FileName.clone(), signature)]),
        feed: VelopackAssetFeed {
            Assets: vec![asset],
        },
    };
    (key, manifest, bytes)
}
fn envelope(key: &SigningKey, manifest: &Manifest) -> Vec<u8> {
    let payload = serde_json::to_vec(&Catalog {
        schema: 1,
        channel: manifest.channel.clone(),
        platforms: BTreeMap::from([(manifest.platform.clone(), manifest.clone())]),
    })
    .unwrap();
    serde_json::to_vec(&Envelope {
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
        payload: STANDARD.encode(payload),
    })
    .unwrap()
}
#[test]
fn manifest_authentication_precedes_parsing_and_rejects_tampering() {
    let (key, manifest, _) = fixture();
    let bytes = envelope(&key, &manifest);
    assert_eq!(
        parse(&bytes, &key.verifying_key(), "stable", &platform(), now())
            .unwrap()
            .feed
            .Assets[0]
            .Version,
        "0.2.0"
    );
    let mut altered: Envelope = serde_json::from_slice(&bytes).unwrap();
    altered.payload = STANDARD.encode(b"{}");
    assert!(parse(
        &serde_json::to_vec(&altered).unwrap(),
        &key.verifying_key(),
        "stable",
        &platform(),
        now()
    )
    .is_err());
    assert!(parse(
        &bytes,
        &SigningKey::from_bytes(&[18; 32]).verifying_key(),
        "stable",
        &platform(),
        now()
    )
    .is_err());
    assert!(parse(
        b"not json",
        &key.verifying_key(),
        "stable",
        &platform(),
        now()
    )
    .is_err());
}
#[test]
fn scope_expiration_asset_paths_and_channels_fail_closed() {
    let (key, mut manifest, _) = fixture();
    let bytes = envelope(&key, &manifest);
    for (channel, target) in [
        ("beta", platform()),
        ("stable", "not-published-platform".into()),
        ("../stable", platform()),
    ] {
        assert!(parse(&bytes, &key.verifying_key(), channel, &target, now()).is_err());
    }
    assert!(parse(
        &bytes,
        &key.verifying_key(),
        "stable",
        &platform(),
        manifest.expires
    )
    .is_err());
    manifest.feed.Assets[0].FileName = "../escape".into();
    assert!(parse(
        &envelope(&key, &manifest),
        &key.verifying_key(),
        "stable",
        &platform(),
        now()
    )
    .is_err());
    let (_, mut manifest, _) = fixture();
    manifest.feed.Assets.push(manifest.feed.Assets[0].clone());
    assert!(parse(
        &envelope(&key, &manifest),
        &key.verifying_key(),
        "stable",
        &platform(),
        now()
    )
    .is_err());
}
#[test]
fn artifact_bytes_and_signature_are_both_required() {
    let (key, mut manifest, bytes) = fixture();
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), &bytes).unwrap();
    let asset = manifest.feed.Assets[0].clone();
    assert!(verify_artifact(file.path(), &asset, &manifest, &key.verifying_key()).is_ok());
    fs::write(file.path(), b"tampered!!!").unwrap();
    assert!(verify_artifact(file.path(), &asset, &manifest, &key.verifying_key()).is_err());
    fs::write(file.path(), bytes).unwrap();
    manifest
        .signatures
        .insert(asset.FileName.clone(), STANDARD.encode([0; 64]));
    assert!(verify_artifact(file.path(), &asset, &manifest, &key.verifying_key()).is_err());
}
#[test]
fn semantic_versions_and_prerelease_channels() {
    for (old, new, channel, expected) in [
        ("1.9.0", "1.10.0", "stable", true),
        ("1.0.0", "1.0.0", "stable", false),
        ("1.0.0+aaa", "1.0.0+bbb", "stable", false),
        ("2.0.0", "1.0.0", "stable", false),
        ("1.0.0", "2.0.0-beta.1", "stable", false),
        ("1.0.0", "2.0.0-beta.1", "beta", true),
        ("2.0.0-beta.1", "2.0.0", "stable", true),
    ] {
        assert_eq!(newer(old, new, channel).unwrap(), expected);
    }
    assert!(newer("bad", "1.0.0", "stable").is_err());
}
#[test]
fn static_server_serves_signed_manifest_and_artifact_without_identifiers() {
    use std::net::TcpListener;
    let (key, manifest, artifact) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/echo/", listener.local_addr().unwrap());
    let bytes = envelope(&key, &manifest);
    let name = manifest.feed.Assets[0].FileName.clone();
    let thread = std::thread::spawn(move || {
        for (path, body) in [
            ("/echo/manifest.json".to_string(), bytes),
            (format!("/echo/{}/{name}", platform()), artifact),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap();
            assert_eq!(
                request.lines().next().unwrap(),
                format!("GET {path} HTTP/1.1")
            );
            assert!(!request.to_lowercase().contains("cookie:"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    let source = SignedSource::new(base, "stable".into(), platform(), key.verifying_key()).unwrap();
    let authenticated = source.load().unwrap();
    let output = tempfile::NamedTempFile::new().unwrap();
    source
        .download_release_entry(&authenticated.feed.Assets[0], output.path(), None)
        .unwrap();
    thread.join().unwrap();
}
#[test]
fn urls_cannot_carry_identifiers_or_use_plaintext_remote_transport() {
    let (key, _, _) = fixture();
    for url in [
        "http://example.com/",
        "https://example.com/?user=123",
        "https://me@example.com/",
        "file:///tmp/",
    ] {
        assert!(
            SignedSource::new(url.into(), "stable".into(), platform(), key.verifying_key())
                .is_err()
        );
    }
}

#[test]
fn reconstructed_zip_requires_signed_content_proof_but_not_identical_compression() {
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};
    let a = tempfile::NamedTempFile::new().unwrap();
    let b = tempfile::NamedTempFile::new().unwrap();
    let c = tempfile::NamedTempFile::new().unwrap();
    let write = |path: &Path, method, content: &[u8]| {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        zip.start_file(
            "lib/app/Echo",
            SimpleFileOptions::default()
                .compression_method(method)
                .unix_permissions(0o755),
        )
        .unwrap();
        zip.write_all(content).unwrap();
        zip.finish().unwrap();
    };
    write(
        a.path(),
        CompressionMethod::Stored,
        b"signed application bytes",
    );
    write(
        b.path(),
        CompressionMethod::Deflated,
        b"signed application bytes",
    );
    write(
        c.path(),
        CompressionMethod::Stored,
        b"tampered application bytes",
    );
    assert_ne!(digest(a.path()).unwrap(), digest(b.path()).unwrap());
    assert_eq!(
        package_digest(a.path()).unwrap(),
        package_digest(b.path()).unwrap()
    );
    let (key, mut manifest, _) = fixture();
    let asset = manifest.feed.Assets[0].clone();
    manifest
        .package_hashes
        .insert(asset.FileName.clone(), package_digest(a.path()).unwrap());
    let authenticated = parse(
        &envelope(&key, &manifest),
        &key.verifying_key(),
        "stable",
        &platform(),
        now(),
    )
    .unwrap();
    assert!(verify_package(b.path(), &asset, &authenticated).is_ok());
    assert!(verify_package(c.path(), &asset, &authenticated).is_err());
}

#[test]
fn delta_selection_requires_a_verified_base_and_an_unbroken_release_chain() {
    let (_, mut manifest, _) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let base_file = dir.path().join("Echo-0.1.0-full.nupkg");
    let mut zip = zip::ZipWriter::new(File::create(&base_file).unwrap());
    zip.start_file("lib/app/Echo", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"old executable").unwrap();
    zip.finish().unwrap();
    let mut base = manifest.feed.Assets[0].clone();
    base.Version = "0.1.0".into();
    base.FileName = "Echo-0.1.0-full.nupkg".into();
    manifest
        .package_hashes
        .insert(base.FileName.clone(), package_digest(&base_file).unwrap());
    let mut delta = manifest.feed.Assets[0].clone();
    delta.Type = "Delta".into();
    delta.FileName = "Echo-0.2.0-delta.nupkg".into();
    delta.Size = 1;
    manifest.feed.Assets.extend([base, delta]);
    let update = choose_update(&manifest, "0.1.0", dir.path())
        .unwrap()
        .unwrap();
    assert!(update.BaseRelease.is_some());
    assert_eq!(update.DeltasToTarget.len(), 1);
    assert!(choose_update(&manifest, "0.2.0", dir.path())
        .unwrap()
        .is_none());
    assert!(choose_update(&manifest, "0.3.0", dir.path())
        .unwrap()
        .is_none());
    let mut intermediate = manifest.feed.Assets[0].clone();
    intermediate.Version = "0.1.5".into();
    intermediate.FileName = "Echo-0.1.5-full.nupkg".into();
    manifest.feed.Assets.push(intermediate);
    assert!(choose_update(&manifest, "0.1.0", dir.path())
        .unwrap()
        .unwrap()
        .BaseRelease
        .is_none());
    manifest.feed.Assets.pop();
    fs::write(base_file, b"tampered cache").unwrap();
    assert!(choose_update(&manifest, "0.1.0", dir.path())
        .unwrap()
        .unwrap()
        .BaseRelease
        .is_none());
}

#[test]
fn one_root_catalog_authenticates_platform_selection() {
    let (key, native, _) = fixture();
    let mut windows = native.clone();
    windows.platform = if platform() == "windows-x86_64" {
        "linux-x86_64"
    } else {
        "windows-x86_64"
    }
    .into();
    let other_platform = windows.platform.clone();
    windows.feed.Assets[0].Version = "0.3.0".into();
    let catalog = Catalog {
        schema: 1,
        channel: "stable".into(),
        platforms: BTreeMap::from([
            (native.platform.clone(), native),
            (windows.platform.clone(), windows),
        ]),
    };
    let payload = serde_json::to_vec(&catalog).unwrap();
    let bytes = serde_json::to_vec(&Envelope {
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
        payload: STANDARD.encode(payload),
    })
    .unwrap();
    assert_eq!(
        parse(&bytes, &key.verifying_key(), "stable", &platform(), now())
            .unwrap()
            .feed
            .Assets[0]
            .Version,
        "0.2.0"
    );
    assert_eq!(
        parse(
            &bytes,
            &key.verifying_key(),
            "stable",
            &other_platform,
            now()
        )
        .unwrap()
        .feed
        .Assets[0]
            .Version,
        "0.3.0"
    );
    assert!(parse(
        &bytes,
        &key.verifying_key(),
        "stable",
        "missing-platform",
        now()
    )
    .is_err());
    assert!(parse(&bytes, &key.verifying_key(), "test", &platform(), now()).is_err());
}

#[test]
fn missing_release_feed_is_a_recoverable_error_and_never_pins_a_manifest() {
    use std::net::TcpListener;
    let (key, _, _) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"GET /manifest.json HTTP/1.1\r\n"));
        stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let source = SignedSource::new(base, "stable".into(), platform(), key.verifying_key()).unwrap();
    assert!(source.load().err().expect("404 must fail").to_string().contains("404"));
    assert!(source.pinned().is_err());
    server.join().unwrap();
}
