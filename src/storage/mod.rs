use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::PathBuf,
    sync::Arc,
};

use once_cell::sync::OnceCell;

use bstr::BString;

use crate::{
    TinygitError,
    ende::Encode,
    error::ParserContext,
    fs::FileName,
    identity::Identity,
    object::{
        Object,
        commit::Commit,
        hash::{HashPrefix, ObjectHash},
        tree::{Tree, TreeEntry, TreeKey},
    },
    storage::{
        index::Index,
        object::{LazyObject, LooseDb, ObjectDb, ObjectsBucket, RawObject, packed::Packfile},
        reference::{Ref, RefName, RefPointer, db::RefsDb},
    },
    time::Timestamp,
};

pub mod index;
pub mod object;
pub mod reference;

#[derive(Debug, Default)]
pub struct Store {
    pub path: PathBuf,
    pub loose: LooseDb,

    packs: Vec<Arc<Packfile>>,
    refs_db: OnceCell<Arc<RefsDb>>,
}

impl Store {
    /// Opens an already existing git database.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, TinygitError> {
        let path = path.into();

        match fs::exists(&path) {
            Ok(true) => {
                let loose = LooseDb::open(path.join("objects/"))?;

                let mut store = Self {
                    path,
                    loose,
                    ..Default::default()
                };

                store.refresh_packs()?;

                Ok(store)
            }
            Ok(false) => Err(TinygitError::NoGitDirectory),
            Err(err) => Err(err.into()),
        }
    }

    #[must_use]
    pub fn objects_path(&self) -> PathBuf {
        self.path.join("objects/")
    }

    #[must_use]
    pub fn packs_path(&self) -> PathBuf {
        self.objects_path().join("pack/")
    }

    #[must_use]
    pub fn refs_path(&self) -> PathBuf {
        self.path.join("refs/")
    }

    #[must_use]
    pub fn ref_path(&self, name: &String) -> PathBuf {
        self.path.join(name)
    }

    #[must_use]
    pub fn packed_refs_path(&self) -> PathBuf {
        self.path.join("packed-refs")
    }

    #[must_use]
    pub fn index_path(&self) -> PathBuf {
        self.path.join("index")
    }

    /// Returns all buckets (directories named after the prefix of a hash) under `.git/objects/`.
    ///
    /// At most 256 of them exist, so they are collected eagerly.
    pub fn buckets(&self) -> Result<Vec<ObjectsBucket>, TinygitError> {
        self.loose.buckets()
    }

    /// Finds an [`ObjectsBucket`] in .git/objects/
    pub fn bucket(&self, prefix: HashPrefix) -> Result<ObjectsBucket, TinygitError> {
        self.loose.bucket(prefix)
    }

    /// Refreshes the internal packfile listing
    pub fn refresh_packs(&mut self) -> Result<(), TinygitError> {
        let pack_dir = self.packs_path();

        self.packs = if pack_dir.exists() {
            fs::read_dir(pack_dir)?
                .filter_map(|entry| {
                    // it should be noted that this closure doesn't propagate any errors up; when an
                    // error is encountered, it is simply ignored
                    let path = entry.ok()?.path();
                    let file_name = path.file_name()?.to_string_lossy();

                    // ensure in the form of "pack-<hash>.pack"
                    if !file_name.starts_with("pack-") {
                        return None;
                    }

                    if !file_name.ends_with(".pack") {
                        return None;
                    }

                    let idx_path = path.with_extension("idx");

                    // ensure respective .idx existts
                    if !idx_path.exists() {
                        return None;
                    }

                    // TODO: when do we ever handle such failures? this seems important
                    let pack = Packfile::open(path, idx_path).ok()?;

                    Some(Arc::new(pack))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::default()
        };

        Ok(())
    }

    /// Gets all the [Packfile]s in the store.
    pub fn packs(&self) -> impl Iterator<Item = Arc<Packfile>> {
        self.packs.iter().cloned()
    }

    pub fn objects(
        &self,
    ) -> Result<impl Iterator<Item = Result<LazyObject, TinygitError>>, TinygitError> {
        let packs = self.packs();

        let objects = packs.into_iter().flat_map(Packfile::lazy_objects).map(Ok);

        Ok(self.loose.objects()?.chain(objects))
    }

    /// Gets a [RawObject] by its [ObjectHash].
    pub fn raw_object(&self, hash: &ObjectHash) -> Result<RawObject, TinygitError> {
        // search in packfiles
        for pack in self.packs() {
            if let Ok(raw) = pack.raw_object(hash) {
                return Ok(raw);
            }
        }

        // search in loose objects
        self.loose.raw_object(hash)
    }

    /// Gets a [LazyObject] by its [ObjectHash].
    pub fn object(&self, hash: &ObjectHash) -> Result<Object, TinygitError> {
        // search in packfiles
        for pack in self.packs() {
            if let Ok(raw) = pack.raw_object(hash) {
                return raw.into_object(ParserContext::File(pack.path().to_owned()));
            }
        }

        // search in loose objects
        self.loose.object(hash)
    }

    /// Writes a [RawObject] to the loose object database.
    pub fn encode_object(&self, object: &RawObject) -> Result<(), TinygitError> {
        self.loose.encode_object(object)
    }

    /// Writes an [Object] and returns its hash.
    ///
    /// An object that already exists is not an error.
    pub fn write_object(&self, object: &Object) -> Result<ObjectHash, TinygitError> {
        let raw_object = RawObject::try_from(object.clone())?;
        let hash = raw_object.clone().hash()?;

        match self.encode_object(&raw_object) {
            Err(TinygitError::ObjectExists) => {}
            other => other?,
        };

        Ok(hash)
    }

    /// Writes the index as a tree object and returns its hash.
    ///
    /// Equivalent to `git write-tree`.
    pub fn write_tree(&self) -> Result<ObjectHash, TinygitError> {
        let mut entries = BTreeMap::new();

        for entry in self.index()?.entries() {
            let key = TreeKey::new(
                FileName::try_new(entry.path_name.to_string())?,
                entry.mode.raw() as u16,
            );

            entries.insert(key, TreeEntry::new(entry.hash.clone()));
        }

        self.write_object(&Object::Tree(Tree::new(entries)))
    }

    /// Commits the current index and moves `HEAD`.
    pub fn commit(
        &self,
        message: impl Into<BString>,
        author: (Identity, Timestamp),
        committer: (Identity, Timestamp),
    ) -> Result<ObjectHash, TinygitError> {
        if self.index()?.entries().next().is_none() {
            return Err(TinygitError::EmptyTree);
        }

        let tree = self.write_tree()?;

        let parents = match self.head() {
            Ok((_, hash)) => vec![hash],
            Err(TinygitError::RefNotFound) => vec![],
            Err(err) => return Err(err),
        };

        // `git commit -m` stores the message followed by a newline
        let mut description: BString = message.into();
        if !description.ends_with(b"\n") {
            description.push(b'\n');
        }

        let commit = Commit {
            tree,
            parents,
            author,
            committer,
            extra: vec![],
            description,
        };

        let hash = self.write_object(&Object::Commit(Box::new(commit)))?;

        self.set_head(hash.clone())?;

        Ok(hash)
    }

    /// Gets a handle to the `packed-refs` database
    pub fn ref_db(&self) -> Result<Arc<RefsDb>, TinygitError> {
        self.refs_db
            .get_or_try_init(|| RefsDb::open(self.packed_refs_path()).map(Arc::new))
            .cloned()
    }

    /// Finds a [Ref] in the repository.
    ///
    /// Priority is given to loose refs, like the `git`.
    ///
    /// Note that this function does not guarantee that the returned ref is peeled.
    pub fn reference(&self, name: &RefName) -> Result<Ref, TinygitError> {
        // try to read as loose
        //
        // NOTE: this will be unpeeled!
        let path = self.ref_path(name);
        if path.exists() {
            return Ref::open_loose(path, name);
        }

        // try to read packed
        // TODO: Is this good error handling? I don't like how we completely ignore the underlying
        // error, but the truth is that, yes, indeed, the ref was not found, no matter why. Perhaps
        // RefNotFound could contain Option<TinygitError> ?

        let ref_db = self.ref_db().map_err(|_| TinygitError::RefNotFound)?;

        ref_db.lookup(name).map_err(|_| TinygitError::RefNotFound)
    }

    /// Sets the pointer of a [Ref] in the repository.
    pub fn set_reference(&self, name: &RefName, pointer: RefPointer) -> Result<(), TinygitError> {
        let path = self.ref_path(name);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = File::create(path)?;

        pointer.encode(&mut file)?;

        Ok(())
    }

    /// Moves `HEAD` to `hash`.
    ///
    /// If HEAD is symbolic, this moves the target branch.
    /// If HEAD is detached, this moves HEAD itself.
    pub fn set_head(&self, hash: ObjectHash) -> Result<(), TinygitError> {
        match self.reference(&"HEAD".into())?.pointer {
            RefPointer::Symbolic(target) => self.set_reference(&target, RefPointer::Hash(hash)),
            RefPointer::Hash(_) => self.set_reference(&"HEAD".into(), RefPointer::Hash(hash)),
        }
    }

    /// Recursively resolve a [RefPointer] to a the [ObjectHash] of the object it points to.
    pub fn resolve_reference(&self, pointer: &RefPointer) -> Result<ObjectHash, TinygitError> {
        match pointer {
            RefPointer::Hash(hash) => Ok(hash.to_owned()),
            RefPointer::Symbolic(name) => {
                let reference = self.reference(name)?;

                self.resolve_reference(&reference.pointer)
            }
        }
    }

    /// Gets the [Commit] and [ObjectHash] (in a tuple) of the commit `HEAD` is pointing to.
    ///
    /// Returns [TinygitError::RefNotFound] if not found.
    pub fn head(&self) -> Result<(Commit, ObjectHash), TinygitError> {
        self.reference(&"HEAD".into())
            .and_then(|head| self.resolve_reference(&head.pointer))
            .and_then(|hash| self.object(&hash).map(|obj| (obj, hash)))
            .and_then(|(object, hash)| {
                object
                    .try_as_commit()
                    .ok_or(TinygitError::UnexpectedObjectType)
                    .map(|commit| (*commit, hash))
            })
    }

    /// Reads the [Index].
    ///
    /// For the sake of doing less bookkeeeping (like trying to keep the index up to date), we don't
    /// store the index in the Store. Instead, it is dynamically read every time this is called.
    ///
    /// Git treats a missing index file as an empty index.
    pub fn index(&self) -> Result<Index, TinygitError> {
        let path = self.index_path();

        if path.exists() {
            Index::open(path)
        } else {
            Ok(Index::new(path))
        }
    }
}

impl ObjectDb for Store {
    fn raw_object(&self, hash: &ObjectHash) -> Result<RawObject, TinygitError> {
        self.raw_object(hash)
    }
}
