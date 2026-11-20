//! Parsing and querying a single Git configuration file, without include expansion.

use std::{collections::HashSet, path::Path};

use winnow::{
    ModalResult, Parser,
    ascii::space0,
    combinator::{alt, delimited, empty, eof, opt, peek, preceded, repeat, seq, terminated},
    error::{StrContext, StrContextValue},
    token::{any, take_while},
};

use crate::{TinygitError, ende::Decode, parsing::Stream};

/// The sections of a single configuration file, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    pub sections: Vec<Section>,
}

/// A named section and its optional case-sensitive subsection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub subsection: Option<String>,
    pub entries: Vec<Entry>,
}

/// A lowercased key and its optional value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    /// `None` denotes a bare key (implicit true), rather than an empty value.
    pub value: Option<String>,
}

/// The URLs and fetch refspecs of a configured remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote<'a> {
    pub name: &'a str,
    pub urls: Vec<&'a str>,
    pub push_urls: Vec<&'a str>,
    pub fetch: Vec<&'a str>,
}

impl Config {
    /// Reads and parses a configuration file without expanding includes.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TinygitError> {
        Self::decode_file(path)
    }

    /// Returns all occurrences in file order, preserving bare keys.
    pub fn get_all<'a>(
        &'a self,
        section: &'a str,
        subsection: Option<&'a str>,
        key: &'a str,
    ) -> impl Iterator<Item = Option<&'a str>> + 'a {
        self.sections
            .iter()
            .filter(move |s| {
                s.name.eq_ignore_ascii_case(section) && s.subsection.as_deref() == subsection
            })
            .flat_map(|s| &s.entries)
            .filter(move |e| e.key.eq_ignore_ascii_case(key))
            .map(|e| e.value.as_deref())
    }

    /// The outer option distinguishes a missing key from a bare key.
    pub fn get<'a>(
        &'a self,
        section: &'a str,
        subsection: Option<&'a str>,
        key: &'a str,
    ) -> Option<Option<&'a str>> {
        self.get_all(section, subsection, key).last()
    }

    /// Distinct subsection names in first-seen order.
    pub fn subsections<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let mut seen = HashSet::new();
        self.sections
            .iter()
            .filter(move |s| s.name.eq_ignore_ascii_case(section))
            .filter_map(|s| s.subsection.as_deref())
            .filter(move |name| seen.insert(*name))
    }

    /// Looks up a remote by its case-sensitive name.
    pub fn remote<'a>(&'a self, name: &'a str) -> Option<Remote<'a>> {
        self.subsections("remote")
            .find(|n| *n == name)
            .map(|name| Remote {
                name,
                urls: self
                    .get_all("remote", Some(name), "url")
                    .flatten()
                    .collect(),
                push_urls: self
                    .get_all("remote", Some(name), "pushurl")
                    .flatten()
                    .collect(),
                fetch: self
                    .get_all("remote", Some(name), "fetch")
                    .flatten()
                    .collect(),
            })
    }

    /// Iterates over distinct remotes in first-seen order.
    pub fn remotes(&self) -> impl Iterator<Item = Remote<'_>> {
        self.subsections("remote")
            .filter_map(|name| self.remote(name))
    }
}

fn identifier(input: &mut Stream<'_>) -> ModalResult<String> {
    take_while(1.., |c: u8| c.is_ascii_alphanumeric() || c == b'-')
        .map(str::from_utf8)
        .verify_map(Result::ok)
        .map(str::to_ascii_lowercase)
        .parse_next(input)
}

fn name(input: &mut Stream<'_>) -> ModalResult<String> {
    identifier
        .context(StrContext::Label("section name"))
        .parse_next(input)
}

fn key(input: &mut Stream<'_>) -> ModalResult<String> {
    identifier
        .verify(|s: &String| s.starts_with(|c: char| c.is_ascii_alphabetic()))
        .context(StrContext::Label("config key"))
        .parse_next(input)
}

