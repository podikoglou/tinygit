pub mod blob;
pub mod commit;
pub mod tag;
pub mod tree;

pub mod hash;

use std::io::Write;

use blob::Blob;
use commit::Commit;
use flate2::{Compression, write::ZlibEncoder};
use sha1::{Digest, Sha1};
use strum::{EnumDiscriminants, EnumIs, EnumString, EnumTryAs, IntoStaticStr, VariantArray};
use tag::Tag;
use tree::Tree;
use winnow::{
    ModalResult, Parser,
    ascii::dec_uint,
    combinator::{alt, seq},
    error::{ContextError, ErrMode, StrContext, StrContextValue},
    token::{literal, take},
};

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    error::ParserContext,
    object::hash::ObjectHash,
    parsing::{ParseResultExt, Stream},
    storage::object::{LazyObject, RawObject},
};

#[derive(Debug, PartialEq, Eq, Clone, EnumDiscriminants, EnumIs, EnumTryAs)]
#[strum_discriminants(name(ObjectType))]
#[strum_discriminants(derive(EnumString, IntoStaticStr, VariantArray))]
#[strum_discriminants(strum(ascii_case_insensitive))]
#[strum_discriminants(strum(serialize_all = "lowercase"))]
pub enum Object {
    Blob(Blob),
    Tree(Tree),
    Commit(Box<Commit>),
    Tag(Tag),
}

impl Object {
    /// Creates a SHA1 hash of the object.
    pub fn hash(self) -> Result<ObjectHash, TinygitError> {
        RawObject::try_from(self)?.hash()
    }
}

impl Encode for Object {
    /// Writes the uncompressed object to a write.
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), crate::TinygitError> {
        match self {
            Self::Blob(blob) => blob.encode(writer),
            Self::Tree(tree) => tree.encode(writer),
            Self::Commit(commit) => commit.encode(writer),
            Self::Tag(tag) => tag.encode(writer),
        }
    }
}

impl From<Blob> for Object {
    fn from(val: Blob) -> Self {
        Self::Blob(val)
    }
}

impl From<Tree> for Object {
    fn from(val: Tree) -> Self {
        Self::Tree(val)
    }
}

impl From<Commit> for Object {
    fn from(val: Commit) -> Self {
        Self::Commit(Box::new(val))
    }
}
impl From<Tag> for Object {
    fn from(val: Tag) -> Self {
        Self::Tag(val)
    }
}

impl TryFrom<LazyObject> for Object {
    type Error = TinygitError;

    fn try_from(value: LazyObject) -> Result<Self, Self::Error> {
        value.into_object()
    }
}

/// Parses an [Object] from some input.
pub fn object<'a>(input: &mut Stream<'a>) -> ModalResult<Object> {
    let (typee, size) = parse_header.parse_next(input)?;
    let mut bytes: Stream<'a> = Stream::new(take(size).parse_next(input)?);

    match typee {
        ObjectType::Blob => Blob::parse.map(Object::Blob).parse_next(&mut bytes),
        ObjectType::Tree => Tree::parse.map(Object::Tree).parse_next(&mut bytes),
        ObjectType::Commit => Commit::parse.map(Object::from).parse_next(&mut bytes),
        ObjectType::Tag => Tag::parse.map(Object::from).parse_next(&mut bytes),
    }
}

/// Parse a header (object type and size) from some bytes.
pub fn parse_header(input: &mut Stream<'_>) -> ModalResult<(ObjectType, usize)> {
    let mut size = dec_uint::<_, usize, ErrMode<ContextError>>
        .context(StrContext::Label("payload size"))
        .context(StrContext::Expected(StrContextValue::Description(
            "bytes amount",
        )));

    seq!(parse_object_type, _: " ", size, _: "\0")
        .context(StrContext::Label("header"))
        .parse_next(input)
}

/// Parses an object type string from some bytes.
pub fn parse_object_type(input: &mut Stream<'_>) -> ModalResult<ObjectType> {
    alt((
        literal("blob").value(ObjectType::Blob),
        literal("tree").value(ObjectType::Tree),
        literal("commit").value(ObjectType::Commit),
        literal("tag").value(ObjectType::Tag),
    ))
    .context(StrContext::Label("type"))
    .context(StrContext::Expected(StrContextValue::Description(
        "blob | tree | commit | tag",
    )))
    .parse_next(input)
}

#[cfg(test)]
mod test {
    use std::assert_matches;

    use winnow::{Parser, error::ErrMode};

    use crate::{
        object::{ObjectType, parse_header, parse_object_type},
        parsing::Stream,
    };

    #[test]
    fn object_type_parses_expected_object_types() {
        assert_eq!(
            parse_object_type
                .parse_peek(Stream::new(b"blob"))
                .map(|(r, v)| (*r, v)),
            Ok((&b""[..], ObjectType::Blob))
        );
        assert_eq!(
            parse_object_type
                .parse_peek(Stream::new(b"tree"))
                .map(|(r, v)| (*r, v)),
            Ok((&b""[..], ObjectType::Tree))
        );
    }

    #[test]
    fn object_type_rejects_invalid_input() {
        assert_matches!(
            parse_object_type.parse_peek(Stream::new(b"")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            parse_object_type.parse_peek(Stream::new(b"blo")),
            Err(ErrMode::Backtrack(_))
        );
    }

    #[test]
    fn header_parses_basic_headers() {
        assert_eq!(
            parse_header
                .parse_peek(Stream::new(b"blob 3\0"))
                .map(|(r, v)| (*r, v)),
            Ok((&b""[..], (ObjectType::Blob, 3)))
        );
        assert_eq!(
            parse_header
                .parse_peek(Stream::new(b"tree 333\0"))
                .map(|(r, v)| (*r, v)),
            Ok((&b""[..], (ObjectType::Tree, 333)))
        );
    }

    #[test]
    fn header_rejets_invalid_input() {
        assert_matches!(
            parse_header.parse_peek(Stream::new(b"tre 3")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            parse_header.parse_peek(Stream::new(b"tree ")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            parse_header.parse_peek(Stream::new(b"tree \0")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            parse_header.parse_peek(Stream::new(b"tree\0")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            parse_header.parse_peek(Stream::new(b"3")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            parse_header.parse_peek(Stream::new(b"3\0")),
            Err(ErrMode::Backtrack(_))
        );
    }
}
