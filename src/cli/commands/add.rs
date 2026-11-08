use std::{env, fs, path::PathBuf};

use argh::FromArgs;
use tinygit::{
    Repo, TinygitError,
    object::{Object, blob::Blob},
    storage::object::RawObject,
};

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
        let repo = Repo::open(env::current_dir()?)?;

        if !self.files.iter().all(|path| path.exists()) {
            return Err(TinygitError::InvalidFileName);
        }

        for path in self.files {
            let bytes = fs::read(path)?;
            let object = Object::Blob(Blob(bytes));
            let raw_object: RawObject = object.try_into()?;

            match repo.store.encode_object(&raw_object) {
                Err(TinygitError::ObjectExists) => {}
                other => other?,
            };
        }

        Ok(())
    }
}
