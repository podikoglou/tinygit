use std::{
    env, fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    time::SystemTime,
};

use argh::FromArgs;
use bstr::{BStr, BString};
use tinygit::{
    Repo, TinygitError,
    object::{Object, blob::Blob},
    storage::{
        index::{Flags, Index, IndexEntry, Mode, ObjectType},
        object::RawObject,
    },
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
            // TODO: this has to be relative to the repo's root
            let path_name = &path
                .file_name()
                .map(|s| BString::from(s.to_string_lossy().as_bytes()))
                .ok_or(TinygitError::InvalidFileName)?;

            let bytes = fs::read(&path)?;
            let object = Object::Blob(Blob(bytes));
            let raw_object: RawObject = object.try_into()?;

            // create blob
            match repo.store.encode_object(&raw_object) {
                Err(TinygitError::ObjectExists) => {}
                other => other?,
            };

            // get the metadata for the entry (before we even open the index, so as to keep the
            // critical section as short as possible)
            let metadata = fs::symlink_metadata(&path)?;
            let ctime = metadata.created()?.duration_since(SystemTime::UNIX_EPOCH)?;
            let mtime = metadata
                .modified()?
                .duration_since(SystemTime::UNIX_EPOCH)?;

            // TODO: support `ObjectType::GitLink`
            let mode = if metadata.is_symlink() {
                Mode::try_new(ObjectType::SymbolicLink, 0)
            } else {
                let is_executable = (metadata.permissions().mode() & 0o100) != 0;

                Mode::try_new(
                    ObjectType::Regular,
                    if is_executable { 0o755 } else { 0o644 },
                )
            }?;

            let name_length = if path_name.len() > 0xFFF {
                0xFFF
            } else {
                path_name.len() as u16
            };

            // Git treats a missing index file as an empty index.
            let index_path = repo.store.index_path();
            let mut index = if index_path.exists() {
                repo.store.index()?
            } else {
                Index::new(index_path)
            };

            // if it already exists in the index, remove it
            index.entries.retain(|entry| &entry.path_name != path_name);

            // add the new entry
            index.entries.push(IndexEntry {
                ctime_seconds: ctime.as_secs().try_into()?,
                ctime_ns_fractions: ctime.subsec_nanos(),
                mtime_seconds: mtime.as_secs().try_into()?,
                mtime_ns_fractions: mtime.subsec_nanos(),
                dev: metadata.dev().try_into()?,
                ino: metadata.ino().try_into()?,
                mode,
                uid: metadata.uid(),
                gid: metadata.gid(),
                file_size: metadata.size().try_into()?,
                hash: raw_object.hash()?,
                flags: Flags::try_new(false, false, 0, name_length)?,
                path_name: path_name.to_owned(),
            });

            index.try_write()?;
        }

        Ok(())
    }
}
