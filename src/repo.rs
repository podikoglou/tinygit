use std::{
    collections::HashSet,
    ffi::OsStr,
    fs, io,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
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
    /// The index is updated once after all entries have been collected. Entries that the
    /// staged subtree no longer contains are removed, so staging a directory also records
    /// deletions of tracked files.
    pub fn stage(&self, path: impl AsRef<Path>) -> Result<(), TinygitError> {
        // A trailing separator would make symlink_metadata follow a directory link.
        let path = path.as_ref().components().as_path();
        let metadata = fs::symlink_metadata(path).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => TinygitError::InvalidFileName,
            _ => err.into(),
        })?;

        let root = self.path.canonicalize()?;
        let path = canonical_stage_path(path, &metadata)?;
        let relative_path = path
            .strip_prefix(&root)
            .map_err(|_| TinygitError::NotInRepo)?;

        let staged_dir = metadata.is_dir();
        let entries = self.collect_index_entries(&path, relative_path, metadata)?;

        // collected entries are files and symlinks, never directories, so nothing can
        // remain tracked beneath one of them
        let collected: HashSet<&[u8]> = entries
            .iter()
            .map(|entry| entry.path_name.as_slice())
            .collect();

        let mut index = self.store.index()?;
        let existing = index.entries().count();

        index.retain_entries(|entry| {
            let entry_path = Path::new(OsStr::from_bytes(entry.path_name.as_slice()));

            entry_survives_staging(entry_path, relative_path, staged_dir, &root, &collected)
        });

        let removed = existing != index.entries().count();

        if entries.is_empty() && !removed {
            return Ok(());
        }

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
    ) -> Result<Vec<IndexEntry>, TinygitError> {
        if !metadata.is_dir() {
            return Ok(vec![self.stage_file(path, relative_path, &metadata)?]);
        }

        let mut entries = Vec::new();
        for child in fs::read_dir(path)? {
            let child = child?;
            let child_path = child.path();
            let child_relative_path = relative_path.join(child.file_name());

            entries.extend(self.collect_index_entries(
                &child_path,
                &child_relative_path,
                child.metadata()?,
            )?);
        }

        Ok(entries)
    }

    fn stage_file(
        &self,
        path: &Path,
        relative_path: &Path,
        metadata: &fs::Metadata,
    ) -> Result<IndexEntry, TinygitError> {
        let (bytes, mode) = if metadata.is_symlink() {
            let target = fs::read_link(path)?;
            let mode = Mode::try_new(ObjectType::SymbolicLink, 0)?;

            (target.into_os_string().into_encoded_bytes(), mode)
        } else if metadata.is_file() {
            let permissions = if metadata.permissions().mode() & 0o100 != 0 {
                0o755
            } else {
                0o644
            };
            let mode = Mode::try_new(ObjectType::Regular, permissions)?;

            (fs::read(path)?, mode)
        } else {
            return Err(
                io::Error::new(io::ErrorKind::Unsupported, "cannot stage this file type").into(),
            );
        };

        let raw: RawObject = Object::Blob(Blob(bytes)).try_into()?;

        match self.store.encode_object(&raw) {
            Err(TinygitError::ObjectExists) => {}
            other => other?,
        }

        let ctime = metadata.created()?.duration_since(SystemTime::UNIX_EPOCH)?;
        let mtime = metadata
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)?;

        let path_name = BString::from(relative_path.as_os_str().as_encoded_bytes());
        let name_length = path_name.len().min(0xFFF) as u16;

        Ok(IndexEntry {
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
            hash: raw.hash()?,
            flags: Flags::try_new(false, false, 0, name_length)?,
            path_name,
        })
    }
}

/// Whether an index entry still describes the working tree after staging the
/// repository-relative `relative_path`, which is a directory when `staged_dir` is set.
fn entry_survives_staging(
    entry_path: &Path,
    relative_path: &Path,
    staged_dir: bool,
    root: &Path,
    collected: &HashSet<&[u8]>,
) -> bool {
    // checking the disk would follow a symlink that replaced one of these directories and
    // find the old targets, so the collected paths decide on their own
    let beneath_collected = entry_path.ancestors().skip(1).any(|ancestor| {
        !ancestor.as_os_str().is_empty()
            && collected.contains(ancestor.as_os_str().as_bytes())
    });

    if beneath_collected {
        return false;
    }

    if entry_path == relative_path {
        // a file replaced by a directory is represented by the files inside it
        return !staged_dir;
    }

    if relative_path.starts_with(entry_path) {
        // an entry cannot be a file while the path it contains is staged
        return false;
    }

    if entry_path.starts_with(relative_path) {
        if !staged_dir {
            // a directory replaced by a file leaves no entries beneath it
            return false;
        }

        // files deleted from the staged directory are no longer tracked, but a
        // file that still exists keeps its entry even if it is now ignored
        return root
            .join(entry_path)
            .symlink_metadata()
            .is_ok_and(|metadata| !metadata.is_dir());
    }

    true
}

fn canonical_stage_path(path: &Path, metadata: &fs::Metadata) -> Result<PathBuf, TinygitError> {
    if !metadata.is_symlink() {
        return Ok(path.canonicalize()?);
    }

    // Resolve the parent but keep the link's own name.
    let name = path.file_name().ok_or(TinygitError::InvalidFileName)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    Ok(parent.canonicalize()?.join(name))
}
