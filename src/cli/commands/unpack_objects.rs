use std::{env, path::PathBuf, sync::Arc};

use argh::FromArgs;
use fallible_iterator::FallibleIterator;
use indicatif::ProgressBar;
use tinygit::{Repo, TinygitError, storage::object::packed::Packfile};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "unpack-objects")]
/// Unpact objects from a packed archive
pub struct UnpackObjectsCommand {
    #[argh(positional)]
    file: String, // TODO: should it support multiple ones?
}

impl UnpackObjectsCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let Self { file } = self;

        let repo = Repo::open_closest(env::current_dir()?)?;

        let pack_path = PathBuf::from(file);
        let idx_path = pack_path.with_extension("idx");

        let pack = Arc::new(Packfile::open(pack_path, idx_path)?);

        let bar = ProgressBar::new(pack.objects_count as u64);

        for packed_obj in pack.objects().iterator() {
            let packed_obj = packed_obj?;
            let raw_obj = pack.resolve(packed_obj)?;

            if let Err(err) = repo.store.encode_object(&raw_obj) {
                match err {
                    TinygitError::ObjectExists => {
                        // we ignore objects that already exist
                    }

                    other => return Err(other),
                }
            }
            bar.inc(1);
        }

        bar.finish();

        Ok(())
    }
}
