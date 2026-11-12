use std::{env, path::PathBuf};

use argh::FromArgs;
use tinygit::{Repo, TinygitError};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "add")]
/// Add file contents to the index
pub struct AddCommand {
    /// paths of the files to stage.
    #[argh(positional)]
    files: Vec<PathBuf>,
}

impl AddCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let repo = Repo::open_closest(env::current_dir()?)?;

        if !self.files.iter().all(|path| path.exists()) {
            return Err(TinygitError::InvalidFileName);
        }

        for path in self.files {
            repo.stage(&path)?;
        }

        Ok(())
    }
}
