use bolero::check;
use bolero::generator::ValueGenerator;
use chrono::{DateTime, FixedOffset};
use strum::IntoDiscriminant;
use tinygit::{
    ende::{Decode, Encode},
    error::ParserContext,
    fs::FileName,
    identity::Identity,
    object::{Object, commit::Commit, parse_header, tag::Tag, tree::Tree},
    parsing::Stream,
    storage::{
        index::{Flags, IndexEntry, Mode, ObjectType},
        object::{
            RawObject,
            loose::{CompressedLooseObject, LooseObject},
            packed::{
                DeltifiedObject, PackedObject, PackedObjectHeader, PackedObjectType,
                PackfileHeader, UndeltifiedObject, base_object,
                delta::{CopyInstruction, Delta, InsertInstruction, Instruction},
                size,
            },
        },
        reference::RefPointer,
    },
    time::Timestamp,
};
use winnow::Parser;

/// Fuzz the uncompressed loose object parser with arbitrary raw bytes.
/// Whenever an input successfully parses into a valid Tinygit `Object`,
/// verify that serializing it and re-parsing it is completely lossless (roundtrip).
#[test]
fn fuzz_read_loose_object() {
    check!().for_each(|input: &[u8]| {
        if let Ok(object) =
            CompressedLooseObject::decode_bytes_with_context(input, ParserContext::None)
        {
            let mut serialized = Vec::new();

            if object.encode(&mut serialized).is_ok() {
                let roundtripped = CompressedLooseObject::decode_bytes_with_context(
                    &serialized[..],
                    ParserContext::None,
                )
                .expect("roundtripped object should parse successfully");

                assert_eq!(object, roundtripped);
            }
        }
    });
}

/// Fuzz the zlib-compressed loose object parser with arbitrary bytes to ensure
/// neither flate2/zlib nor winnow panics on corrupt or malformed payloads.
#[test]
fn fuzz_read_loose_object_compressed() {
    check!().for_each(|input: &[u8]| {
        let _ = CompressedLooseObject::decode_bytes_with_context(input, ParserContext::None);
    });
}

/// Fuzz the git object header parser (`<type> <size>\0`).
#[test]
fn fuzz_header() {
    check!().for_each(|input: &[u8]| {
        let mut stream = Stream::new(input);
        let _ = parse_header(&mut stream);
    });
}

/// Fuzz the raw timestamp parser on arbitrary byte sequences.
#[test]
fn fuzz_timestamp() {
    check!().for_each(|input: &[u8]| {
        let _ = Timestamp::decode_bytes(input);
    });
}

/// Property test for Timestamp using structured generation:
/// Generate valid (unix_secs, offset_hours, offset_minutes) directly,
/// format them into Git timestamp wire format, and verify roundtrip parsing.
#[test]
fn prop_timestamp_roundtrip() {
    let secs = 0i64..2_000_000_000i64;
    let offset_hours = -12i32..=14i32;
    let offset_minutes = 0i32..60i32;

    check!()
        .with_generator((secs, offset_hours, offset_minutes))
        .for_each(|&(s, h, m)| {
            let total_offset_secs = h * 3600 + if h >= 0 { m * 60 } else { -m * 60 };
            if let Some(offset) = FixedOffset::east_opt(total_offset_secs) {
                if let Some(dt) = DateTime::from_timestamp(s, 0) {
                    let dt = dt.with_timezone(&offset);
                    if let Ok(ts) = Timestamp::try_new(dt) {
                        let sign = if h >= 0 { '+' } else { '-' };
                        let formatted =
                            format!("{} {}{:02}{:02}", ts.timestamp(), sign, h.abs(), m);
                        let parsed = Timestamp::decode_str(&formatted)
                            .expect("validly formatted timestamp should parse");
                        assert_eq!(parsed, ts);
                    }
                }
            }
        });
}

/// Fuzz the identity parser (`Name <email>`).
#[test]
fn fuzz_identity() {
    check!().for_each(|input: &[u8]| {
        let _ = Identity::decode_bytes(input);
    });
}

