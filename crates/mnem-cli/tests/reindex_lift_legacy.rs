//! Integration test for `mnem reindex --lift-legacy-extra` (G19).
//!
//! Verifies that legacy v0.3 nodes carrying `extra["embed"]` are
//! promoted to the embedding sidecar (`Commit.embeddings`) without
//! re-deriving from text, and that `NodeCid` is unchanged after the lift.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use assert_cmd::prelude::*;
use bytes::Bytes;
use mnem_backend_redb::open_or_init;
use mnem_core::codec::to_canonical_bytes;
use mnem_core::id::{Cid, NodeId};
use mnem_core::objects::node::{Dtype, Embedding};
use mnem_core::objects::Node;
use mnem_core::repo::ReadonlyRepo;
use mnem_core::store::Blockstore;
use tempfile::TempDir;

fn mnem(repo: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("mnem").expect("built mnem binary");
    cmd.current_dir(repo);
    cmd.arg("-R").arg(repo);
    for a in args {
        cmd.arg(a);
    }
    cmd
}

fn init(dir: &Path) {
    mnem(dir, &["init", dir.to_str().unwrap()])
        .assert()
        .success();
}

/// Open the redb store and return (blockstore, repo).
fn open_repo(dir: &Path) -> (Arc<dyn Blockstore>, ReadonlyRepo) {
    let db = dir.join(".mnem").join("repo.redb");
    let (bs, ohs, _) = open_or_init(&db).expect("open redb");
    let repo = ReadonlyRepo::open(bs.clone(), ohs).expect("open repo");
    (bs, repo)
}

/// Build a minimal `Embedding` with a 2-element f32 vector.
fn test_embedding(model: &str) -> Embedding {
    // Two f32 values: 1.0 and 0.5 packed as little-endian bytes.
    let mut buf = Vec::with_capacity(8);
    buf.extend_from_slice(&1.0_f32.to_le_bytes());
    buf.extend_from_slice(&0.5_f32.to_le_bytes());
    Embedding {
        model: model.to_string(),
        dtype: Dtype::F32,
        dim: 2,
        vector: Bytes::from(buf),
    }
}

/// Encode `Embedding` as an `Ipld` value so we can store it in
/// `node.extra["embed"]`. We go via DAG-CBOR bytes -> `Ipld` using
/// the same codec the Node decoder uses, so the round-trip is exact.
fn embedding_to_ipld(emb: &Embedding) -> ipld_core::ipld::Ipld {
    // Serialize the Embedding to DAG-CBOR bytes, then deserialize as Ipld.
    let bytes = to_canonical_bytes(emb).expect("encode Embedding");
    serde_ipld_dagcbor::from_slice(&bytes).expect("decode as Ipld")
}

/// Commit a node that has `extra["embed"]` set (simulating a v0.3 node).
/// Returns the `NodeCid` of the committed node and the model string.
fn seed_legacy_node(dir: &Path) -> (Cid, String) {
    let model = "legacy-test-model:v0.3".to_string();
    let emb = test_embedding(&model);

    // Open the repo for writing.
    let db = dir.join(".mnem").join("repo.redb");
    let (bs, ohs, _) = open_or_init(&db).expect("open redb");
    let repo = ReadonlyRepo::open(bs, ohs)
        .or_else(|e| {
            if e.is_uninitialized() {
                let db2 = dir.join(".mnem").join("repo.redb");
                let (bs2, ohs2, _) = open_or_init(&db2).expect("reopen");
                ReadonlyRepo::init(bs2, ohs2).map_err(Into::into)
            } else {
                Err(anyhow::anyhow!("{e}"))
            }
        })
        .expect("open or init repo");

    let mut tx = repo.start_transaction();

    // Build a node with extra["embed"] set to the Ipld encoding of emb.
    let mut node = Node::new(NodeId::from_bytes_raw([42u8; 16]), "LegacyDoc")
        .with_summary("a legacy document with inline embed");
    node.extra
        .insert("embed".to_string(), embedding_to_ipld(&emb));

    let node_cid = tx.add_node(&node).expect("add node");

    // Commit WITHOUT calling set_embedding - the embedding is only in extra.
    let r2 = tx.commit("test author", "seed legacy node").expect("commit");

    // Verify: sidecar must be empty for this node (no set_embedding was called).
    assert!(
        r2.embedding_for(&node_cid, &model)
            .expect("embedding_for")
            .is_none(),
        "sidecar must be empty before lift"
    );

    (node_cid, model)
}

