use bstr::BString;
use nutype::nutype;
use winnow::{
    ModalResult, Parser,
    combinator::{repeat, seq},
    error::StrContext,
    token::rest,
};

use crate::{
    ende::Decode,
    ende::Encode,
    identity::Identity,
    object::hash::{ObjectHash, parse_object_hash_str},
    parsing::{Stream, extra_property, property},
    time::Timestamp,
};
use std::io::Write;

/// A commit an object that contains information about a commit.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Commit {
    pub tree: ObjectHash,
    pub parents: Vec<ObjectHash>,
    pub author: (Identity, Timestamp),
    pub committer: (Identity, Timestamp),
    pub extra: Vec<CommitProperty>,

    pub description: BString,
}

impl Commit {
    #[must_use]
    pub fn new(
        tree: ObjectHash,
        parents: Vec<ObjectHash>,
        author: (Identity, Timestamp),
        committer: (Identity, Timestamp),
        extra: Vec<CommitProperty>,
        description: BString,
    ) -> Self {
        Self {
            tree,
            parents,
            author,
            committer,
            extra,
            description,
        }
    }

    /// Attempts to get the GPG Signature (including the armor) used to sign this commit.
    #[must_use]
    pub fn gpg_signature(&self) -> Option<&PropertyValue> {
        self.extra
            .iter()
            .find_map(|(key, value)| if key == "gpgsig" { Some(value) } else { None })
    }
}

impl Encode for Commit {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), crate::TinygitError> {
        write!(writer, "tree {}", self.tree)?;
        writeln!(writer)?;

        for parent in &self.parents {
            write!(writer, "parent {}", parent)?;
            writeln!(writer)?;
        }

        write!(writer, "author ")?;
        self.author.encode(writer)?;
        writeln!(writer)?;

        write!(writer, "committer ")?;
        self.committer.encode(writer)?;
        writeln!(writer)?;

        for property in &self.extra {
            property.encode(writer)?;
            writeln!(writer)?;
        }

        writeln!(writer)?;
        writer.write_all(self.description.as_ref())?;

        Ok(())
    }
}

impl Decode for Commit {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        seq! {Commit{
            tree: property("tree", parse_object_hash_str),
            parents: repeat(0.., property("parent", parse_object_hash_str)),
            author: property("author", <(Identity, Timestamp)>::parse),
            committer: property("committer", <(Identity, Timestamp)>::parse),
            extra: repeat(0.., extra_property),
            _: "\n",
            description: rest.map(|x: &[u8]| x.into()),
        }}
        .context(StrContext::Label("commit object"))
        .parse_next(input)
    }
}

impl Decode for (Identity, Timestamp) {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        seq!(Identity::parse, _: " ", Timestamp::parse).parse_next(input)
    }
}

impl Encode for (Identity, Timestamp) {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), crate::TinygitError> {
        self.0.encode(writer)?;
        write!(writer, " {}", self.1.format("%s %z"))?;

        Ok(())
    }
}

pub type CommitProperty = (PropertyKey, PropertyValue);

#[nutype(
    sanitize(with = |x| x.trim_ascii().into()),
    validate(predicate = |x| !x.is_empty() && !x.contains(' ') && !x.contains('\n') && !x.contains('\0')),
    derive(Debug, PartialEq, Eq, Clone, Display, AsRef, Deref)
)]
pub struct PropertyKey(String);

impl PartialEq<&str> for PropertyKey {
    fn eq(&self, other: &&str) -> bool {
        self.as_ref() == *other
    }
}

impl PartialEq<str> for PropertyKey {
    fn eq(&self, other: &str) -> bool {
        self.as_ref() == other
    }
}

impl PartialEq<PropertyKey> for &str {
    fn eq(&self, other: &PropertyKey) -> bool {
        *self == other.as_ref()
    }
}

#[nutype(
    validate(predicate = |x| !x.contains('\0')),
    derive(Debug, PartialEq, Eq, Clone, Display, AsRef, Deref)
)]
pub struct PropertyValue(String);

impl Encode for &CommitProperty {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), crate::TinygitError> {
        // write key
        write!(writer, "{} ", self.0)?;

        let mut lines = self.1.split("\n");

        let Some(first_line) = lines.next() else {
            return Ok(()); // is this a failure?
        };

        // write first line of value
        write!(writer, "{first_line}")?;

        // write each subsequent line, indented by one space, indented by one space.
        //
        // (the reason we prefix with a newline rather than putting it at the end is
        // because we don't want to finish with one.)
        for line in lines {
            write!(writer, "\n {line}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, FixedOffset};

    use crate::{
        ende::Encode,
        identity::{Email, Identity, Name},
        object::{commit::Commit, hash::ObjectHash},
        time::Timestamp,
    };

    #[test]
    fn writes_basic_commits() {
        let hash = ObjectHash::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let alex = (
            Identity::new(
                Name::try_new("alex".into()).unwrap(),
                Email::try_new("alex.podikoglou@gmail.com".into()).unwrap(),
            ),
            Timestamp::try_new(
                DateTime::from_timestamp(0, 0)
                    .unwrap()
                    .with_timezone(&FixedOffset::west_opt(0).unwrap()),
            )
            .unwrap(),
        );
        let commit = Commit::new(
            hash.clone(),
            vec![hash.clone(), hash],
            alex.clone(),
            alex,
            vec![],
            "test commit\nsome more text maybe\n".into(),
        );

        let mut buf = Vec::new();

        commit.encode(&mut buf).unwrap();

        insta::assert_snapshot!(String::from_utf8(buf).unwrap(), @r#"
        tree 0000000000000000000000000000000000000000
        parent 0000000000000000000000000000000000000000
        parent 0000000000000000000000000000000000000000
        author alex <alex.podikoglou@gmail.com> 0 +0000
        committer alex <alex.podikoglou@gmail.com> 0 +0000

        test commit
        some more text maybe
        "#);
    }
}
