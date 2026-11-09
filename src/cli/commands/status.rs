use std::env;

use argh::FromArgs;
use tinygit::{Repo, TinygitError, storage::reference::RefPointer};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "status")]
/// Show the working tree status
pub struct StatusCommand {}

impl StatusCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let repo = Repo::open(env::current_dir()?)?;

        match &repo.store.reference(&"HEAD".into())?.pointer {
            RefPointer::Symbolic(r#ref) => {
                let branch = repo.store.reference(r#ref)?;
                let name = branch.stripped_name();

                println!("On branch {name}")
            }

            // TODO: red color + first few bytes from hash
            RefPointer::Hash(hash) => println!("HEAD detached at {hash}"),
        };

        Ok(())
    }
}
