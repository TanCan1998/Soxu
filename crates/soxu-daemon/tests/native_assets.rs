mod common;

use std::fs;
use std::path::PathBuf;

use common::fixture_root;
use soxu_daemon::NativeAssetGuard;
use tempfile::TempDir;

fn guard(dir: &TempDir) -> NativeAssetGuard {
    let root = fixture_root(dir);
    fs::create_dir(root.join("config")).expect("create isolated config");
    fs::write(root.join("config/hooks.json"), b"trusted").expect("write fixture");
    NativeAssetGuard::new(
        root,
        [(PathBuf::from("config/hooks.json"), b"trusted".to_vec())],
    )
    .expect("declare trusted bytes")
}

#[test]
fn verifies_again_on_every_launch_and_rejects_same_length_tampering() {
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    let guard = guard(&dir);
    let work = root.join("work");
    fs::create_dir(&work).expect("create launch directory");
    let outside = TempDir::new().expect("outside directory");
    assert!(
        guard
            .verify_launch_directory(&fixture_root(&outside))
            .is_err()
    );
    guard.verify_launch_directory(&work).expect("first launch");
    fs::write(root.join("config/hooks.json"), b"allowed").expect("tamper fixture");
    assert!(guard.verify_launch_directory(&work).is_err());
    fs::write(root.join("config/hooks.json"), b"trusted").expect("restore fixture");
    guard
        .verify_launch_directory(&work)
        .expect("restored content");
    fs::rename(
        root.join("config/hooks.json"),
        root.join("saved-hooks.json"),
    )
    .expect("simulate missing asset");
    assert!(guard.verify_launch_directory(&work).is_err());
}

#[test]
fn expected_bytes_are_not_learned_from_existing_files() {
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    fs::write(root.join("hooks.json"), b"allow").expect("untrusted bytes");
    let guard = NativeAssetGuard::new(root, [(PathBuf::from("hooks.json"), b"deny!".to_vec())])
        .expect("declare expectation without accepting disk bytes");
    assert!(guard.verify().is_err());
}

#[test]
fn rejects_relative_escape_duplicate_empty_and_oversized_declarations() {
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    for path in ["", "../hooks.json", "config/../hooks.json", "/hooks.json"] {
        assert!(
            NativeAssetGuard::new(root.clone(), [(PathBuf::from(path), b"deny".to_vec())],)
                .is_err()
        );
    }
    assert!(
        NativeAssetGuard::new(
            PathBuf::from("relative"),
            [(PathBuf::from("hooks.json"), vec![])]
        )
        .is_err()
    );
    assert!(NativeAssetGuard::new(root.clone(), []).is_err());
    assert!(
        NativeAssetGuard::new(
            root.clone(),
            [
                (PathBuf::from("hooks.json"), vec![]),
                (PathBuf::from("hooks.json"), vec![]),
            ]
        )
        .is_err()
    );
    assert!(
        NativeAssetGuard::new(
            root,
            [(PathBuf::from("hooks.json"), vec![0; 1024 * 1024 + 1])]
        )
        .is_err()
    );
}

#[test]
fn rejects_project_customizations_in_launch_directory_and_ancestors() {
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    let guard = guard(&dir);
    let work = root.join("work");
    fs::create_dir(&work).expect("launch directory");
    fs::create_dir(root.join(".agents")).expect("ancestor customizations");
    assert!(guard.verify_launch_directory(&work).is_err());
    fs::rename(root.join(".agents"), root.join("saved-agents")).expect("remove ancestor override");
    fs::write(work.join(".agents"), b"not a directory").expect("project path collision");
    assert!(guard.verify_launch_directory(&work).is_err());
}

#[test]
fn rejects_directory_in_place_of_asset_and_bounded_size_changes() {
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    let guard = guard(&dir);
    let asset = root.join("config/hooks.json");
    fs::rename(&asset, root.join("original.json")).expect("move fixture");
    fs::create_dir(&asset).expect("directory replacement");
    assert!(guard.verify().is_err());
    fs::rename(&asset, root.join("saved-dir")).expect("move directory");
    fs::write(&asset, vec![0; 1024 * 1024 + 1]).expect("oversized file");
    assert!(guard.verify().is_err());
}

#[cfg(windows)]
#[test]
fn rejects_windows_junction_ancestor_replacement() {
    use std::process::Command;
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    let guard = guard(&dir);
    fs::rename(root.join("config"), root.join("saved-config")).expect("move fixture directory");
    let junction = root.join("config");
    let destination = root.join("saved-config");
    let status = Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&destination)
        .output()
        .expect("create isolated junction");
    assert!(status.status.success(), "fixture junction must exist");
    assert!(guard.verify().is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlink_assets_and_ancestor_directories() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().expect("isolated directory");
    let root = fixture_root(&dir);
    let guard = guard(&dir);
    let asset = root.join("config/hooks.json");
    fs::rename(&asset, root.join("original.json")).expect("move fixture");
    symlink(root.join("original.json"), &asset).expect("asset symlink");
    assert!(guard.verify().is_err());
    // 有意构造的链接必须原样交给守卫，因此这里不做任何规范化。
    let linked = root.join("linked-root");
    symlink(&root, &linked).expect("linked root");
    assert!(
        NativeAssetGuard::new(
            linked,
            [(PathBuf::from("config/hooks.json"), b"trusted".to_vec())]
        )
        .is_err()
    );
}
