use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use book_extension_runtime::{
    corpus_build_profile::{CorpusManifest, ManifestSource},
    corpus_download::{DownloadError, collect_manifest},
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn serve_once(payload: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
        .unwrap();
        stream.write_all(&payload).unwrap();
    });
    (format!("http://{address}/fixture"), handle)
}

fn manifest(url: String, payload: &[u8], sha256: String) -> CorpusManifest {
    CorpusManifest {
        manifest_version: 1,
        created_at_utc: "2026-07-15T00:00:00Z".to_owned(),
        sources: vec![ManifestSource {
            site: "wcsc".to_owned(),
            event: "fixture".to_owned(),
            year: 2026,
            retrieved_at: 1,
            url,
            relative_path: "nested/fixture.zip".to_owned(),
            size: payload.len() as u64,
            sha256,
        }],
    }
}

#[test]
fn downloads_to_partial_verifies_and_reuses_frozen_cache() {
    let payload = b"frozen payload".to_vec();
    let (url, server) = serve_once(payload.clone());
    let expected = format!("{:x}", Sha256::digest(&payload));
    let manifest = manifest(url, &payload, expected);
    let dir = TempDir::new().unwrap();

    let first = collect_manifest(&manifest, dir.path()).unwrap();
    server.join().unwrap();
    let second = collect_manifest(&manifest, dir.path()).unwrap();

    assert_eq!(first.downloaded, 1);
    assert_eq!(second.reused, 1);
    assert_eq!(
        std::fs::read(dir.path().join("nested/fixture.zip")).unwrap(),
        payload
    );
    assert!(dir.path().join("snapshot.json").is_file());
    assert!(
        std::fs::read_dir(dir.path().join("nested"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".part"))
    );
}

#[test]
fn hash_mismatch_never_becomes_the_cache_entry() {
    let payload = b"wrong payload".to_vec();
    let (url, server) = serve_once(payload.clone());
    let manifest = manifest(url, &payload, "11".repeat(32));
    let dir = TempDir::new().unwrap();

    let error = collect_manifest(&manifest, dir.path()).unwrap_err();
    server.join().unwrap();

    assert!(matches!(error, DownloadError::HashMismatch { .. }));
    assert!(!dir.path().join("nested/fixture.zip").exists());
}
