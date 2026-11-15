use std::collections::BTreeMap;

use fallible_iterator::FallibleIterator;
use tinygit::{
    Repo, TinygitError,
    fs::FileName,
    object::{
        Object,
        blob::Blob,
        hash::ObjectHash,
        tree::{Tree, TreeEntry, TreeKey},
    },
};

mod common;

fn write_blob(repo: &Repo, contents: &[u8]) -> ObjectHash {
    repo.store
        .write_object(&Object::Blob(Blob(contents.to_vec())))
        .expect("should write blob")
}

fn write_tree(repo: &Repo, entries: &[(&str, u16, &ObjectHash)]) -> ObjectHash {
    let entries = entries
        .iter()
        .map(|(name, mode, hash)| {
            (
                TreeKey::new(FileName::try_new((*name).to_owned()).unwrap(), *mode),
                TreeEntry::new((*hash).clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    repo.store
        .write_object(&Object::Tree(Tree::new(entries)))
        .expect("should write tree")
}

fn paths(repo: &Repo, root: &ObjectHash) -> Vec<String> {
    repo.store
        .walk_tree(root)
        .expect("should start walk")
        .map(|entry| Ok(entry.path))
        .collect()
        .expect("should read entries")
}

#[test]
fn walks_nested_trees_in_git_order() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let repo = Repo::new(dir.path()).expect("should create repo");

    let baz = write_blob(&repo, b"baz");
    let foo = write_tree(&repo, &[("baz", 0o100644, &baz)]);
    let foo_bar = write_blob(&repo, b"foo-bar");
    let z = write_blob(&repo, b"z");
    let root = write_tree(
        &repo,
        &[
            ("foo", 0o40000, &foo),
            ("foo-bar", 0o100644, &foo_bar),
            ("z", 0o100644, &z),
        ],
    );

    assert_eq!(paths(&repo, &root), ["foo-bar", "foo", "foo/baz", "z"]);
}

#[test]
fn yields_directory_entries() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let repo = Repo::new(dir.path()).expect("should create repo");

    let file = write_blob(&repo, b"file");
    let subtree = write_tree(&repo, &[("file", 0o100644, &file)]);
    let root = write_tree(&repo, &[("dir", 0o40000, &subtree)]);

    let entries = repo
        .store
        .walk_tree(&root)
        .expect("should start walk")
        .collect::<Vec<_>>()
        .expect("should read entries");

    assert_eq!(entries[0].path, "dir");
    assert!(entries[0].key.is_tree());
    assert_eq!(entries[0].key.mode, 0o40000);
    assert_eq!(entries[1].path, "dir/file");
    assert!(!entries[1].key.is_tree());
}

#[test]
fn does_not_descend_into_gitlinks() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let repo = Repo::new(dir.path()).expect("should create repo");

    let child = write_blob(&repo, b"child");
    let subtree = write_tree(&repo, &[("child", 0o100644, &child)]);

    // The hash points at a tree, but mode 160000 marks a commit. The walker
    // must yield the gitlink and must not descend into it.
    let root = write_tree(&repo, &[("submodule", 0o160000, &subtree)]);

    assert_eq!(paths(&repo, &root), ["submodule"]);
}

#[test]
fn yields_an_error_for_a_missing_subtree() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let repo = Repo::new(dir.path()).expect("should create repo");

    let missing = ObjectHash::from([0x11; 20]);
    let root = write_tree(&repo, &[("missing", 0o40000, &missing)]);

    let mut walk = repo.store.walk_tree(&root).expect("should start walk");
    let result = walk.next();

    assert!(result.is_err(), "a missing subtree should yield an error");
}

#[test]
fn rejects_a_non_tree_root() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let repo = Repo::new(dir.path()).expect("should create repo");

    let blob = write_blob(&repo, b"not a tree");

    assert!(matches!(
        repo.store.walk_tree(&blob),
        Err(TinygitError::UnexpectedObjectType)
    ));
}

#[test]
fn walks_a_real_repository() {
    let (repo, _dir) = include_repo!("fixtures/repos/repo-8.tar");

    let (head, _) = repo.store.head().expect("should find HEAD");

    assert_eq!(
        paths(&repo, &head.tree),
        ["README.md", "foo-bar", "foo.c", "foo", "foo/bar.rs"]
    );
}
