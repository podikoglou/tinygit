use std::{fs::File, path::PathBuf};

use fallible_iterator::FallibleIterator;
use memmap2::Mmap;
use quick_cache::sync::Cache;
use winnow::{Parser, token::take};

use crate::{
    TinygitError,
    ende::Decode,
    error::ParserContext,
    object::hash::{ObjectHash, parse_object_hash},
    parsing::{ParseNextExt, Stream},
    storage::object::{
        ObjectDb, RawObject,
        packed::{BaseObject, PackedObject, PackfileHeader, idx::PackIndex, stream::PackStream},
    },
};

/// Amount of items that the item can hold.
const CACHE_CAPACITY: usize = 100;

#[derive(Debug)]
pub struct PackIndexBuilder {
    pack_path: PathBuf,
    index_path: PathBuf,
    mmap: Mmap,

    objects_expected: usize,
    checksum: ObjectHash,
}

impl PackIndexBuilder {
    /// Creates a new pack builder, memory-mapping the packfile and parsing its header.
    ///
    /// If `index_path` is not provided, it is inferred from the pack's path, where the `.pack`
    /// extension is replaced with `.idx`
    pub fn new(pack_path: PathBuf, index_path: Option<PathBuf>) -> Result<Self, TinygitError> {
        let index_path = index_path.unwrap_or_else(|| pack_path.with_extension("idx"));

        let file = File::open(&pack_path)?;
        let mmap = unsafe { Mmap::map(&file) }?;

        let mut stream = Stream::new(&mmap);
        let context = ParserContext::File(pack_path.clone());

        let header = PackfileHeader::parse
            .parse_next(&mut stream)
            .context_err(context.clone(), &stream)?;

        // seek to the end
        let _ = take(mmap.len() - 12 - 20)
            .parse_next(&mut stream)
            .context_err(context.clone(), &stream)?;

        let checksum = parse_object_hash
            .parse_next(&mut stream)
            .context_err(context, &stream)?;

        Ok(Self {
            pack_path,
            index_path,
            mmap,
            objects_expected: header.objects as usize,
            checksum,
        })
    }