/// Fuzz the null-terminated filename parser.
#[test]
fn fuzz_file_name() {
    check!().for_each(|input: &[u8]| {
        let _ = FileName::decode_bytes(input);
    });
}

/// Fuzz the commit object parser.
#[test]
fn fuzz_commit() {
    check!().for_each(|input: &[u8]| {
        let _ = Commit::decode_bytes(input);
    });
}

/// Fuzz the tag object parser.
#[test]
fn fuzz_tag() {
    check!().for_each(|input: &[u8]| {
        let _ = Tag::decode_bytes(input);
    });
}

/// Fuzz the tree object parser.
#[test]
fn fuzz_tree() {
    check!().for_each(|input: &[u8]| {
        let _ = Tree::decode_bytes(input);
    });
}

/// Fuzz the insert instruction parser and writer (roundtrip)
#[test]
fn fuzz_insert_instruction() {
    check!().for_each(|input: &[u8]| {
        if let Ok(instruction) = InsertInstruction::decode_bytes(input) {
            let mut buf = Vec::default();

            instruction.encode(&mut buf).expect("should be encodable");

            assert_eq!(buf, input);
        }
    });
}

/// Fuzz the copy instruction parser and writer (roundtrip)
///
/// This property is a little different than others. The flow is:
/// 1. Get input data
/// 2. Parse it
/// 3. Encode it to bytes
/// 3. Parse it
/// 4. Compare the two parsed versions
///
/// This is because copy instructions speciically are not bijective, i.e. there can be multiple raw
/// bytes that map to the same instruction representation, and our encoder isn't guaranteed to
/// output the exact same raw bytes as we were given, despite them being correct and parsing to the
/// same instruction.
#[test]
fn fuzz_copy_instruction() {
    check!().for_each(|input: &[u8]| {
        if let Ok(instruction) = CopyInstruction::decode_bytes(input) {
            let mut buf = Vec::default();

            instruction.encode(&mut buf).expect("should be encodable");

            let instruction_2 = CopyInstruction::decode_bytes(&buf).expect("should be parseable");
            assert_eq!(instruction_2, instruction);
        }
    });
}

#[test]
fn fuzz_delta_instruction() {
    check!().for_each(|input: &[u8]| {
        if let Ok(instr) = Instruction::decode_bytes(input) {
            let mut buf = Vec::default();

            instr.encode(&mut buf).expect("should be encodable");

            let instruction_2 = Instruction::decode_bytes(&buf).expect("should be parseable");
            assert_eq!(instruction_2, instr);
        }
    });
}

#[test]
fn fuzz_size() {
    check!().for_each(|input: &[u8]| {
        let stream = Stream::new(input);

        if let Ok(parsed) = size.parse(stream) {
            let mut buf = Vec::default();

            (parsed as usize)
                .encode(&mut buf)
                .expect("should be encodable");

            let parsed_2 = size.parse(Stream::new(&buf)).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_delta() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = Delta::decode_bytes(input) {
            let mut buf = Vec::default();

            (parsed).encode(&mut buf).expect("should be encodable");

            let parsed_2 = Delta::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_undeltified_object() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = UndeltifiedObject::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = UndeltifiedObject::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_ofs_delta_base_object() {
    check!().for_each(|input: &[u8]| {
        let stream = Stream::new(input);

        if let Ok(parsed) = base_object(PackedObjectType::OfsDelta).parse(stream) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = base_object(PackedObjectType::OfsDelta)
                .parse(Stream::new(&buf))
                .expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_ref_delta_base_object() {
    check!().for_each(|input: &[u8]| {
        let stream = Stream::new(input);

        if let Ok(parsed) = base_object(PackedObjectType::RefDelta).parse(stream) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = base_object(PackedObjectType::RefDelta)
                .parse(Stream::new(&buf))
                .expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_object_header() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = PackedObjectHeader::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = PackedObjectHeader::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_deltified_object() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = DeltifiedObject::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = DeltifiedObject::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_pack_header() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = PackfileHeader::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = PackfileHeader::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn fuzz_packed_object() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = PackedObject::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = PackedObject::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn roundtrip_ref_pointer() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = RefPointer::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            let parsed_2 = RefPointer::decode_bytes(&buf).expect("should be parseable");

            assert_eq!(parsed_2, parsed);
        }
    });
}

#[test]
fn mode_properties() {
    check!().for_each(|input: &[u8]| {
        if let Ok(mode) = Mode::decode_bytes(input) {
            match mode.object_type() {
                ObjectType::Regular => {
                    assert!(mode.permission() == 0o0755 || mode.permission() == 0o0644)
                }
                ObjectType::SymbolicLink | ObjectType::GitLink => {
                    assert_eq!(mode.permission(), 0)
                }
            }
        }
    });
}

#[test]
fn mode_roundtrip() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = Mode::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            assert_eq!(buf, input);
        }
    });
}

#[test]
fn flags_roundtrip() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = Flags::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            assert_eq!(buf, input);
        }
    });
}

