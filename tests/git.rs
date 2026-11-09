//! This is a differential fuzzing suite. It uses `bolero` to generate Git commands, runs them with
//! the real Git implementation, and we check against tinygit.

use bolero::{TypeGenerator, ValueGenerator, check, one_value_of, produce};
use git2::Repository;
use nutype::nutype;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, iter,
    path::Path,
    process::{Command, Output},
};
use strum::EnumDiscriminants;
use tempfile::TempDir;
use tinygit::storage::index::{Index, IndexEntry};

#[derive(Debug, Clone, TypeGenerator)]
struct Name(#[generator(produce::<String>().with().len(1..=255).chars('A'..='z'))] String);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Modification {
    Insert { offset: usize, bytes: Vec<u8> },
    Delete { offset: usize, size: usize },
}

impl Modification {
    fn apply(&self, contents: &mut Vec<u8>) {
        match self {
            Modification::Insert { offset, bytes } => {
                assert!(*offset <= contents.len(), "Insert offset out of bounds");
                contents.splice(*offset..*offset, bytes.iter().copied());
            }
            Modification::Delete { offset, size } => {
                assert!(
                    *offset + *size <= contents.len(),
                    "Delete range out of bounds"
                );
                contents.drain(*offset..*offset + *size);
            }
        }
    }
}

#[derive(Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(TypeGenerator))]
enum Op {
    Create(Name, Vec<u8>),
    Modify(Name, Modification),
    GitAdd(Vec<Name>),
    GitRm(Vec<Name>),
}

#[nutype(derive(Debug, Clone))]
struct Sequence(Vec<Op>);

impl TypeGenerator for Sequence {
    fn generate<D: bolero::Driver>(driver: &mut D) -> Option<Self> {
        let mut seq: Vec<Op> = Vec::default();

        // working tree: file path -> file contents
        let mut working: BTreeMap<String, Vec<u8>> = BTreeMap::default();

        // tracked files
        let mut index: BTreeSet<String> = BTreeSet::default();

        let n = (0..32usize).generate(driver)?;

        for _ in 0..n {
            let mut kinds = Vec::default();

            kinds.push(OpDiscriminants::Create);

            // modifying or staging is valid for any file that exists in the working tree.
            if !working.is_empty() {
                kinds.push(OpDiscriminants::Modify);
                kinds.push(OpDiscriminants::GitAdd);
            }

            // `git rm` is valid only for files tracked by the index.
            if !index.is_empty() {
                kinds.push(OpDiscriminants::GitRm);
            }

            let kind = one_value_of(&kinds[..]).generate(driver)?;

            let op = match kind {
                OpDiscriminants::Create => {
                    let name = Name::generate(driver)?;
                    let contents = <Vec<u8> as TypeGenerator>::generate(driver)?;

                    working.insert(name.0.clone(), contents.clone());

                    Op::Create(name, contents)
                }
                OpDiscriminants::Modify => {
                    let names: Vec<String> = working.keys().cloned().collect();
                    let name = Name(one_value_of(&names[..]).generate(driver)?);
                    let contents = working
                        .get_mut(&name.0)
                        .expect("file must exist in working tree");

                    let is_insert = if contents.is_empty() {
                        true
                    } else {
                        <bool as TypeGenerator>::generate(driver)?
                    };

                    let delta = if is_insert {
                        let offset = (0..=contents.len()).generate(driver)?;
                        let bytes = <Vec<u8> as TypeGenerator>::generate(driver)?;

                        Modification::Insert { offset, bytes }
                    } else {
                        let offset = (0..contents.len()).generate(driver)?;
                        let max_size = contents.len() - offset;
                        let size = (0..=max_size).generate(driver)?;

                        Modification::Delete { offset, size }
                    };

                    delta.apply(contents);

                    Op::Modify(name, delta)
                }
                OpDiscriminants::GitAdd => {
                    let names: Vec<String> = working.keys().cloned().collect();
                    let name = Name(one_value_of(&names[..]).generate(driver)?);

                    index.insert(name.0.clone());

                    Op::GitAdd(vec![name])
                }
                OpDiscriminants::GitRm => {
                    let names: Vec<String> = index.iter().cloned().collect();
                    let name = Name(one_value_of(&names[..]).generate(driver)?);

                    assert!(
                        working.remove(&name.0).is_some(),
                        "name should exist in the working tree"
                    );
                    assert!(index.remove(&name.0), "name should exist in the index");

                    Op::GitRm(vec![name])
                }
            };

            seq.push(op);
        }

        Some(Self::new(seq))
    }
}