fn subsection(input: &mut Stream<'_>) -> ModalResult<String> {
    terminated(
        repeat(
            0..,
            alt((
                preceded(b'\\', any.verify(|c| !matches!(c, b'\n' | b'\r' | 0))),
                any.verify(|c| !matches!(c, b'"' | b'\\' | b'\n' | b'\r' | 0)),
            )),
        )
        .try_map(String::from_utf8),
        b'"'.context(StrContext::Expected(StrContextValue::CharLiteral('"'))),
    )
    .context(StrContext::Label("subsection"))
    .parse_next(input)
}

fn header(input: &mut Stream<'_>) -> ModalResult<Section> {
    seq! {Section {
        _: "[",
        name: name,
        subsection: opt(alt((
            seq!(_: take_while(1.., (b' ', b'\t')), _: "\"", subsection).map(|(s,)| s),
            seq!(_: ".", take_while(1.., |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.')))
                .try_map(|(s,): (&[u8],)| str::from_utf8(s).map(str::to_ascii_lowercase)),
        ))),
        _: space0,
        _: "]",
        entries: empty.value(Vec::new()),
    }}
    .context(StrContext::Label("config section"))
    .parse_next(input)
}

fn escape(input: &mut Stream<'_>) -> ModalResult<Vec<u8>> {
    preceded(
        b'\\',
        alt((
            b'n'.value(vec![b'\n']),
            b't'.value(vec![b'\t']),
            b'b'.value(vec![8]),
            b'\\'.value(vec![b'\\']),
            b'"'.value(vec![b'"']),
        )),
    )
    .context(StrContext::Label("value escape"))
    .parse_next(input)
}

fn continuation(input: &mut Stream<'_>) -> ModalResult<Vec<u8>> {
    preceded(b'\\', alt(("\r\n", "\n")))
        .value(Vec::new())
        .parse_next(input)
}

fn quoted_value(input: &mut Stream<'_>) -> ModalResult<Vec<u8>> {
    delimited(
        b'"',
        repeat(
            0..,
            alt((
                escape,
                continuation,
                take_while(1.., |c| !matches!(c, b'"' | b'\\' | b'\n' | b'\r' | 0))
                    .map(<[u8]>::to_vec),
            )),
        )
        .map(|parts: Vec<Vec<u8>>| parts.concat()),
        b'"'.context(StrContext::Expected(StrContextValue::CharLiteral('"'))),
    )
    .context(StrContext::Label("quoted value"))
    .parse_next(input)
}

fn value(input: &mut Stream<'_>) -> ModalResult<String> {
    preceded(
        space0,
        repeat(
            0..,
            alt((
                quoted_value.map(|v| (v, false)),
                escape.map(|v| (v, false)),
                continuation.map(|v| (v, false)),
                take_while(1.., |c| {
                    !matches!(c, b'"' | b'\\' | b'\n' | b'\r' | b'#' | b';' | 0)
                })
                .map(|v: &[u8]| (v.to_vec(), true)),
            )),
        ),
    )
    .map(|mut parts: Vec<(Vec<u8>, bool)>| {
        for (bytes, unquoted) in parts.iter_mut().rev() {
            if !*unquoted {
                break;
            }
            while matches!(bytes.last(), Some(b' ' | b'\t')) {
                bytes.pop();
            }
            if !bytes.is_empty() {
                break;
            }
        }
        parts
            .into_iter()
            .flat_map(|(bytes, _)| bytes)
            .collect::<Vec<_>>()
    })
    .try_map(String::from_utf8)
    .context(StrContext::Label("config value"))
    .parse_next(input)
}

fn entry(input: &mut Stream<'_>) -> ModalResult<Entry> {
    seq! {Entry {
        key: key,
        _: space0,
        value: opt(seq!(_: "=", value.context(StrContext::Label("config value"))).map(|(v,)| v)),
    }}
    .context(StrContext::Label("config entry"))
    .parse_next(input)
}

fn line_end(input: &mut Stream<'_>) -> ModalResult<()> {
    seq!(
        _: space0,
        _: opt(preceded(alt((b'#', b';')), take_while(0.., |c| !matches!(c, b'\n' | b'\r' | 0)))),
        _: alt(("\r\n".void(), "\n".void(), eof.void())),
    )
    .void()
    .context(StrContext::Label("config line ending"))
    .parse_next(input)
}

fn line(input: &mut Stream<'_>) -> ModalResult<(Option<Section>, Option<Entry>)> {
    seq!(
        _: peek(any),
        _: space0,
        opt(header),
        _: space0,
        opt(entry),
        _: line_end,
    )
    .context(StrContext::Label("config line"))
    .parse_next(input)
}

impl Decode for Config {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        preceded(opt(b"\xef\xbb\xbf"), repeat(0.., line))
            .verify_map(|lines: Vec<(Option<Section>, Option<Entry>)>| {
                lines
                    .into_iter()
                    .try_fold(Config::default(), |mut config, (section, entry)| {
                        if let Some(section) = section {
                            config.sections.push(section);
                        }
                        if let Some(entry) = entry {
                            config.sections.last_mut()?.entries.push(entry);
                        }
                        Some(config)
                    })
            })
            .context(StrContext::Label("config"))
            .parse_next(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn parses_sections() {
        for bytes in [
            b"[CoRe]".as_slice(),
            b"[remote \"A\\\"B\\\\C\"]",
            b"[Remote.OLD.Name]",
        ] {
            assert_debug_snapshot!(header.parse_peek(Stream::new(bytes)));
        }
    }

    #[test]
    fn parses_entries() {
        for bytes in [
            b"enabled".as_slice(),
            b"empty =",
            b"url =  a  \" b # ; \" c  # comment",
            include_bytes!("../tests/fixtures/config/entry-continuation.gitconfig"),
        ] {
            assert_debug_snapshot!(entry.parse_peek(Stream::new(bytes)));
        }
    }

    #[test]
    fn queries_config() {
        let config =
            Config::decode_str(include_str!("../tests/fixtures/config/queries.gitconfig")).unwrap();
        assert_eq!(
            config.get("REMOTE", Some("Origin"), "URL"),
            Some(Some("last"))
        );
        assert_eq!(config.get("remote", Some("Origin"), "bare"), Some(None));
        assert_eq!(
            config.get("remote", Some("Origin"), "empty"),
            Some(Some(""))
        );
        assert_eq!(config.get("remote", Some("origin"), "url"), None);
        assert_debug_snapshot!(config.remotes().collect::<Vec<_>>());
        let empty = Config::decode_bytes(b"").unwrap();
        assert_eq!(empty.get("core", None, "bare"), None);
        assert_eq!(empty.subsections("remote").count(), 0);
        assert_eq!(empty.remotes().count(), 0);
        assert!(empty.remote("origin").is_none());
    }

    #[test]
    fn remote_urls_preserve_file_order() {
        let config =
            Config::decode_str(include_str!("../tests/fixtures/config/remotes.gitconfig")).unwrap();
        let remote = config.remote("origin").unwrap();
        assert_eq!(remote.urls, ["first", "second"]);
        assert_eq!(remote.push_urls, ["push-first", "push-second"]);
        let empty = config.remote("empty").unwrap();
        assert!(empty.urls.is_empty());
        assert!(empty.push_urls.is_empty());
    }

    #[test]
    fn rejects_invalid_config() {
        for bytes in [
            b"key=x".as_slice(),
            b"[core",
            include_bytes!("../tests/fixtures/config/invalid-key.gitconfig"),
            include_bytes!("../tests/fixtures/config/invalid-value-escape.gitconfig"),
            include_bytes!("../tests/fixtures/config/unclosed-value.gitconfig"),
            include_bytes!("../tests/fixtures/config/unfinished-escape.gitconfig"),
            include_bytes!("../tests/fixtures/config/invalid-utf8.gitconfig"),
            include_bytes!("../tests/fixtures/config/nul-value.gitconfig"),
            include_bytes!("../tests/fixtures/config/invalid-subsection-newline.gitconfig"),
            b"[core] junk other",
        ] {
            assert!(
                matches!(
                    Config::decode_bytes(bytes),
                    Err(TinygitError::ParserError(_))
                ),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn opens_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        assert!(Config::open(&path).is_err());
        std::fs::write(
            &path,
            include_bytes!("../tests/fixtures/config/simple.gitconfig"),
        )
        .unwrap();
        assert_eq!(
            Config::open(path).unwrap().get("core", None, "key"),
            Some(Some("value"))
        );
    }
}