#[test]
fn index_entry_roundtrip() {
    check!().for_each(|input: &[u8]| {
        if let Ok(parsed) = IndexEntry::decode_bytes(input) {
            let mut buf = Vec::default();

            parsed.encode(&mut buf).expect("should be encodable");

            assert_eq!(buf, input);
        }
    });
}

mod generators {
    use std::collections::BTreeMap;

    use bolero::generator::bolero_generator::Driver;
    use bolero::generator::{ValueGenerator, one_value_of, produce};
    use chrono::{DateTime, FixedOffset};
    use tinygit::{
        fs::FileName,
        identity::{Email, Identity, Name},
        object::{
            ObjectType,
            blob::Blob,
            commit::Commit,
            hash::ObjectHash,
            tag::Tag,
            tree::{Tree, TreeEntry, TreeKey},
        },
        time::Timestamp,
    };

    #[derive(Clone, Copy, Debug)]
    pub struct HashGen;

    impl ValueGenerator for HashGen {
        type Output = ObjectHash;

        fn generate<D: Driver>(&self, driver: &mut D) -> Option<Self::Output> {
            produce::<[u8; 20]>().generate(driver).map(ObjectHash::from)
        }
    }

    pub fn hash() -> HashGen {
        HashGen
    }

    pub fn blob() -> impl ValueGenerator<Output = Blob> {
        produce::<Vec<u8>>().map_gen(Blob::new)
    }

    fn file_name() -> impl ValueGenerator<Output = FileName> {
        produce::<String>().filter_map_gen(|s| FileName::try_new(s).ok())
    }

    fn name() -> impl ValueGenerator<Output = Name> {
        produce::<String>().filter_map_gen(|s| Name::try_new(s.into()).ok())
    }

    fn email() -> impl ValueGenerator<Output = Email> {
        produce::<String>().filter_map_gen(|s| Email::try_new(s.into()).ok())
    }

    fn identity() -> impl ValueGenerator<Output = Identity> {
        (name(), email()).map_gen(|(name, email)| Identity::new(name, email))
    }

    fn timestamp() -> impl ValueGenerator<Output = Timestamp> {
        (0i64..2_000_000_000i64, -720i32..=840i32).filter_map_gen(|(secs, offset_minutes)| {
            let offset = FixedOffset::east_opt(offset_minutes * 60)?;
            let datetime = DateTime::from_timestamp(secs, 0)?.with_timezone(&offset);

            Timestamp::try_new(datetime).ok()
        })
    }

    fn object_type() -> impl ValueGenerator<Output = ObjectType> {
        one_value_of([
            ObjectType::Blob,
            ObjectType::Tree,
            ObjectType::Commit,
            ObjectType::Tag,
        ])
    }

    fn property() -> impl ValueGenerator<Output = (String, String)> {
        (produce::<String>(), produce::<String>())
    }

    #[derive(Clone, Debug)]
    struct VecGen<G> {
        values: G,
        len: std::ops::RangeInclusive<usize>,
    }

    impl<G: ValueGenerator> ValueGenerator for VecGen<G> {
        type Output = Vec<G::Output>;

