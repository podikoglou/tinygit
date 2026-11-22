use std::{fs, path::Path};

use git2::Repository;
use tempfile::TempDir;
use tinygit::{Ignore, Repo};

#[test]
fn gitignore_differential_matches_libgit2() {
    let dir = TempDir::new().unwrap();
    let git2_repo = Repository::init(dir.path()).unwrap();

    fs::write(
        dir.path().join(".gitignore"),
        b"*.log\ntarget/\n!important.log\n",
    )
    .unwrap();

    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("debug.log"), b"").unwrap();
    fs::write(dir.path().join("important.log"), b"").unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), b"").unwrap();

    fs::write(dir.path().join(".git/info/exclude"), b"local_exclude.txt\n").unwrap();

    fs::create_dir_all(dir.path().join("nested/sub")).unwrap();
    fs::write(dir.path().join("nested/.gitignore"), b"*.tmp\n/local_only\n").unwrap();
    fs::write(dir.path().join("nested/a.tmp"), b"").unwrap();
    fs::write(dir.path().join("nested/local_only"), b"").unwrap();
    fs::write(dir.path().join("nested/sub/local_only"), b"").unwrap();

    let ignore = Ignore::for_dir(dir.path()).unwrap();

    let test_cases = [
        (".git", true),
        ("target", true),
        ("debug.log", false),
        ("important.log", false),
        ("src/main.rs", false),
        (".gitignore", false),
        ("local_exclude.txt", false),
    ];

    for (path, is_dir) in test_cases {
        let expected = git2_repo.status_should_ignore(Path::new(path)).unwrap();
        let actual = ignore.is_ignored(path, is_dir);
        assert_eq!(actual, expected, "ignore mismatch for {path}");
    }

    let nested_dir = dir.path().join("nested");
    let nested_ignore = ignore.child(&nested_dir).unwrap();
    for (path, is_dir) in [
        ("nested/a.tmp", false),
        ("nested/local_only", false),
        ("nested/sub/local_only", false),
    ] {
        let expected = git2_repo.status_should_ignore(Path::new(path)).unwrap();
        assert_eq!(
            nested_ignore.is_ignored(path, is_dir),
            expected,
            "nested ignore mismatch for {path}"
        );
    }

    let direct_nested_ignore = Ignore::for_dir(&nested_dir).unwrap();
    for (path, is_dir) in [
        ("nested/a.tmp", false),
        ("nested/local_only", false),
        ("nested/sub/local_only", false),
    ] {
        let expected = git2_repo.status_should_ignore(Path::new(path)).unwrap();
        assert_eq!(
            direct_nested_ignore.is_ignored(path, is_dir),
            expected,
            "direct nested ignore mismatch for {path}"
        );
    }
}

#[test]
fn stage_repo_root_ignores_git_directory() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    fs::write(dir.path().join("README.md"), b"hello").unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), b"fn main() {}").unwrap();

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec!["README.md", "src/main.rs"]);
    assert!(!paths.iter().any(|p| p.starts_with(".git")));
}

#[test]
fn stage_respects_root_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    fs::write(
        dir.path().join(".gitignore"),
        b"*.log\ntarget/\nbuild/\n!important.log\n",
    )
    .unwrap();

    fs::write(dir.path().join("debug.log"), b"debug").unwrap();
    fs::write(dir.path().join("important.log"), b"keep me").unwrap();

    fs::create_dir_all(dir.path().join("target/debug")).unwrap();
    fs::write(dir.path().join("target/debug/output.bin"), b"binary").unwrap();

    fs::create_dir_all(dir.path().join("build")).unwrap();
    fs::write(dir.path().join("build/artifact.o"), b"artifact").unwrap();

    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), b"pub fn run() {}").unwrap();

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec![".gitignore", "important.log", "src/lib.rs"]);
}

#[test]
fn stage_respects_nested_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    let nested = dir.path().join("nested");
    fs::create_dir_all(nested.join("ignored_dir")).unwrap();

    fs::write(nested.join(".gitignore"), b"*.tmp\nignored_dir/\n").unwrap();
    fs::write(nested.join("a.tmp"), b"temp").unwrap();
    fs::write(nested.join("keep.txt"), b"keep").unwrap();
    fs::write(nested.join("ignored_dir/data.bin"), b"data").unwrap();

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec!["nested/.gitignore", "nested/keep.txt"]);
}

#[test]
fn stage_nested_directory_respects_its_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();

    fs::write(nested.join(".gitignore"), b"*.tmp\n").unwrap();
    fs::write(nested.join("a.tmp"), b"temp").unwrap();
    fs::write(nested.join("keep.txt"), b"keep").unwrap();

    repo.stage(&nested).expect("should stage directory");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec!["nested/.gitignore", "nested/keep.txt"]);
}

#[test]
fn stage_ignored_directory_does_not_apply_its_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    let ignored = dir.path().join("ignored");
    fs::create_dir(&ignored).unwrap();

    fs::write(dir.path().join(".gitignore"), b"ignored/\n").unwrap();
    fs::write(ignored.join(".gitignore"), b"!*\n").unwrap();
    fs::write(ignored.join("file.txt"), b"content").unwrap();

    repo.stage(&ignored).expect("should succeed");

    let index = repo.store.index().expect("should read index");
    assert_eq!(index.entries().count(), 0);
}

#[test]
fn stage_keeps_entry_of_tracked_file_that_became_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    fs::write(dir.path().join("tracked.log"), b"tracked").unwrap();
    repo.stage(dir.path()).expect("should stage repo root");

    fs::write(dir.path().join(".gitignore"), b"*.log\n").unwrap();
    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec![".gitignore", "tracked.log"]);
}

#[test]
fn stage_ignored_file_directly_does_not_stage_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    fs::write(dir.path().join(".gitignore"), b"ignored.txt\n").unwrap();
    let ignored_file = dir.path().join("ignored.txt");
    fs::write(&ignored_file, b"content").unwrap();

    repo.stage(&ignored_file).expect("should succeed");

    let index = repo.store.index().expect("should read index");
    assert_eq!(index.entries().count(), 0);
}

#[test]
fn stage_dot_git_directly_does_not_stage_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    let git_dir = dir.path().join(".git");
    repo.stage(&git_dir).expect("should succeed");

    let index = repo.store.index().expect("should read index");
    assert_eq!(index.entries().count(), 0);
}

#[test]
fn stage_respects_git_info_exclude() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    fs::create_dir_all(dir.path().join(".git/info")).unwrap();
    fs::write(dir.path().join(".git/info/exclude"), b"local_secret.txt\n").unwrap();
    fs::write(dir.path().join("local_secret.txt"), b"secret").unwrap();
    fs::write(dir.path().join("tracked.txt"), b"hello").unwrap();

    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();
    assert_eq!(paths, vec!["tracked.txt"]);
}

#[test]
fn stage_does_not_add_ignored_file_that_replaced_a_tracked_directory() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::new(dir.path()).expect("should create repo");

    let thing = dir.path().join("thing");
    fs::create_dir(&thing).unwrap();
    fs::write(thing.join("child"), b"child").unwrap();
    repo.stage(dir.path()).expect("should stage repo root");

    fs::remove_dir_all(&thing).unwrap();
    fs::write(&thing, b"now a file").unwrap();
    fs::write(dir.path().join(".gitignore"), b"thing\n").unwrap();
    repo.stage(dir.path()).expect("should stage repo root");

    let index = repo.store.index().expect("should read index");
    let paths: Vec<_> = index.entries().map(|e| e.path_name.to_string()).collect();

    assert_eq!(paths, vec![".gitignore"]);
}