fn run_tinygit(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tinygit"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("should be able to run tinygit")
}

/// All the properties that libgit2 reports for an index entry.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    path: String,
    mode: u32,
    hash: String,
    ctime_seconds: u32,
    ctime_ns_fractions: u32,
    mtime_seconds: u32,
    mtime_ns_fractions: u32,
    dev: u32,
    ino: u32,
    uid: u32,
    gid: u32,
    file_size: u32,
    flags: u16,
}

impl From<&IndexEntry> for Entry {
    fn from(entry: &IndexEntry) -> Self {
        let mode = (u32::from(u8::from(entry.mode.object_type())) << 12)
            | u32::from(entry.mode.permission());
        let flags = (u16::from(entry.flags.assume_valid()) << 15)
            | (u16::from(entry.flags.extended()) << 14)
            | (u16::from(entry.flags.stage()) << 12)
            | entry.flags.name_length();

        Self {
            path: entry.path_name.to_string(),
            mode,
            hash: entry.hash.to_string(),
            ctime_seconds: entry.ctime_seconds,
            ctime_ns_fractions: entry.ctime_ns_fractions,
            mtime_seconds: entry.mtime_seconds,
            mtime_ns_fractions: entry.mtime_ns_fractions,
            dev: entry.dev,
            ino: entry.ino,
            uid: entry.uid,
            gid: entry.gid,
            file_size: entry.file_size,
            flags,
        }
    }
}

impl From<git2::IndexEntry> for Entry {
    fn from(entry: git2::IndexEntry) -> Self {
        Self {
            path: String::from_utf8_lossy(&entry.path).into_owned(),
            mode: entry.mode,
            hash: entry.id.to_string(),
            ctime_seconds: entry.ctime.seconds() as u32,
            ctime_ns_fractions: entry.ctime.nanoseconds(),
            mtime_seconds: entry.mtime.seconds() as u32,
            mtime_ns_fractions: entry.mtime.nanoseconds(),
            dev: entry.dev,
            ino: entry.ino,
            uid: entry.uid,
            gid: entry.gid,
            file_size: entry.file_size,
            flags: entry.flags,
        }
    }
}

/// Ask libgit2 what the index contains.
fn expected_index_entries(dir: &Path) -> Vec<Entry> {
    let repo = Repository::open(dir).expect("should be able to open git repo");
    let index = repo.index().expect("should be able to get repo index");
    index.iter().map(Entry::from).collect()
}

fn run_op(path: &Path, op: &Op) {
    match op {
        Op::Create(name, contents) => {
            fs::write(path.join(&name.0), contents).expect("should be able to write file");
        }
        Op::Modify(name, delta) => {
            let file_path = path.join(&name.0);
            let mut contents = fs::read(&file_path).expect("should be able to read file to modify");
            delta.apply(&mut contents);
            fs::write(&file_path, contents).expect("should be able to write modified file");
        }
        Op::GitAdd(names) => {
            let repo = Repository::open(path).expect("should be able to open git repo");
            let mut index = repo.index().expect("should be able to get repo index");
            for name in names {
                index
                    .add_path(Path::new(&name.0))
                    .expect("libgit2 should be able to add file");
            }
            index.write().expect("libgit2 should be able to write index");
        }
        Op::GitRm(names) => {
            let repo = Repository::open(path).expect("should be able to open git repo");
            let mut index = repo.index().expect("should be able to get repo index");
            for name in names {
                let file_path = path.join(&name.0);
                if file_path.exists() {
                    let _ = fs::remove_file(&file_path);
                }
                index
                    .remove_path(Path::new(&name.0))
                    .expect("libgit2 should be able to remove file");
            }
            index.write().expect("libgit2 should be able to write index");
        }
    }
}

/// Creates a repository with libgit2, and applies the generated operations.
fn run_ops(path: &Path, ops: &Sequence) {
    Repository::init(path).expect("should be able to init git repo");

    for op in ops.clone().into_inner() {
        run_op(path, &op);
    }
}

