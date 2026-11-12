use std::path::Path;

use include_dir::{Dir, include_dir};
use itertools::Itertools;
use tinygit::{
    ende::Decode,
    storage::object::{LazyObject, loose::CompressedLooseObject},
};

mod fixtures {
    use tinygit::{
        ende::Decode, error::ParserContext, storage::object::loose::CompressedLooseObject,
    };

    #[test]
    fn read_loose_blob() {
        let bytes = include_bytes!("fixtures/objects/blob-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose blob object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose blob object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_tree() {
        let bytes = include_bytes!("fixtures/objects/tree-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tree object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose tree object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_non_root_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-2.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_merge_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-3.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_signed_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-4.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_extra_properties() {
        let bytes = include_bytes!("fixtures/objects/commit-5.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_empty_email() {
        let bytes = include_bytes!("fixtures/objects/commit-6.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_signed_commit_with_extra_properties() {
        let bytes = include_bytes!("fixtures/objects/commit-7.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_non_utf8_author() {
        let bytes = include_bytes!("fixtures/objects/commit-8.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_empty_author_name() {
        let bytes = include_bytes!("fixtures/objects/commit-9.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_non_utf8_description() {
        let bytes = include_bytes!("fixtures/objects/commit-10.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_tag() {
        let bytes = include_bytes!("fixtures/objects/tag-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tag object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose tag object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_tag_with_carriage_return_in_name() {
        let bytes = include_bytes!("fixtures/objects/tag-2.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tag object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose tag object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_taggerless_tag() {
        let bytes = include_bytes!("fixtures/objects/tag-3.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tag object")
            .into_inner()
            .into_inner()
            .into_object(ParserContext::None)
            .expect("should be able to parse loose tag object");

        insta::assert_debug_snapshot!(object);
    }
}

static OBJECTS_DIR: Dir = include_dir!("tests/fixtures/objects");

#[test]
fn hash_fixtures() {
    for (hash_file, object_file) in OBJECTS_DIR
        .files()
        .sorted_by_key(|e| e.path().file_stem().unwrap())
        .tuples::<(_, _)>()
    {
        // ensure correct setup
        assert_eq!(hash_file.path().file_stem(), object_file.path().file_stem());
        assert_eq!(hash_file.path().extension().unwrap(), "hash");
        assert_eq!(object_file.path().extension().unwrap(), "obj");

        let real_hash = hash_file.contents_utf8().unwrap().trim();
        let obj_bytes = object_file.contents();

        // test `Object::hash()`
        let parsed = CompressedLooseObject::decode_bytes(obj_bytes).expect("couldn't parse object");

        let hash = parsed
            .into_inner()
            .into_inner()
            .hash()
            .expect("couldn't hash object");

        assert_eq!(
            hash.to_string(),
            real_hash,
            "{} Object hash doesn't match actual hash",
            object_file.path().to_string_lossy()
        );

        // test `RawObject::hash()`
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/objects")
            .join(object_file.path());

        let lazy_obj = LazyObject::Loose(path);
        let raw_obj = lazy_obj
            .into_raw()
            .expect("couldn't convert into raw object");

        let raw_hash = raw_obj.hash().expect("couldn't hash raw object");

        assert_eq!(
            raw_hash.to_string(),
            real_hash,
            "{} RawObject hash doesn't match actual hash",
            object_file.path().to_string_lossy()
        );
    }
}
