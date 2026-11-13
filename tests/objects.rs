use tinygit::object::hash::ObjectHash;

mod common;

mod roundtrip {
    use strum::IntoDiscriminant;
    use tinygit::{
        ende::{Decode, Encode},
        error::ParserContext,
        object::{Object, ObjectType, hash::ObjectHash, tag::Tag},
        storage::object::{
            RawObject,
            loose::{CompressedLooseObject, LooseObject},
        },
    };

    #[test]
    fn roundtrip_tag_with_empty_description() {
        let object = Object::from(Tag::new(
            (ObjectHash::from([0; 20]), ObjectType::Blob),
            "v1".to_owned(),
            None,
            String::new(),
        ));

        let mut raw = Vec::new();
        object.encode(&mut raw).unwrap();

        let mut buf = Vec::new();
        CompressedLooseObject::new(LooseObject::new(RawObject::new(object.discriminant(), raw)))
            .encode(&mut buf)
            .unwrap();

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

#[test]
fn objects_iter() {
    let (repo, _dir) = include_repo!("fixtures/repos/repo-1.tar");

    let objects = repo
        .store
        .objects()
        .expect("should be able to list objects");

    assert_eq!(objects.count(), 3, "should have 3 objects");
}

#[test]
fn store_object_retrieval() {
    let (repo, _dir) = include_repo!("fixtures/repos/repo-5.tar");

    // loose object
    let loose_hash: ObjectHash = "ddc67c9b017423980bd3e82879ada0e43a71b8ba"
        .parse()
        .expect("should parse hash");
    let loose_object = repo
        .store
        .object(&loose_hash)
        .expect("should find loose object");
    assert_eq!(
        loose_object.hash().expect("should calculate hash"),
        loose_hash
    );

    // packed object
    let packed_hash: ObjectHash = "babbe85f1e25b2b7d2c4321bc9e278bae96b718b"
        .parse()
        .expect("should parse hash");
    let packed_object = repo
        .store
        .object(&packed_hash)
        .expect("should find packed object");
    assert_eq!(
        packed_object.hash().expect("should calculate hash"),
        packed_hash
    );
}
