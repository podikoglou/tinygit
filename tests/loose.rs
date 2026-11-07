use std::path::Path;

use include_dir::{Dir, include_dir};
use itertools::Itertools;
use tinygit::{
    ende::Decode,
    storage::object::{LazyObject, loose::CompressedLooseObject},
};

mod fixtures {
    use tinygit::{ende::Decode, storage::object::loose::CompressedLooseObject};

    #[test]
    fn read_loose_blob() {
        let bytes = include_bytes!("fixtures/objects/blob-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose blob object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_tree() {
        let bytes = include_bytes!("fixtures/objects/tree-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tree object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_non_root_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-2.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_merge_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-3.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_signed_commit() {
        let bytes = include_bytes!("fixtures/objects/commit-4.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_extra_properties() {
        let bytes = include_bytes!("fixtures/objects/commit-5.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_empty_email() {
        let bytes = include_bytes!("fixtures/objects/commit-6.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_signed_commit_with_extra_properties() {
        let bytes = include_bytes!("fixtures/objects/commit-7.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_non_utf8_author() {
        let bytes = include_bytes!("fixtures/objects/commit-8.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_empty_author_name() {
        let bytes = include_bytes!("fixtures/objects/commit-9.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_commit_with_non_utf8_description() {
        let bytes = include_bytes!("fixtures/objects/commit-10.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose commit object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_tag() {
        let bytes = include_bytes!("fixtures/objects/tag-1.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tag object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_tag_with_carriage_return_in_name() {
        let bytes = include_bytes!("fixtures/objects/tag-2.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tag object");

        insta::assert_debug_snapshot!(object);
    }

    #[test]
    fn read_loose_taggerless_tag() {
        let bytes = include_bytes!("fixtures/objects/tag-3.obj");
        let object = CompressedLooseObject::decode_bytes(bytes)
            .expect("should be able to read loose tag object");

        insta::assert_debug_snapshot!(object);
    }
}

mod roundtrip {
    use hegel::Generator;
    use hegel::TestCase;

    use tinygit::ende::Decode;
    use tinygit::error::ParserContext;
    use tinygit::object::{Object, ObjectType, hash::ObjectHash, tag::Tag};

    use tinygit::ende::Encode;
    use tinygit::storage::object::loose::CompressedLooseObject;

    mod generators {
        use crate::roundtrip::generators;

        use hegel::Generator;
        use hegel::TestCase;
        use hegel::extras::chrono::datetimes;
        use hegel::generators as gs;
        use std::collections::BTreeMap;
        use strum::VariantArray;
        use tinygit::ende::Decode;
        use tinygit::fs::FileName;
        use tinygit::identity::Email;
        use tinygit::identity::Identity;
        use tinygit::identity::Name;
        use tinygit::object::ObjectType;
        use tinygit::object::blob::Blob;
        use tinygit::object::commit::Commit;
        use tinygit::object::commit::CommitProperty;
        use tinygit::object::hash::ObjectHash;
        use tinygit::object::tag::Tag;
        use tinygit::object::tree::Tree;
        use tinygit::object::tree::TreeEntry;
        use tinygit::object::tree::TreeKey;
        use tinygit::time::Timestamp;

        #[hegel::composite]
        pub fn hash(tc: &TestCase) -> ObjectHash {
            tc.draw(
                gs::arrays(gs::integers())
                    .map(|bytes: [u8; 20]| ObjectHash::from(bytes))
                    .print_as_debug(),
            )
        }

        #[hegel::composite]
        pub fn blob(tc: &TestCase) -> Blob {
            tc.draw(gs::binary().map(Blob).print_as_debug())
        }

        #[hegel::composite]
        pub fn tree_entry(tc: &TestCase) -> TreeEntry {
            let hash = tc.draw(generators::hash().print_as_debug());

            TreeEntry { object: hash }
        }

        #[hegel::composite]
        pub fn tree(tc: &TestCase) -> Tree {
            let key = gs::text()
                .map(|str| FileName::decode_str(&str))
                .filter(|e| e.is_ok())
                .map(Result::unwrap)
                .print_as_debug();

            let value = generators::tree_entry().print_as_debug();

            tc.draw(
                gs::btree_maps(key, value)
                    .map(|map: BTreeMap<FileName, TreeEntry>| {
                        map.into_iter()
                            .map(|(name, entry)| (TreeKey::new(name, 0o100644), entry))
                            .collect::<BTreeMap<TreeKey, TreeEntry>>()
                    })
                    .map(Tree::new)
                    .print_as_debug(),
            )
        }

        #[hegel::composite]
        pub fn name(tc: &TestCase) -> Name {
            tc.draw(
                gs::text()
                    .map(|x| x.into())
                    .map(Name::try_new)
                    .filter(Result::is_ok)
                    .map(Result::unwrap)
                    .print_as_debug(),
            )
        }

        #[hegel::composite]
        pub fn email(tc: &TestCase) -> Email {
            tc.draw(
                gs::emails()
                    .map(|x| x.into())
                    .map(Email::new)
                    .print_as_debug(),
            )
        }

        #[hegel::composite]
        pub fn identity(tc: &TestCase) -> Identity {
            let name = tc.draw(name().print_as_debug());
            let email = tc.draw(email().print_as_debug());

            Identity::new(name, email)
        }

        #[hegel::composite]
        pub fn property(tc: &TestCase) -> CommitProperty {
            tc.draw(gs::tuples!(gs::text(), gs::text()))
        }

        #[hegel::composite]
        pub fn object_type(tc: &TestCase) -> ObjectType {
            tc.draw(gs::sampled_from(ObjectType::VARIANTS).print_as_debug())
        }

        #[hegel::composite]
        pub fn timestamp(tc: &TestCase) -> Timestamp {
            tc.draw(
                datetimes()
                    .map(Timestamp::try_new)
                    .filter(Result::is_ok)
                    .map(Result::unwrap)
                    .print_as_debug(),
            )
        }

        #[hegel::composite]
        pub fn commit(tc: &TestCase) -> Commit {
            let tree = tc.draw(hash().print_as_debug());
            let parents = tc.draw(gs::vecs(hash()).print_as_debug());
            let author = tc.draw(gs::tuples!(identity(), timestamp()).print_as_debug());
            let committer = tc.draw(gs::tuples!(identity(), timestamp()).print_as_debug());
            let extra = tc.draw(gs::vecs(property()).print_as_debug());
            let description = tc.draw(gs::text().map(|x| x.into()).print_as_debug());

            Commit::new(tree, parents, author, committer, extra, description)
        }

        #[hegel::composite]
        pub fn tag(tc: &TestCase) -> Tag {
            let target = tc.draw(gs::tuples!(hash(), object_type()).print_as_debug());
            let name = tc.draw(gs::text());
            let tagger = tc.draw(gs::optional(
                gs::tuples!(identity(), timestamp()).print_as_debug(),
            ));
            let description = tc.draw(gs::text());

            Tag::new(target, name, tagger, description)
        }
    }

    #[hegel::test]
    #[ignore = "see https://github.com/podikoglou/tinygit/issues/13"]
    fn roundtrip_blob(tc: TestCase) {
        let object = tc.draw(generators::blob().map(Object::from).print_as_debug());

        // write to buffer
        let mut buf: Vec<u8> = Vec::new();
        object.encode(&mut buf).unwrap();

        // read back
        let raw = CompressedLooseObject::decode_bytes(&buf)
            .unwrap()
            .into_inner()
            .into_inner();

        let read_blob = raw
            .into_object(ParserContext::None)
            .expect("should be able to parse");

        assert_eq!(read_blob, object);
    }

    #[hegel::test]
    #[ignore = "see https://github.com/podikoglou/tinygit/issues/13"]
    fn roundtrip_tree(tc: TestCase) {
        let object = tc.draw(generators::tree().map(Object::from).print_as_debug());

        // write to buffer
        let mut buf: Vec<u8> = Vec::new();
        object.encode(&mut buf).unwrap();

        // read back
        let raw = CompressedLooseObject::decode_bytes(&buf)
            .unwrap()
            .into_inner()
            .into_inner();

        let read_tree = raw
            .into_object(ParserContext::None)
            .expect("should be able to parse");

        assert_eq!(read_tree, object);
    }

    #[hegel::test]
    #[ignore = "see https://github.com/podikoglou/tinygit/issues/13"]
    fn roundtrip_commit(tc: TestCase) {
        let object = tc.draw(generators::commit().map(Object::from).print_as_debug());

        // write to buffer
        let mut buf: Vec<u8> = Vec::new();
        object.encode(&mut buf).unwrap();

        // read back
        let raw = CompressedLooseObject::decode_bytes(&buf)
            .unwrap()
            .into_inner()
            .into_inner();

        let read_commit = raw
            .into_object(ParserContext::None)
            .expect("should be able to parse");

        assert_eq!(read_commit, object);
    }

    #[hegel::test]
    #[ignore = "see https://github.com/podikoglou/tinygit/issues/13"]
    fn roundtrip_tag(tc: TestCase) {
        let object = tc.draw(generators::tag().map(Object::from).print_as_debug());

        // write to buffer
        let mut buf: Vec<u8> = Vec::new();
        object.encode(&mut buf).unwrap();

        // read back
        let raw = CompressedLooseObject::decode_bytes(&buf)
            .unwrap()
            .into_inner()
            .into_inner();

        let read_tag = raw
            .into_object(ParserContext::None)
            .expect("should be able to parse");

        assert_eq!(read_tag, object);
    }

    #[test]
    fn roundtrip_tag_with_empty_description() {
        let object = Object::from(Tag::new(
            (ObjectHash::from([0; 20]), ObjectType::Blob),
            "v1".to_owned(),
            None,
            String::new(),
        ));

        let mut buf = Vec::new();
        object.encode(&mut buf).unwrap();

        let raw = CompressedLooseObject::decode_bytes(&buf)
            .unwrap()
            .into_inner()
            .into_inner();

        let read_tag = raw
            .into_object(ParserContext::None)
            .expect("should be able to parse");

        assert_eq!(read_tag, object);
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
