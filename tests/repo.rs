use std::fs::{self, File};

use chrono::{DateTime, FixedOffset};
use tinygit::{
    Repo, TinygitError,
    identity::{Email, Identity, Name},
    object::{Object, blob::Blob, hash::ObjectHash},
    storage::reference::RefPointer,
    time::Timestamp,
};

fn fixed_identity() -> Identity {
    Identity::new(
        Name::try_new("Unknown".into()).expect("should be able to create name"),
        Email::new("your.email@soon.com".into()),
    )
}

fn fixed_timestamp() -> Timestamp {
    Timestamp::try_new(
        DateTime::from_timestamp(1_700_000_000, 0)
            .expect("should be able to create datetime")
            .with_timezone(&FixedOffset::east_opt(0).expect("should be able to create offset")),
    )
    .expect("should be able to create timestamp")
}

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

#[test]
fn write_object_writes_blob_and_returns_hash() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let blob = Object::Blob(Blob(b"hello world".to_vec()));
    let hash = repo
        .store
        .write_object(&blob)
        .expect("should be able to write object");

    assert_eq!(
        hash.to_string(),
        "95d09f2b10159347eece71399a7e2e907ea3df4f",
        "blob hash should match Git's"
    );

    // writing an object that already exists is not an error
    let again = repo
        .store
        .write_object(&blob)
        .expect("writing an existing object should not fail");

    assert_eq!(again, hash, "hash should not change");
}

#[test]
fn write_tree_on_empty_index_returns_empty_tree_hash() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let hash = repo
        .store
        .write_tree()
        .expect("should be able to write tree");

    assert_eq!(
        hash.to_string(),
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
        "empty tree hash should match Git's"
    );

    let object = repo
        .store
        .object(&hash)
        .expect("tree should be present in the store");

    assert!(matches!(object, Object::Tree(_)), "object should be a tree");
}

#[test]
fn write_tree_contains_staged_entries() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let file_path = dir.path().join("hello.txt");
    fs::write(&file_path, b"hello world").expect("should be able to write file");
    repo.stage(&file_path).expect("should stage file");

    let (blob_hash, mode) = {
        let index = repo.store.index().expect("should be able to read index");
        let entry = index.entries().next().expect("index should have an entry");

        (entry.hash.clone(), entry.mode.raw() as u16)
    };

    let hash = repo
        .store
        .write_tree()
        .expect("should be able to write tree");

    let Object::Tree(tree) = repo
        .store
        .object(&hash)
        .expect("tree should be present in the store")
    else {
        panic!("object should be a tree");
    };

    assert_eq!(tree.entries.len(), 1, "tree should have one entry");

    let (key, entry) = tree
        .entries
        .iter()
        .next()
        .expect("tree should have an entry");
    assert_eq!(key.as_str(), "hello.txt", "entry name should match");
    assert_eq!(key.mode, mode, "entry mode should match the index");
    assert_eq!(entry.object, blob_hash, "entry hash should match the index");
}

#[test]
fn set_head_moves_symbolic_head_target() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let hash = ObjectHash::from([1u8; 20]);
    repo.store
        .set_head(hash.clone())
        .expect("should be able to set HEAD");

    let head = repo
        .store
        .reference(&"HEAD".into())
        .expect("should be able to read HEAD");
    assert_eq!(
        head.pointer,
        RefPointer::Symbolic("refs/heads/main".into()),
        "HEAD should stay symbolic"
    );

    let main = repo
        .store
        .reference(&"refs/heads/main".into())
        .expect("should be able to read the branch");
    assert_eq!(
        main.pointer,
        RefPointer::Hash(hash),
        "branch should point to the hash"
    );
}

#[test]
fn set_head_moves_detached_head() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let old = ObjectHash::from([1u8; 20]);
    let new = ObjectHash::from([2u8; 20]);

    repo.store
        .set_reference(&"HEAD".into(), RefPointer::Hash(old))
        .expect("should be able to detach HEAD");

    repo.store
        .set_head(new.clone())
        .expect("should be able to set HEAD");

    let head = repo
        .store
        .reference(&"HEAD".into())
        .expect("should be able to read HEAD");
    assert_eq!(
        head.pointer,
        RefPointer::Hash(new),
        "HEAD should point to the new hash"
    );
}

