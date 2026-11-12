use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::SystemTime,
};

use bstr::BString;

use crate::{
    TinygitError,
    object::{Object, blob::Blob},
    storage::{
        Store,
        index::{Flags, IndexEntry, Mode, ObjectType},
        object::RawObject,
        reference::RefPointer,
    },
};

/// A Git repository, containing a `.git` directory.
#[derive(Debug)]
pub struct Repo {
    pub path: PathBuf,
    pub store: Store,
}

impl Repo {
    /// Opens an already existing Git repository, constructing a [Repo].
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, TinygitError> {
        let path = path.into();

        let store = Store::open(path.join(".git"))?;

        Ok(Self { path, store })
    }

    /// Creates a new empty Git repository.
    pub fn new(path: impl Into<PathBuf>) -> Result<Repo, TinygitError> {
        let path = path.into();

        let git_dir = path.join(".git");

        // remove directory if it already exists (to re-initialize it)
        if git_dir.exists() {
            fs::remove_dir_all(git_dir)?;
        }

        fs::create_dir_all(path.join(".git/objects/pack"))?;
        fs::create_dir_all(path.join(".git/refs/heads"))?;

        let store = Store::open(path.join(".git"))?;

        // a fresh repository starts on an unborn branch
        store.set_reference(
            &"HEAD".into(),
            RefPointer::Symbolic("refs/heads/main".into()),
        )?;

        Ok(Self { store, path })
    }

    /// Stages a file into the working area.
    pub fn stage(&self, file_path: impl AsRef<Path>) -> Result<(), TinygitError> {
        let path = file_path.as_ref();

        if !path.exists() {
            return Err(TinygitError::InvalidFileName);
        }

        // check if path is within the repo
        if !path.canonicalize()?.starts_with(&self.path.canonicalize()?) {
            return Err(TinygitError::NotInRepo);
        }

        // TODO: this has to be relative to the repo's root
        let path_name = &path
            .file_name()
            .map(|s| BString::from(s.to_string_lossy().as_bytes()))
            .ok_or(TinygitError::InvalidFileName)?;

        let bytes = fs::read(path)?;
        let object = Object::Blob(Blob(bytes));
        let raw_object: RawObject = object.try_into()?;

        // create blob
        match self.store.encode_object(&raw_object) {
            Err(TinygitError::ObjectExists) => {}
            other => other?,
        };

        // get the metadata for the entry (before we even open the index, so as to keep the
        // critical section as short as possible)
        let metadata = fs::symlink_metadata(path)?;
        let ctime = metadata.created()?.duration_since(SystemTime::UNIX_EPOCH)?;
        let mtime = metadata
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)?;

        // TODO: support `ObjectType::GitLink`
        let mode = if metadata.is_symlink() {
            Mode::try_new(ObjectType::SymbolicLink, 0)
        } else {
            let is_executable = (&metadata.permissions().mode() & 0o100) != 0;

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
        let mut index = self.store.index()?;

        // add the new entry
        index.add_entry(IndexEntry {
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

        Ok(())
    }
}
