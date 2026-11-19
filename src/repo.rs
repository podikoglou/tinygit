use std::{
    fs, io,
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

    /// Opens the closest repository by traversing up parent directories starting at `path`.
    pub fn open_closest(path: impl AsRef<Path>) -> Result<Self, TinygitError> {
        let start = path.as_ref().canonicalize()?;

        for ancestor in start.ancestors() {
            if ancestor.join(".git").exists() {
                return Self::open(ancestor);
            }
        }

        Err(TinygitError::NoGitDirectory)
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

    /// Stages a file, symbolic link, or the contents of a directory in the index.
    ///
    /// Relative input paths are resolved from the current directory. Index paths are
    /// relative to the repository root. Symlinks are staged without following their
    /// targets, and empty directories add no entries.
    ///
    /// The index is updated once after all entries have been collected.
    pub fn stage(&self, path: impl AsRef<Path>) -> Result<(), TinygitError> {
        let path = path.as_ref().components().as_path();
        let metadata = fs::symlink_metadata(path).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => TinygitError::InvalidFileName,
            _ => err.into(),
        })?;
        let root = self.path.canonicalize()?;
        let path = if metadata.is_symlink() {
            let name = path.file_name().ok_or(TinygitError::InvalidFileName)?;
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            parent.canonicalize()?.join(name)
        } else {
            path.canonicalize()?
        };
        let relative_path = path
            .strip_prefix(&root)
            .map_err(|_| TinygitError::NotInRepo)?;

        let mut entries = Vec::new();
        self.collect_index_entries(&path, relative_path, metadata, &mut entries)?;
        if entries.is_empty() {
            return Ok(());
        }

        let mut index = self.store.index()?;
        for entry in entries {
            index.add_entry(entry);
        }
        index.try_write()
    }

    fn collect_index_entries(
        &self,
        path: &Path,
        relative_path: &Path,
        metadata: fs::Metadata,
        entries: &mut Vec<IndexEntry>,
    ) -> Result<(), TinygitError> {
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                self.collect_index_entries(
                    &entry.path(),
                    &relative_path.join(entry.file_name()),
                    entry.metadata()?,
                    entries,
                )?;
            }
            return Ok(());
        }

        let (bytes, mode) = if metadata.is_symlink() {
            (
                fs::read_link(path)?.as_os_str().as_encoded_bytes().to_vec(),
                Mode::try_new(ObjectType::SymbolicLink, 0)?,
            )
        } else if metadata.is_file() {
            let permissions = if metadata.permissions().mode() & 0o100 != 0 {
                0o755
            } else {
                0o644
            };
            (
                fs::read(path)?,
                Mode::try_new(ObjectType::Regular, permissions)?,
            )
        } else {
            return Err(
                io::Error::new(io::ErrorKind::Unsupported, "cannot stage this file type").into(),
            );
        };
        let object: RawObject = Object::Blob(Blob(bytes)).try_into()?;
        match self.store.encode_object(&object) {
            Err(TinygitError::ObjectExists) => {}
            other => other?,
        }

        let ctime = metadata.created()?.duration_since(SystemTime::UNIX_EPOCH)?;
        let mtime = metadata
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)?;
        let path_name = BString::from(relative_path.as_os_str().as_encoded_bytes());
        let name_length = path_name.len().min(0xFFF) as u16;

        entries.push(IndexEntry {
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
            hash: object.hash()?,
            flags: Flags::try_new(false, false, 0, name_length)?,
            path_name,
        });

        Ok(())
    }
}