#[test]
fn commit_writes_tree_and_commit_and_moves_head() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let file_path = dir.path().join("hello.txt");
    fs::write(&file_path, b"hello world").expect("should be able to write file");
    repo.stage(&file_path).expect("should stage file");

    let hash = repo
        .store
        .commit(
            "initial commit",
            (fixed_identity(), fixed_timestamp()),
            (fixed_identity(), fixed_timestamp()),
        )
        .expect("should be able to commit");

    let branch = repo
        .store
        .reference(&"refs/heads/main".into())
        .expect("should be able to read the branch");
    assert_eq!(
        branch.pointer,
        RefPointer::Hash(hash.clone()),
        "branch should point to the commit"
    );

    let (commit, head_hash) = repo.store.head().expect("should be able to read HEAD");
    assert_eq!(head_hash, hash, "HEAD should resolve to the commit");
    assert_eq!(
        commit.description, "initial commit\n",
        "message should end with a newline"
    );
    assert_eq!(commit.author.0, fixed_identity(), "author should match");
    assert!(
        commit.parents.is_empty(),
        "first commit should have no parents"
    );

    let Object::Tree(tree) = repo
        .store
        .object(&commit.tree)
        .expect("tree should be present in the store")
    else {
        panic!("commit tree should be a tree object");
    };

    assert_eq!(tree.entries.len(), 1, "tree should have one entry");
}

#[test]
fn second_commit_uses_first_commit_as_parent() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let file_path = dir.path().join("hello.txt");
    fs::write(&file_path, b"hello world").expect("should be able to write file");
    repo.stage(&file_path).expect("should stage file");

    let first = repo
        .store
        .commit(
            "first",
            (fixed_identity(), fixed_timestamp()),
            (fixed_identity(), fixed_timestamp()),
        )
        .expect("should be able to commit");

    let second = repo
        .store
        .commit(
            "second",
            (fixed_identity(), fixed_timestamp()),
            (fixed_identity(), fixed_timestamp()),
        )
        .expect("should be able to commit");

    let (commit, _) = repo.store.head().expect("should be able to read HEAD");

    assert_eq!(
        commit.parents,
        vec![first],
        "second commit should have the first as its only parent"
    );

    let branch = repo
        .store
        .reference(&"refs/heads/main".into())
        .expect("should be able to read the branch");
    assert_eq!(
        branch.pointer,
        RefPointer::Hash(second),
        "branch should point to the second commit"
    );
}

#[test]
fn commit_with_empty_index_returns_empty_tree() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let err = repo
        .store
        .commit(
            "empty",
            (fixed_identity(), fixed_timestamp()),
            (fixed_identity(), fixed_timestamp()),
        )
        .expect_err("commit should fail on an empty index");

    assert!(matches!(err, TinygitError::EmptyTree));
}

#[test]
fn commit_does_not_change_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    let file_path = dir.path().join("hello.txt");
    fs::write(&file_path, b"hello world").expect("should be able to write file");
    repo.stage(&file_path).expect("should stage file");

    let index_path = repo.store.index_path();
    let before = fs::read(&index_path).expect("index should exist");

    repo.store
        .commit(
            "initial commit",
            (fixed_identity(), fixed_timestamp()),
            (fixed_identity(), fixed_timestamp()),
        )
        .expect("should be able to commit");

    let after = fs::read(&index_path).expect("index should exist");

    assert_eq!(before, after, "commit should not change the index");
}

#[test]
fn open_closest_finds_repo_at_root() {
    let dir = tempfile::tempdir().unwrap();
    Repo::new(dir.path()).expect("should create repo");

    let repo = Repo::open_closest(dir.path()).expect("should find repo at root");
    assert_eq!(
        repo.path.canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn open_closest_finds_repo_from_nested_subdir() {
    let dir = tempfile::tempdir().unwrap();
    Repo::new(dir.path()).expect("should create repo");

    let nested = dir.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested).expect("should create nested dirs");

    let repo = Repo::open_closest(&nested).expect("should find repo from nested dir");
    assert_eq!(
        repo.path.canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn open_closest_finds_repo_from_file_path() {
    let dir = tempfile::tempdir().unwrap();
    Repo::new(dir.path()).expect("should create repo");

    let nested_file = dir.path().join("subdir").join("file.txt");
    fs::create_dir_all(nested_file.parent().unwrap()).expect("should create subdir");
    fs::write(&nested_file, b"content").expect("should create file");

    let repo = Repo::open_closest(&nested_file).expect("should find repo from file path");
    assert_eq!(
        repo.path.canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn open_closest_fails_outside_repo() {
    let dir = tempfile::tempdir().unwrap();
    let err = Repo::open_closest(dir.path()).expect_err("should fail when no .git is found");
    assert!(matches!(err, TinygitError::NoGitDirectory));
}
