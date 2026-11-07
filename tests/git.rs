//! This is a differential fuzzing suite. It uses `bolero` to generate Git commands, runs them with
//! the real Git implementation, and we check against tinygit.

use bolero::{TypeGenerator, ValueGenerator, check, one_value_of, produce};
use nutype::nutype;
use std::{
    collections::HashMap,
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
        let mut names: Vec<String> = Vec::default();

        let n = (0..32usize).generate(driver)?;

        for _ in 0..n {
            let kind = {
                if names.is_empty() {
                    OpDiscriminants::Create
                } else {
                    OpDiscriminants::generate(driver)?
                }
            };

            let op = match kind {
                OpDiscriminants::Create => {
                    let name = Name::generate(driver)?;

                    names.push(name.0.clone());

                    Op::Create(name, <Vec<u8> as TypeGenerator>::generate(driver)?)
                }
                OpDiscriminants::GitAdd => {
                    Op::GitAdd(vec![Name(one_value_of(&names[..]).generate(driver)?)])
                }
                OpDiscriminants::GitRm => {
                    Op::GitRm(vec![Name(one_value_of(&names[..]).generate(driver)?)])
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

#[test]
fn fuzz_index_parsing() {
    check!().with_type::<Sequence>().for_each(|ops| {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let mut working: HashMap<String, Vec<u8>> = HashMap::default();
        let mut expected: HashMap<String, Vec<u8>> = HashMap::default();

        run_git(path, &["init"]);

        for op in ops.clone().into_inner() {
            match op {
                Op::Create(name, contents) => {
                    fs::write(path.join(&name.0), contents.clone())
                        .expect("should be able to write file");

                    working.insert(name.0, contents);
                }
                Op::GitAdd(names) => {
                    let out = run_git(
                        path,
                        &iter::once("add")
                            .chain(names.iter().map(|x| x.0.as_str()))
                            .collect::<Vec<&str>>(),
                    );
                    assert!(out.status.success(), "git add shouldn't fail");

                    for name in &names {
                        let bytes = working.get(&name.0).expect("name should exist");

                        expected.insert(name.0.clone(), bytes.clone());
                    }
                }
                Op::GitRm(names) => {
                    let out = run_git(
                        path,
                        &iter::once("rm")
                            .chain(names.iter().map(|x| x.0.as_str()))
                            .collect::<Vec<&str>>(),
                    );

                    assert!(out.status.success(), "git rm shouldn't fail");

                    for name in &names {
                        working
                            .remove(&name.0)
                            .expect("name should exist in the working tree");
                    }
                }
            };
        }

        let index_path = path.join(".git/index");

        if !index_path.exists() {
            assert!(expected.is_empty());
            return;
        }

        // open index and assert it was properly parsed
        let index = Index::open(index_path).expect("should be able to parse index");

        assert_eq!(
            index.entries.len(),
            expected.len(),
            "index should have correct amount of entries"
        );

        // assert entries correctness
        let mut parsed: Vec<(String, u32)> = index
            .entries
            .iter()
            .map(|entry| (entry.path_name.to_string(), entry.file_size))
            .collect();

        parsed.sort();

        let mut expected: Vec<(String, u32)> = expected
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.len() as u32))
            .collect();

        expected.sort();

        assert_eq!(parsed, expected);
    });
}
