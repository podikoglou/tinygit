mod common;

use std::fs::{self, OpenOptions};
use std::io::Write;

use tinygit::Repo;

#[test]
fn stage_repo_root_respects_ignore_rules() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_nested_directory_respects_its_gitignore() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");
    let nested = dir.path().join("nested");

    repo.stage(&nested).expect("should stage directory");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_non_canonical_path_respects_nested_gitignore() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");
    let path = dir.path().join("nested/..");

    repo.stage(&path).expect("should stage directory");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_ignored_directory_does_not_apply_its_gitignore() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");
    let target = dir.path().join("target");

    repo.stage(&target).expect("should succeed");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_keeps_entry_of_tracked_file_that_became_ignored() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");

    fs::write(dir.path().join("src/.gitignore"), b"*.rs\n").unwrap();
    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/.gitignore",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_ignored_file_directly_does_not_stage_it() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");
    let ignored_file = dir.path().join("debug.log");

    repo.stage(&ignored_file).expect("should succeed");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_dot_git_directly_does_not_stage_it() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");
    let git_dir = dir.path().join(".git");

    repo.stage(&git_dir).expect("should succeed");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only",
            "src/main.rs"
        ]
    );
}

#[test]
fn stage_does_not_add_ignored_file_that_replaced_a_tracked_directory() {
    let (repo, dir) = include_repo!("fixtures/repos/repo-11.tar");
    let src = dir.path().join("src");

    fs::remove_dir_all(&src).unwrap();
    fs::write(&src, b"now a file").unwrap();
    let mut gitignore = OpenOptions::new()
        .append(true)
        .open(dir.path().join(".gitignore"))
        .unwrap();
    writeln!(gitignore, "src").unwrap();

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "important.log",
            "nested/.gitignore",
            "nested/sub/local_only"
        ]
    );
}

#[test]
fn stage_matches_wildcards_like_git() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should be able to create repository");

    fs::write(
        dir.path().join(".gitignore"),
        b"**/generated.txt\nlocal?.txt\nbuild[0-9].log\nsrc/*.log\n",
    )
    .unwrap();

    fs::create_dir_all(dir.path().join("nested/deep")).unwrap();
    fs::create_dir_all(dir.path().join("src/nested")).unwrap();

    for path in [
        "generated.txt",
        "nested/deep/generated.txt",
        "local1.txt",
        "build2.log",
        "src/root.log",
        "src/nested/inside.log",
    ] {
        fs::write(dir.path().join(path), b"").unwrap();
    }

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec![".gitignore", "src/nested/inside.log"]);
}
