use std::fs::{self, File};

use tinygit::{Repo, TinygitError, object::Object, storage::reference::RefPointer};

#[test]
fn repo_new_dir_structure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    let repo = Repo::new(path).expect("should be able to create repository");

    // assert .git structure
    assert!(path.join(".git").exists(), ".git should exist");
    assert!(
        path.join(".git/objects").exists(),
        ".git/objects should exist"
    );
    assert!(
        path.join(".git/objects/pack").exists(),
        ".git/objects/pack should exist"
    );

    // a fresh repository should be on an unborn branch
    let head = repo
        .store
        .reference(&"HEAD".into())
        .expect("should be able to read HEAD");
    assert_eq!(
        head.pointer,
        RefPointer::Symbolic("refs/heads/main".into()),
        "HEAD should point to refs/heads/main"
    );
}

#[test]
fn repo_new_create_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    let path = path.join("some-subdir");

    let _ = Repo::new(path).expect("should be able to create directory for repository");
}

#[test]
fn repo_new_reinitialize() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    // first repo
    let _ = Repo::new(path).expect("should be able to create repository");

    File::create(path.join("foo.bar")).expect("should be able to create file");

    // reinitialize repo
    let _ = Repo::new(path).expect("should be able to re-create repository");

    assert!(
        path.join("foo.bar").exists(),
        "reinitialization shouldn't remove repo files"
    );
}

#[test]
fn stage_file_creates_blob_and_index_entry() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let file_path = dir.path().join("hello.txt");
    fs::write(&file_path, b"hello world").expect("should be able to write file");

    repo.stage(&file_path).expect("should stage file");

    // Index should exist and contain one entry
    let index = repo.store.index().expect("should be able to read index");
    let entries: Vec<_> = index.entries().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path_name, "hello.txt");

    // Blob should exist in the loose object database
    let obj = repo
        .store
        .object(&entries[0].hash)
        .expect("should find object");
    assert!(matches!(obj, Object::Blob(_)));
}

#[test]
fn stage_replaces_existing_index_entry() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, b"version 1").expect("should be able to write file");
    repo.stage(&file_path).expect("should stage file");

    let index1 = repo.store.index().expect("should be able to read index");
    let entries1: Vec<_> = index1.entries().collect();
    assert_eq!(entries1.len(), 1);
    let hash1 = entries1[0].hash.clone();

    // Modify file and stage again
    fs::write(&file_path, b"version 2").expect("should be able to write file");
    repo.stage(&file_path).expect("should stage file");

    let index2 = repo.store.index().expect("should be able to read index");
    let entries2: Vec<_> = index2.entries().collect();
    assert_eq!(entries2.len(), 1);
    assert_eq!(entries2[0].path_name, "file.txt");
    assert_ne!(entries2[0].hash, hash1);
}

#[test]
fn stage_outside_repo_returns_not_in_repo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let outside_dir = tempfile::tempdir().unwrap();
    let outside_file = outside_dir.path().join("outside.txt");
    fs::write(&outside_file, b"outside").expect("should be able to write file");

    let err = repo
        .stage(&outside_file)
        .expect_err("should fail for file outside repo");
    assert!(matches!(err, TinygitError::NotInRepo));
}

#[test]
fn stage_nonexistent_file_returns_invalid_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let nonexistent = dir.path().join("nonexistent.txt");
    let err = repo
        .stage(&nonexistent)
        .expect_err("should fail for nonexistent file");

    assert!(matches!(err, TinygitError::InvalidFileName));
}
