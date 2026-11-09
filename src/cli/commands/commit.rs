use std::env;

use argh::FromArgs;
use tinygit::{Repo, TinygitError};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "commit")]
/// Record changes to the repository
pub struct CommitCommand {
    /// description of the commit
    #[argh(option, short = 'm')]
    message: String,
}

impl CommitCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let repo = Repo::open(env::current_dir()?)?;

        Ok(())
    }
}