#[test]
fn lift_legacy_extra_promotes_embedding_to_sidecar() {
    let dir = TempDir::new().unwrap();
    init(dir.path());

    // Seed a node with extra["embed"] via the Rust API.
    let (node_cid, model) = seed_legacy_node(dir.path());

    // Run `mnem reindex --lift-legacy-extra` via the CLI binary.
    let out = mnem(dir.path(), &["reindex", "--lift-legacy-extra"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        stdout.contains("lifted") || stdout.contains("embedding"),
        "expected lift confirmation in stdout, got: {stdout}"
    );

    // Re-open the repo and verify the sidecar now has the embedding.
    let (_bs, repo) = open_repo(dir.path());
    let got = repo
        .embedding_for(&node_cid, &model)
        .expect("embedding_for after lift");
    assert!(
        got.is_some(),
        "sidecar must have the lifted embedding for model {model}"
    );

    let emb = got.unwrap();
    assert_eq!(emb.model, model);
    assert_eq!(emb.dim, 2);
    assert_eq!(emb.dtype, Dtype::F32);
    assert_eq!(emb.vector.len(), 8); // 2 * 4 bytes per f32
}

#[test]
fn lift_legacy_extra_nodecid_unchanged() {
    let dir = TempDir::new().unwrap();
    init(dir.path());

    // Seed a legacy node and record its NodeCid.
    let (node_cid_before, _model) = seed_legacy_node(dir.path());

    // Run lift.
    mnem(dir.path(), &["reindex", "--lift-legacy-extra"])
        .assert()
        .success();

    // The NodeCid must be unchanged: lift only writes to the sidecar,
    // never rewrites node bytes.
    let (bs, repo) = open_repo(dir.path());
    let head = repo.head_commit().expect("head commit after lift");

    // Walk the nodes tree and find our node using the already-open blockstore.
    let cursor = mnem_core::prolly::Cursor::new(&*bs, &head.nodes).expect("cursor");
    let mut found = false;
    for entry in cursor {
        let (_k, cid) = entry.expect("entry");
        if cid == node_cid_before {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "NodeCid {node_cid_before} must still be present in the nodes tree after lift"
    );
}

#[test]
fn lift_legacy_extra_dry_run_no_commit() {
    let dir = TempDir::new().unwrap();
    init(dir.path());

    seed_legacy_node(dir.path());

    // Capture op count before.
    let ops_before = {
        let out = mnem(dir.path(), &["log", "--oneline"]).assert().success();
        let s = String::from_utf8_lossy(&out.get_output().stdout).to_string();
        s.lines().filter(|l| !l.trim().is_empty()).count()
    };

    // Dry run should print a count but not commit.
    let out = mnem(dir.path(), &["reindex", "--lift-legacy-extra", "--dry-run"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        stdout.contains("would lift") || stdout.contains("1"),
        "dry-run must report candidate count, got: {stdout}"
    );

    // Op count must be unchanged.
    let ops_after = {
        let out = mnem(dir.path(), &["log", "--oneline"]).assert().success();
        let s = String::from_utf8_lossy(&out.get_output().stdout).to_string();
        s.lines().filter(|l| !l.trim().is_empty()).count()
    };
    assert_eq!(
        ops_before, ops_after,
        "--dry-run must not write a new commit"
    );
}

#[test]
fn lift_legacy_extra_and_force_are_mutually_exclusive() {
    let dir = TempDir::new().unwrap();
    init(dir.path());

    mnem(
        dir.path(),
        &["reindex", "--lift-legacy-extra", "--force"],
    )
    .assert()
    .failure();
}
