use std::fs;
use std::path::PathBuf;

use soxu_daemon::NativeAssetGuard;
use tempfile::TempDir;

fn guard(root: &TempDir) -> NativeAssetGuard {
    fs::create_dir(root.path().join("config")).expect("create isolated config");
    fs::write(root.path().join("config/hooks.json"), b"trusted").expect("write fixture");
    NativeAssetGuard::new(
        root.path().to_owned(),
        [(PathBuf::from("config/hooks.json"), b"trusted".to_vec())],
    )
    .expect("declare trusted bytes")
}

#[test]
fn verifies_again_on_every_launch_and_rejects_same_length_tampering() {
    let root = TempDir::new().expect("isolated directory");
    let guard = guard(&root);
    let work = root.path().join("work");
    fs::create_dir(&work).expect("create launch directory");
    let outside = TempDir::new().expect("outside directory");
    assert!(guard.verify_launch_directory(outside.path()).is_err());
    guard.verify_launch_directory(&work).expect("first launch");
    fs::write(root.path().join("config/hooks.json"), b"allowed").expect("tamper fixture");
    assert!(guard.verify_launch_directory(&work).is_err());
    fs::write(root.path().join("config/hooks.json"), b"trusted").expect("restore fixture");
    guard
        .verify_launch_directory(&work)
        .expect("restored content");
    fs::rename(
        root.path().join("config/hooks.json"),
        root.path().join("saved-hooks.json"),
    )
    .expect("simulate missing asset");
    assert!(guard.verify_launch_directory(&work).is_err());
}

#[test]
fn expected_bytes_are_not_learned_from_existing_files() {
    let root = TempDir::new().expect("isolated directory");
    fs::write(root.path().join("hooks.json"), b"allow").expect("untrusted bytes");
    let guard = NativeAssetGuard::new(
        root.path().to_owned(),
        [(PathBuf::from("hooks.json"), b"deny!".to_vec())],
    )
    .expect("declare expectation without accepting disk bytes");
    assert!(guard.verify().is_err());
}

#[test]
fn rejects_relative_escape_duplicate_empty_and_oversized_declarations() {
    let root = TempDir::new().expect("isolated directory");
    for path in ["", "../hooks.json", "config/../hooks.json", "/hooks.json"] {
        assert!(
            NativeAssetGuard::new(
                root.path().to_owned(),
                [(PathBuf::from(path), b"deny".to_vec())],
            )
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
    assert!(NativeAssetGuard::new(root.path().to_owned(), []).is_err());
    assert!(
        NativeAssetGuard::new(
            root.path().to_owned(),
            [
                (PathBuf::from("hooks.json"), vec![]),
                (PathBuf::from("hooks.json"), vec![]),
            ]
        )
        .is_err()
    );
    assert!(
        NativeAssetGuard::new(
            root.path().to_owned(),
            [(PathBuf::from("hooks.json"), vec![0; 1024 * 1024 + 1])]
        )
        .is_err()
    );
}

#[test]
fn rejects_project_customizations_in_launch_directory_and_ancestors() {
    let root = TempDir::new().expect("isolated directory");
    let guard = guard(&root);
    let work = root.path().join("work");
    fs::create_dir(&work).expect("launch directory");
    fs::create_dir(root.path().join(".agents")).expect("ancestor customizations");
    assert!(guard.verify_launch_directory(&work).is_err());
    fs::rename(
        root.path().join(".agents"),
        root.path().join("saved-agents"),
    )
    .expect("remove ancestor override");
    fs::write(work.join(".agents"), b"not a directory").expect("project path collision");
    assert!(guard.verify_launch_directory(&work).is_err());
}

#[test]
fn rejects_directory_in_place_of_asset_and_bounded_size_changes() {
    let root = TempDir::new().expect("isolated directory");
    let guard = guard(&root);
    let asset = root.path().join("config/hooks.json");
    fs::rename(&asset, root.path().join("original.json")).expect("move fixture");
    fs::create_dir(&asset).expect("directory replacement");
    assert!(guard.verify().is_err());
    fs::rename(&asset, root.path().join("saved-dir")).expect("move directory");
    fs::write(&asset, vec![0; 1024 * 1024 + 1]).expect("oversized file");
    assert!(guard.verify().is_err());
}

#[cfg(windows)]
#[test]
fn rejects_windows_junction_ancestor_replacement() {
    use std::process::Command;
    let root = TempDir::new().expect("isolated directory");
    let guard = guard(&root);
    fs::rename(root.path().join("config"), root.path().join("saved-config"))
        .expect("move fixture directory");
    let junction = root.path().join("config");
    let destination = root.path().join("saved-config");
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
    let root = TempDir::new().expect("isolated directory");
    let guard = guard(&root);
    let asset = root.path().join("config/hooks.json");
    fs::rename(&asset, root.path().join("original.json")).expect("move fixture");
    symlink(root.path().join("original.json"), &asset).expect("asset symlink");
    assert!(guard.verify().is_err());
    let linked = root.path().join("linked-root");
    symlink(root.path(), &linked).expect("linked root");
    assert!(
        NativeAssetGuard::new(
            linked,
            [(PathBuf::from("config/hooks.json"), b"trusted".to_vec())]
        )
        .is_err()
    );
}
