use std::assert_matches;
use std::fs::File;

use fallible_iterator::FallibleIterator;
use insta::assert_debug_snapshot;
use memmap2::Mmap;
use tinygit::{
    ende::Decode,
    object::{ObjectType, hash::ObjectHash},
    parsing::Stream,
    storage::object::packed::{
        PackedObject::{self},
        Packfile, PackfileHeader,
        idx::PackIndex,
    },
};
use winnow::Parser;

mod common;

#[test]
fn undeltified_packfile_parse() {
    let (_, dir) = include_repo!("fixtures/repos/repo-5.tar");

    let path = dir
        .path()
        .join(".git/objects/pack/pack-a0ba98959a3d422d8bbe8e86f2cf62eb8f2b2df7.pack");

    let file = File::open(path).expect("should be able to open packfile");

    let mmap = unsafe { Mmap::map(&file).expect("should be able to mmap file") };
    let mut slice = Stream::new(&mmap[..]);

    let header = PackfileHeader::parse
        .parse_next(&mut slice)
        .expect("should be able to parse packfile header");

    assert_eq!(header.objects, 3, "should have three objects");

    let objects: Vec<_> = (0..header.objects)
        .map(|_| {
            PackedObject::parse
                .parse_next(&mut slice)
                .expect("should be able to parse object")
        })
        .collect();

    insta::assert_debug_snapshot!(objects);
}

#[test]
fn deltified_packfile_parse() {
    let (_, dir) = include_repo!("fixtures/repos/repo-6.tar");

    let path = dir
        .path()
        .join(".git/objects/pack/pack-c7e64c7164f8f92168a1ce0953f0716848c2c32c.pack");

    let file = File::open(path).expect("should be able to open packfile");

    let mmap = unsafe { Mmap::map(&file).expect("should be able to mmap file") };
    let mut slice = Stream::new(&mmap[..]);

    let header = PackfileHeader::parse
        .parse_next(&mut slice)
        .expect("should be able to parse packfile header");

    assert_eq!(header.objects, 12, "should have 12 objects");

    let objects: Vec<_> = (0..header.objects)
        .map(|_| {
            PackedObject::parse
                .parse_next(&mut slice)
                .expect("should be able to parse object")
        })
        .collect();

    assert_eq!(slice.len(), 20, "packfile should end with 20-byte checksum");

    insta::assert_debug_snapshot!(objects);
}

#[test]
fn objects_iterator() {
    let (_, dir) = include_repo!("fixtures/repos/repo-6.tar");

    let path = dir
        .path()
        .join(".git/objects/pack/pack-c7e64c7164f8f92168a1ce0953f0716848c2c32c.pack");

    let packfile = Packfile::open(path.clone(), path.with_extension("idx"))
        .expect("should be able to open packfile");
    let objects = packfile.objects();

    assert_eq!(
        objects
            .count()
            .expect("should be able to iterate over all objects"),
        packfile.objects_count as usize,
        "read objects should match packfile header's object count"
    );
}

#[test]
fn indexed_objects_query() {
    let (_, dir) = include_repo!("fixtures/repos/repo-6.tar");

    let pack_path = dir
        .path()
        .join(".git/objects/pack/pack-c7e64c7164f8f92168a1ce0953f0716848c2c32c.pack");

    let packfile = Packfile::open(pack_path.clone(), pack_path.with_extension("idx"))
        .expect("should be able to open packfile");

    let obj = packfile
        .read_object_by_id(
            "babbe85f1e25b2b7d2c4321bc9e278bae96b718b"
                .parse()
                .expect("should be able to parse hash"),
        )
        .expect("should be able to read object by id");

    assert_debug_snapshot!(obj);
}

#[test]
fn idx_read() {
    let (_, dir) = include_repo!("fixtures/repos/repo-5.tar");

    let path = dir
        .path()
        .join(".git/objects/pack/pack-a0ba98959a3d422d8bbe8e86f2cf62eb8f2b2df7.idx");

    let idx = PackIndex::open(path).expect("should be able to open pack idx file");

    assert_eq!(
        idx.fanout_table.len(),
        256,
        "fanout table should have 256 entries",
    );

    for (idx, window) in idx.fanout_table.windows(2).enumerate() {
        assert!(
            window[0] <= window[1],
            "window {} should not break monotonicity",
            idx
        );
    }

    let objects_count = idx.objects_count();

    assert_eq!(
        objects_count, 3,
        "object count (last fanout entry) should be 3"
    );

    assert_eq!(
        idx.object_names.len(),
        objects_count,
        "object names count should match object count"
    );

    assert_eq!(
        idx.crc_entries.len(),
        objects_count,
        "crc entries count should match object count"
    );

    assert_eq!(
        idx.offsets_1.len(),
        objects_count,
        "offsets_1 count should match object count"
    );

    let offsets_2_entries = idx
        .offsets_1
        .iter()
        .filter(|entry| (*entry & 0x8000_0000) != 0)
        .count();

    assert_eq!(
        idx.offsets_2.len(),
        offsets_2_entries,
        "offsets_2 count should match count of high-bit offsets in offsets_1"
    );

    assert_eq!(
        idx.pack_checksum,
        "a0ba98959a3d422d8bbe8e86f2cf62eb8f2b2df7"
            .parse::<ObjectHash>()
            .expect("should parse pack checksum")
    );

    assert_eq!(
        idx.lookup(
            "babbe85f1e25b2b7d2c4321bc9e278bae96b718b"
                .parse()
                .expect("should parse hash"),
        ),
        Some(12)
    );
}

#[test]
fn resolve_deltified_object_copy_size_zero() {
    let (_, dir) = include_repo!("fixtures/repos/repo-7.tar");

    let pack_path = dir
        .path()
        .join(".git/objects/pack/pack-c810c1b92e401673d07ba82467562a599f53e298.pack");

    let packfile = Packfile::open(pack_path.clone(), pack_path.with_extension("idx"))
        .expect("should be able to open packfile");

    let hash: ObjectHash = "db8b4b08da60fba8e2b5edab88f9cbaa01972293"
        .parse()
        .expect("should parse hash");

    let obj = packfile
        .read_object_at_offset(7848)
        .expect("should be able to read deltified object at offset 7848");

    assert_matches!(obj, PackedObject::Deltified { .. });

    let raw = packfile
        .resolve(obj)
        .expect("should be able to resolve deltified object");

    assert_eq!(raw.r#type, ObjectType::Blob);
    assert_eq!(raw.bytes.len(), 78000);
    assert_eq!(raw.hash().expect("should be able to hash raw object"), hash);
}

#[test]
fn resolve_deltified_object_by_id_copy_size_zero() {
    let (_, dir) = include_repo!("fixtures/repos/repo-7.tar");

    let pack_path = dir
        .path()
        .join(".git/objects/pack/pack-c810c1b92e401673d07ba82467562a599f53e298.pack");

    let packfile = Packfile::open(pack_path.clone(), pack_path.with_extension("idx"))
        .expect("should be able to open packfile");

    let hash: ObjectHash = "db8b4b08da60fba8e2b5edab88f9cbaa01972293"
        .parse()
        .expect("should parse hash");

    let obj = packfile
        .read_object_by_id(hash.clone())
        .expect("should be able to read deltified object by id");

    assert_matches!(obj, PackedObject::Deltified { .. });

    let raw = packfile
        .resolve(obj)
        .expect("should be able to resolve deltified object");

    assert_eq!(raw.r#type, ObjectType::Blob);
    assert_eq!(raw.bytes.len(), 78000);
    assert_eq!(raw.hash().expect("should be able to hash raw object"), hash);
}
