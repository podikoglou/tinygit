use std::{
    fs::{self, File},
    io::BufWriter,
    path::PathBuf,
};

use itertools::Itertools;

use crate::{
    TinygitError,
    object::{
        Object,
        hash::{HashPrefix, ObjectHash},
    },
    storage::object::{LazyObject, ObjectDb, ObjectsBucket, RawObject},
};

/// A database managing loose Git objects stored under `.git/objects/`.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct LooseDb {
    pub path: PathBuf,
}

impl LooseDb {
    /// Opens an already existing loose object database.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, TinygitError> {
        let path = path.into();

        match fs::exists(&path) {
            Ok(true) => Ok(Self { path }),
            Ok(false) => Err(TinygitError::NoGitDirectory),
            Err(err) => Err(err.into()),
        }
    }

    /// Resolves the filesystem path for a loose object given its [`ObjectHash`].
    #[must_use]
    pub fn object_path(&self, hash: &ObjectHash) -> PathBuf {
        self.path
            .join(hash.prefix().to_string())
            .join(&hash.to_string()[2..])
    }

    /// Returns all buckets (directories named after the 2-char prefix of a hash) under `.git/objects/`.
    ///
    /// At most 256 of them exist, so they are collected eagerly.
    pub fn buckets(&self) -> Result<Vec<ObjectsBucket>, TinygitError> {
        fs::read_dir(&self.path)?
            .filter_map(|result| match result {
                Err(err) => Some(Err(err.into())),
                Ok(entry) if entry.file_name().len() == 2 => {
                    Some(ObjectsBucket::open(entry.path()))
                }
                Ok(_) => None,
            })
            .collect()
    }

    /// Finds an [`ObjectsBucket`] in `.git/objects/`.
    pub fn bucket(&self, prefix: HashPrefix) -> Result<ObjectsBucket, TinygitError> {
        self.buckets()?
            .into_iter()
            .find(|dir| dir.prefix == prefix)
            .ok_or(TinygitError::BucketNotFound)
    }

    /// Returns an iterator over all loose [`LazyObject`]s.
    pub fn objects(
        &self,
    ) -> Result<impl Iterator<Item = Result<LazyObject, TinygitError>>, TinygitError> {
        Ok(self
            .buckets()?
            .into_iter()
            .map(ObjectsBucket::objects)
            .flatten_ok()
            .map(Result::flatten))
    }

    /// Gets a [`RawObject`] by its [`ObjectHash`].
    pub fn raw_object(&self, hash: &ObjectHash) -> Result<RawObject, TinygitError> {
        let path = self.object_path(hash);
        LazyObject::loose(path).into_raw()
    }

    /// Gets a parsed [`Object`] by its [`ObjectHash`].
    pub fn object(&self, hash: &ObjectHash) -> Result<Object, TinygitError> {
        let path = self.object_path(hash);
        LazyObject::loose(path).into_object()
    }

    /// Writes a [`RawObject`] to the loose object database.
    pub fn encode_object(&self, object: &RawObject) -> Result<(), TinygitError> {
        let hash = object.clone().hash()?;
        let prefix = hash.prefix();

        let bucket_path = self.path.join(prefix.to_string());

        if !bucket_path.exists() {
            fs::create_dir_all(&bucket_path)?;
        }

        let object_path = bucket_path.join(&hash.to_string()[2..]);

        let object_file = File::create_new(object_path).map_err(|_| TinygitError::ObjectExists)?;
        let mut writer = BufWriter::new(object_file);

        object.encode_compressed(&mut writer)
    }
}

impl ObjectDb for LooseDb {
    fn raw_object(&self, hash: &ObjectHash) -> Result<RawObject, TinygitError> {
        self.raw_object(hash)
    }
}
