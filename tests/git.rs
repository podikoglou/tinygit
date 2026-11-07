//! This is a differential fuzzing suite. It uses `bolero` to generate Git commands, runs them with
//! the real Git implementation, and we check against tinygit.

use bolero::{TypeGenerator, ValueGenerator, check, one_value_of, produce};
use nutype::nutype;
use std::{
    collections::HashSet,
    fs, iter,
    path::Path,
    process::{Command, Output},
};
use strum::EnumDiscriminants;
use tempfile::TempDir;
use tinygit::storage::index::Index;

#[derive(Debug, Clone, TypeGenerator)]
struct Name(#[generator(produce::<String>().with().len(1..=255).chars('A'..='z'))] String);

#[derive(Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(TypeGenerator))]
enum Op {
    Create(Name, Vec<u8>),
    GitAdd(Vec<Name>),
    GitRm(Vec<Name>),
}

#[nutype(derive(Debug, Clone))]
struct Sequence(Vec<Op>);

impl TypeGenerator for Sequence {
    fn generate<D: bolero::Driver>(driver: &mut D) -> Option<Self> {
        let mut seq: Vec<Op> = Vec::default();

        // working tree
        let mut working: HashSet<String> = HashSet::default();

        // tracked files
        let mut index: HashSet<String> = HashSet::default();

        let n = (0..32usize).generate(driver)?;

        for _ in 0..n {
            let mut kinds = Vec::default();

            kinds.push(OpDiscriminants::Create);

            // `git add` is valid for any file that exists in the working tree.
            if !working.is_empty() {
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

                    working.insert(name.0.clone());

                    Op::Create(name, contents)
                }
                OpDiscriminants::GitAdd => {
                    let names: Vec<String> = working.iter().cloned().collect();
                    let name = Name(one_value_of(&names[..]).generate(driver)?);

                    index.insert(name.0.clone());

                    Op::GitAdd(vec![name])
                }
                OpDiscriminants::GitRm => {
                    let names: Vec<String> = index.iter().cloned().collect();
                    let name = Name(one_value_of(&names[..]).generate(driver)?);

                    assert!(
                        working.remove(&name.0),
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

fn run_git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("should be able to run git")
}

/// Ask the real Git implementation what the index contains.
fn expected_index_entries(dir: &Path) -> Vec<(String, u32)> {
    let out = run_git(dir, &["ls-files", "--debug", "-z"]);

    assert!(out.status.success(), "git ls-files shouldn't fail");

    // `-z` terminates each path with NUL, but the debug fields stay newline-terminated.
    // Generated names cannot contain NUL or newline, so replacing NUL with newline gives
    // one path line followed by its debug lines.
    let output = String::from_utf8_lossy(&out.stdout).replace('\0', "\n");

    let mut entries = Vec::default();
    let mut path = None;

    for line in output.lines() {
        if let Some(size) = line.strip_prefix("  size: ") {
            let size = size
                .split_whitespace()
                .next()
                .expect("size should have a value")
                .parse::<u32>()
                .expect("size should be a number");

            entries.push((path.take().expect("size should follow a path"), size));
        } else if !line.starts_with("  ") {
            path = Some(line.to_owned());
        }
    }

    entries
}

#[test]
fn fuzz_index_parsing() {
    check!().with_type::<Sequence>().for_each(|ops| {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        run_git(path, &["init"]);

        for op in ops.clone().into_inner() {
            match op {
                Op::Create(name, contents) => {
                    fs::write(path.join(&name.0), contents).expect("should be able to write file");
                }
                Op::GitAdd(names) => {
                    let out = run_git(
                        path,
                        &iter::once("add")
                            .chain(names.iter().map(|x| x.0.as_str()))
                            .collect::<Vec<&str>>(),
                    );
                    assert!(out.status.success(), "git add shouldn't fail");
                }
                Op::GitRm(names) => {
                    let out = run_git(
                        path,
                        &["rm", "-f"]
                            .into_iter()
                            .chain(names.iter().map(|x| x.0.as_str()))
                            .collect::<Vec<&str>>(),
                    );

                    assert!(out.status.success(), "git rm shouldn't fail");
                }
            };
        }

        let mut expected = expected_index_entries(path);
        expected.sort();

        let index_path = path.join(".git/index");

        if !index_path.exists() {
            assert!(expected.is_empty());
            return;
        }

        // open index and assert it was properly parsed
        let index = Index::open(index_path).expect("should be able to parse index");

        // assert entries correctness
        let mut parsed: Vec<(String, u32)> = index
            .entries
            .iter()
            .map(|entry| (entry.path_name.to_string(), entry.file_size))
            .collect();

        parsed.sort();

        assert_eq!(parsed, expected);
    });
}