fn run_op_tinygit(path: &Path, op: &Op) {
    match op {
        Op::Create(name, contents) => {
            fs::write(path.join(&name.0), contents).expect("should be able to write file");
        }
        Op::Modify(name, delta) => {
            let file_path = path.join(&name.0);
            let mut contents = fs::read(&file_path).expect("should be able to read file to modify");
            delta.apply(&mut contents);
            fs::write(&file_path, contents).expect("should be able to write modified file");
        }
        Op::GitAdd(names) => {
            let out = run_tinygit(
                path,
                &iter::once("add")
                    .chain(names.iter().map(|x| x.0.as_str()))
                    .collect::<Vec<&str>>(),
            );
            assert!(
                out.status.success(),
                "tinygit add shouldn't fail: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Op::GitRm(names) => {
            let repo = Repository::open(path).expect("should be able to open git repo");
            let mut index = repo.index().expect("should be able to get repo index");
            for name in names {
                let file_path = path.join(&name.0);
                if file_path.exists() {
                    let _ = fs::remove_file(&file_path);
                }
                index
                    .remove_path(Path::new(&name.0))
                    .expect("libgit2 should be able to remove file");
            }
            index.write().expect("libgit2 should be able to write index");
        }
    }
}



#[test]
fn fuzz_index_parsing() {
    check!().with_type::<Sequence>().for_each(|ops| {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        run_ops(path, ops);

        let mut expected = expected_index_entries(path);
        expected.sort_by(|left, right| left.path.cmp(&right.path));

        let index_path = path.join(".git/index");

        if !index_path.exists() {
            assert!(expected.is_empty());
            return;
        }

        // open index and assert it was properly parsed
        let index = Index::open(index_path).expect("should be able to parse index");

        // assert entries correctness
        let mut parsed: Vec<Entry> = index.entries().map(Entry::from).collect();
        parsed.sort_by(|left, right| left.path.cmp(&right.path));

        assert_eq!(parsed, expected);
    });
}

/// This test parses Git's index with tinygit, writes it back, and asserts that the result is
/// byte-for-byte identical to Git's index.
#[test]
fn fuzz_index_roundtrip() {
    check!().with_type::<Sequence>().for_each(|ops| {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        run_ops(path, ops);

        let index_path = path.join(".git/index");

        if !index_path.exists() {
            return;
        }

        // read Git's index before parsing, because `Index::write` truncates the file in place.
        let original = fs::read(&index_path).expect("should be able to read Git's index");

        let index = Index::open(index_path.clone()).expect("Git's index should parse");

        index
            .try_write()
            .expect("tinygit should re-encode the index");

        let rewritten = fs::read(&index_path).expect("should be able to read tinygit's index");

        assert_eq!(
            original, rewritten,
            "tinygit's index should be byte-identical to Git's"
        );
    });
}

/// Tests random sequences of git operations by running them once in a directory using git, and
/// running them in another directory using tinygit. Asserts the contents of the index are the same
/// (minus the things that can't be the same, like the timestamps and inodes)
#[test]
fn fuzz_index_differential_add() {
    check!().with_type::<Sequence>().for_each(|ops| {
        let git_dir = TempDir::new().unwrap();
        let tinygit_dir = TempDir::new().unwrap();

        Repository::init(git_dir.path()).expect("libgit2 should be able to init git repo");
        Repository::init(tinygit_dir.path()).expect("libgit2 should be able to init tinygit repo");

        let indexed = |entries: Vec<Entry>| -> Vec<Entry> {
            let mut entries = entries;
            entries.sort_by(|left, right| left.path.cmp(&right.path));
            entries
        };

        let content_view = |entries: Vec<Entry>| -> Vec<(String, u32, String, u16)> {
            entries
                .into_iter()
                .map(|entry| (entry.path, entry.mode, entry.hash, entry.flags))
                .collect()
        };

        for op in ops.clone().into_inner() {
            run_op(git_dir.path(), &op);
            run_op_tinygit(tinygit_dir.path(), &op);

            let tinygit_index_path = tinygit_dir.path().join(".git/index");

            if tinygit_index_path.exists() {
                let tinygit_view = indexed(
                    Index::open(tinygit_index_path)
                        .expect("tinygit's index should parse")
                        .entries()
                        .map(Entry::from)
                        .collect(),
                );
                let ls_files_view = indexed(expected_index_entries(tinygit_dir.path()));
                assert_eq!(
                    tinygit_view, ls_files_view,
                    "tinygit internal index parse must match git ls-files on tinygit dir after op: {:?}",
                    op
                );

                let git_view = indexed(expected_index_entries(git_dir.path()));
                assert_eq!(
                    content_view(git_view),
                    content_view(tinygit_view),
                    "git index and tinygit index content must match after op: {:?}",
                    op
                );
            } else {
                let git_view = indexed(expected_index_entries(git_dir.path()));
                assert!(
                    git_view.is_empty(),
                    "tinygit index does not exist, but git index is not empty after op: {:?}",
                    op
                );
            }
        }
    });
}
