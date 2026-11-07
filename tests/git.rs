//! This is a differential fuzzing suite. It uses `bolero` to generate Git commands, runs them with
//! the real Git implementation, and we check against tinygit.

use bolero::{TypeGenerator, ValueGenerator, check, one_value_of, produce};
use nutype::nutype;
use std::{
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

        run_git(path, &["init"]);

        for op in ops.clone().into_inner() {
            match op {
                Op::Create(name, contents) => {
                    fs::write(path.join(&name.0), contents).expect("should be able to write file");
                }
                Op::GitAdd(names) => {
                    run_git(
                        path,
                        &iter::once("add")
                            .chain(names.iter().map(|x| x.0.as_str()))
                            .collect::<Vec<&str>>(),
                    );
                }
                Op::GitRm(names) => {
                    run_git(
                        path,
                        &iter::once("rm")
                            .chain(names.iter().map(|x| x.0.as_str()))
                            .collect::<Vec<&str>>(),
                    );
                }
            };
        }

        let index_path = path.join(".git/index");

        if !index_path.exists() {
            return;
        }

        Index::open(index_path).expect("should be able to parse index");
    });
}
