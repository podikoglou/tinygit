use bolero::check;
use chrono::{DateTime, FixedOffset};
use tinygit::{
    ende::{Decode, Encode},
    error::ParserContext,
    fs::FileName,
    identity::Identity,
    object::{commit::Commit, parse_header, tag::Tag, tree::Tree},
    parsing::Stream,
    storage::object::{
        loose::{read_object, read_object_compressed},
        packed::{
            DeltifiedObject, PackedObject, PackedObjectHeader, PackedObjectType, PackfileHeader,
            UndeltifiedObject, base_object,
            delta::{CopyInstruction, Delta, InsertInstruction, Instruction},
            size,
        },
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
fn mode_properties() {
    check!().for_each(|input: &[u8]| {
        if let Ok(mode) = Mode::decode_bytes(input) {
            assert!(mode.permission() == 0o0755 || mode.permission() == 0o644);

            if let ObjectType::GitLink | ObjectType::SymbolicLink = mode.object_type() {
                assert_eq!(mode.permission(), 0)
            };
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