    fn objects(&self) -> PackStream<'_> {
        PackStream::new_from_stream(
            Stream::new(&self.mmap[12..]),
            self.objects_expected,
            ParserContext::File(self.pack_path.clone()),
        )
    }

    fn read_object_at_offset(&self, offset: usize) -> Result<PackedObject, TinygitError> {
        let mut stream = Stream::new(&self.mmap[12..]);
        let context = ParserContext::File(self.pack_path.clone());

        take(offset)
            .parse_next(&mut stream)
            .context_err(context.clone(), &stream)?;

        PackedObject::parse
            .parse_next(&mut stream)
            .context_err(context, &stream)
    }

    /// Recursively resolves a [PackedObject] to a [RawObject], possibly using the cache (thus the
    /// need for self to be mutable)
    fn resolve(
        &self,
        object: PackedObject,
        cache: &mut ObjectCache,
    ) -> Result<RawObject, TinygitError> {
        let offset = object.offset();

        let raw_object = match object {
            PackedObject::Deltified(_offset, object) => {
                let (base, delta) = object.into_inner();

                let base_raw = match base {
                    BaseObject::Ref(id) => self.raw_object(&id),

                    BaseObject::Ofs(offset) => {
                        let offset = offset as usize;

                        Ok(match cache.get(&offset) {
                            // already cached
                            Some(val) => val.clone(),

                            // read at offset, cache for future use
                            None => self
                                .read_object_at_offset(offset)
                                .and_then(|packed| self.resolve(packed, cache))?,
                        })
                    }
                };

                base_raw.map(|base| base.apply_delta(delta))
            }

            PackedObject::Undeltified(_, object) => Ok(object.into_inner()),
        }?;

        cache.insert(offset, raw_object.clone());

        Ok(raw_object)
    }

    pub fn build(&self, cache: &mut ObjectCache) -> Result<PackIndex, TinygitError> {
        let mut idx = PackIndex::new(&self.index_path);

        let mut intermediate: Vec<IntermediateTableEntry> = Vec::default();

        // a slice of the pack data, relative to the start of the first object, which the offsets
        // also are naturally, due to the way the `objects` iterator works
        let pack_data = &self.mmap[12..];

        let mut iter = self.objects().peekable();

        while let Some(packed_object) = iter.next()? {
            let offset = packed_object.offset();

            // offset of the end of the object in the packfile, aka, the start of the next if it
            // exists
            let end = match iter.peek()? {
                Some(next) => next.offset(),
                None => pack_data.len() - 20,
            };

            // resolve object
            let raw_object = self.resolve(packed_object, cache)?;

            // get its hash and prefix
            // this encodes the objet to a temporary buffer, slow
            let hash = raw_object.hash()?;
            let prefix = Into::<u8>::into(hash.prefix()) as usize;

            // populate fanout table cumulatively
            for i in prefix..idx.fanout_table.len() {
                if let Some(entry) = idx.fanout_table.get_mut(i) {
                    *entry += 1;
                }
            }

            // compute CRC on the full packed object including its header
            let object_bytes = &pack_data[offset..end];
            let crc = crc32fast::hash(object_bytes);

            // figure out where to place offset
            let offset = if offset > (1 << 31) {
                let offsets_2_idx = idx.offsets_2.len();

                idx.offsets_2.push(offset as u64);

                Offset::Large(offsets_2_idx as u32)
            } else {
                Offset::Small(offset as u32 + 12)
            };

            intermediate.push(IntermediateTableEntry(hash, crc, offset));
        }

        intermediate.sort();

        for IntermediateTableEntry(hash, crc, offset) in intermediate {
            idx.object_names.push(hash);
            idx.crc_entries.push(crc);

            match offset {
                Offset::Small(offset) => idx.offsets_1.push(offset),
                Offset::Large(index) => idx.offsets_1.push(index | 0x10),
            }
        }

        idx.objects_count = idx.fanout_table[255] as usize;
        idx.pack_checksum = self.checksum.to_owned();

        Ok(idx)
    }
}

impl ObjectDb for PackIndexBuilder {
    fn raw_object(&self, _hash: &ObjectHash) -> Result<RawObject, TinygitError> {
        // we could have an in memory cache that maps hashes to raw objects like we do
        // with offsets, but i believe ref deltas are mostly used when the object is
        // outside the pack, which is a feature we do not yet support and would be hard
        // to implement
        todo!("raw_object for pack index builder")
    }
}

/// Caches objects by their offsets (TODO: also hash?)
#[derive(Debug)]
pub struct ObjectCache {
    cache: Cache<usize, RawObject>,
}

impl ObjectCache {
    pub fn get(&mut self, offset: &usize) -> Option<RawObject> {
        self.cache.get(offset).map(|x| x.to_owned())
    }

    fn insert(&mut self, offset: usize, object: RawObject) {
        self.cache.insert(offset, object);
    }
}

impl Default for ObjectCache {
    fn default() -> Self {
        Self {
            cache: Cache::new(CACHE_CAPACITY),
        }
    }
}

/// A value of the first offset table.
///
/// Either an offset to the Packfile ([Offset::Small]) or an index to the second table
/// ([Offset::Large]).
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
enum Offset {
    /// The offset of the object in the Packfile.
    Small(u32),

    /// The index to the large offsets table. It is assumed that it exists.
    Large(u32),
}

impl From<Offset> for u32 {
    fn from(val: Offset) -> Self {
        match val {
            Offset::Small(offset) => offset,
            Offset::Large(index) => index | 0x10,
        }
    }
}

#[derive(PartialOrd, Ord, PartialEq, Eq)]
struct IntermediateTableEntry(ObjectHash, u32, Offset);