        fn generate<D: Driver>(&self, driver: &mut D) -> Option<Self::Output> {
            let len = self.len.generate(driver)?;
            let mut values = Vec::with_capacity(len);

            for _ in 0..len {
                values.push(self.values.generate(driver)?);
            }

            Some(values)
        }
    }

    fn vec_of<G: ValueGenerator>(values: G) -> VecGen<G> {
        VecGen { values, len: 0..=4 }
    }

    fn tree_entry() -> impl ValueGenerator<Output = TreeEntry> {
        hash().map_gen(TreeEntry::new)
    }

    pub fn tree() -> impl ValueGenerator<Output = Tree> {
        vec_of((file_name(), tree_entry())).map_gen(|entries| {
            Tree::new(
                entries
                    .into_iter()
                    .map(|(name, entry)| (TreeKey::new(name, 0o100644), entry))
                    .collect::<BTreeMap<_, _>>(),
            )
        })
    }

    pub fn commit() -> impl ValueGenerator<Output = Commit> {
        (
            hash(),
            vec_of(hash()),
            (identity(), timestamp()),
            (identity(), timestamp()),
            vec_of(property()),
            produce::<Vec<u8>>(),
        )
            .map_gen(|(tree, parents, author, committer, extra, description)| {
                Commit::new(tree, parents, author, committer, extra, description.into())
            })
    }

    pub fn tag() -> impl ValueGenerator<Output = Tag> {
        (
            (hash(), object_type()),
            produce::<String>(),
            (produce::<bool>(), identity(), timestamp())
                .map_gen(|(some, identity, timestamp)| some.then_some((identity, timestamp))),
            produce::<String>(),
        )
            .map_gen(|(target, name, tagger, description)| {
                Tag::new(target, name, tagger, description)
            })
    }
}

fn roundtrip_object(object: Object) {
    let mut raw = Vec::new();
    object.encode(&mut raw).expect("object should encode");

    let mut buf = Vec::new();
    CompressedLooseObject::new(LooseObject::new(RawObject::new(object.discriminant(), raw)))
        .encode(&mut buf)
        .expect("loose object should encode");

    let read = CompressedLooseObject::decode_bytes(&buf)
        .expect("loose object should decode")
        .into_inner()
        .into_inner()
        .into_object(ParserContext::None)
        .expect("object should parse");

    assert_eq!(read, object);
}

#[test]
fn roundtrip_blob() {
    check!()
        .with_generator(generators::blob().map_gen(Object::from))
        .cloned()
        .for_each(roundtrip_object);
}

#[test]
fn roundtrip_tree() {
    check!()
        .with_generator(generators::tree().map_gen(Object::from))
        .cloned()
        .for_each(roundtrip_object);
}

#[test]
fn roundtrip_commit() {
    check!()
        .with_generator(generators::commit().map_gen(Object::from))
        .cloned()
        .for_each(roundtrip_object);
}

#[test]
fn roundtrip_tag() {
    check!()
        .with_generator(generators::tag().map_gen(Object::from))
        .cloned()
        .for_each(roundtrip_object);
}

#[test]
fn hash_prefix_debug_eq_display() {
    check!()
        .with_generator(generators::hash().map_gen(|hash| hash.prefix()))
        .cloned()
        .for_each(|prefix| assert_eq!(format!("{prefix:?}"), format!("{prefix}")));
}

#[test]
fn hash_prefix_length_2() {
    check!()
        .with_generator(generators::hash().map_gen(|hash| hash.prefix()))
        .cloned()
        .for_each(|prefix| {
            assert_eq!(format!("{prefix}").len(), 2);
            assert_eq!(format!("{prefix:?}").len(), 2);
        });
}

#[test]
fn hash_debug_eq_display() {
    check!()
        .with_generator(generators::hash())
        .cloned()
        .for_each(|hash| assert_eq!(format!("{hash:?}"), format!("{hash}")));
}

#[test]
fn hash_len_40() {
    check!()
        .with_generator(generators::hash())
        .cloned()
        .for_each(|hash| assert_eq!(format!("{hash}").len(), 40));
}
