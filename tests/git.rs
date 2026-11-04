//! This is a differential fuzzing suite. It uses `bolero` to generate Git commands, runs them with
//! the real Git implementation, and we check against tinygit.

use bolero::{TypeGenerator, check, produce};
use std::{
    fs, iter,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;
use tinygit::storage::index::Index;

#[derive(Debug, Clone, TypeGenerator)]
struct Name(#[generator(produce::<String>().with().len(1..=255).chars('A'..='z'))] String);

#[derive(Debug, Clone, TypeGenerator)]
enum Op {
    Create(Name, Vec<u8>),
    GitAdd(Vec<Name>),
    GitRm(Vec<Name>),
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
    check!().with_type::<Vec<Op>>().for_each(|ops| {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        run_git(path, &["init"]);

        for op in ops {
            match op {
                Op::Create(name, contents) => {
                    fs::write(&name.0, contents).expect("should be able to write file");
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

        Index::open(path.join(".git/index")).expect("should be able to parse index");
    });
}
