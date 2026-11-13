//! This module contains various utilities for parsing object files using the `winnow` crate.

use std::io::Error;

use winnow::{
    LocatingSlice, ModalResult, Parser,
    ascii::oct_digit1,
    combinator::{repeat, seq, terminated},
    error::{ContextError, ErrMode, Needed, StrContext},
    stream::Location,
    token::{take_till, take_until},
};

use crate::{
    error::{ParseError, ParserContext, TinygitError},
    object::commit::{CommitProperty, PropertyKey, PropertyValue},
};

pub type Stream<'a> = LocatingSlice<&'a [u8]>;

/// Helper for creating parsers that parse a key value pair found in a commit object, such as
/// `author <author>`
pub fn property<'a, O>(
    mut key: impl Parser<Stream<'a>, &'a [u8], ErrMode<ContextError>>,
    mut value: impl Parser<Stream<'a>, O, ErrMode<ContextError>>,
) -> impl Parser<Stream<'a>, O, ErrMode<ContextError>> {
    seq!(_: key, _: " ", value, _: "\n").map(|(value,)| value)
}

/// Helper for creating parsers that parse a multi-line key value pair found in a commit object,
/// such as `gpgsig`, where continuation lines start with a single space.
pub fn multiline_property<'a>(
    mut key: impl Parser<Stream<'a>, &'a [u8], ErrMode<ContextError>>,
) -> impl Parser<Stream<'a>, String, ErrMode<ContextError>> {
    seq!(
        _: key,
        _: " ",
        terminated(take_until(0.., "\n"), "\n")
            .map(str::from_utf8)
            .verify_map(Result::ok),
        repeat(
            0..,
            seq!(_: " ", terminated(take_until(0.., "\n"), "\n"))
                .map(|(line,)| line)
                .map(str::from_utf8)
                .verify_map(Result::ok),
        ),
    )
    .map(|(first_line, continuation_lines): (&str, Vec<&str>)| {
        let mut lines = Vec::with_capacity(1 + continuation_lines.len());
        lines.push(first_line);
        lines.extend(continuation_lines);
        lines.join("\n")
    })
}

/// Parses an arbitrary commit property including its name and value.
pub fn extra_property(input: &mut Stream<'_>) -> ModalResult<CommitProperty> {
    seq!(
        // name
        take_till(1.., (b' ', b'\n'))
            .map(str::from_utf8)
            .verify_map(Result::ok)
            .map(str::to_owned)
            .map(PropertyKey::try_new)
            .verify_map(Result::ok),

        // separator
        _: " ",

        // value
        seq!(
            terminated(take_until(0.., "\n"), "\n")
                .map(str::from_utf8)
                .verify_map(Result::ok),
            repeat(
                0..,
                seq!(_: " ", terminated(take_until(0.., "\n"), "\n"))
                    .map(|(line,)| line)
                    .map(str::from_utf8)
                    .verify_map(Result::ok),
            )
        )
        .map(|(first_line, continuation_lines): (&str, Vec<&str>)| {
            let mut lines = Vec::with_capacity(1 + continuation_lines.len());
            lines.push(first_line);
            lines.extend(continuation_lines);
            lines.join("\n")
        })
        .map(PropertyValue::try_new)
        .verify_map(Result::ok)
    )
    .parse_next(input)
}

/// Parses a UTF-8 encoded file mode such as 100644 from some bytes.
pub fn mode(input: &mut Stream<'_>) -> ModalResult<u16> {
    oct_digit1
        .map(str::from_utf8)
        .verify_map(Result::ok)
        .map(|str| u16::from_str_radix(str, 8))
        .verify_map(Result::ok)
        .context(StrContext::Label("file mode"))
        .parse_next(input)
}

/// Helper to extract a human-readable message and optional cause from winnow's `ContextError`.
pub fn format_context_error(
    err: &ContextError,
) -> (String, Option<Box<dyn std::error::Error + Send + Sync>>) {
    let labels: Vec<&'static str> = err
        .context()
        .filter_map(|c| match c {
            StrContext::Label(l) => Some(*l),
            _ => None,
        })
        .collect();

    let expected: Vec<String> = err
        .context()
        .filter_map(|c| match c {
            StrContext::Expected(e) => Some(e.to_string()),
            _ => None,
        })
        .collect();

    let cause = err
        .cause()
        .map(|c| {
            if let Some(io) = c.downcast_ref::<Error>() {
                if let Some(raw_os) = io.raw_os_error() {
                    Error::from_raw_os_error(raw_os)
                } else {
                    Error::new(io.kind(), io.to_string())
                }
            } else {
                Error::other(c.to_string())
            }
        })
        .map(|x| Box::new(x) as Box<dyn std::error::Error + Send + Sync>);

    let mut message = String::new();

    if let Some(ref c) = cause {
        // we have a cause
        message.push_str(&c.to_string());
        if !labels.is_empty() {
            message.push_str(" while parsing ");
            message.push_str(&labels.join(" in "));
        }
    } else if !labels.is_empty() {
        // we have labels
        message.push_str("invalid ");
        message.push_str(&labels.join(" in "));
        if !expected.is_empty() {
            message.push_str(", expected ");
            message.push_str(&expected.join(", "));
        }
    } else if !expected.is_empty() {
        // we have expected
        message.push_str("expected ");
        message.push_str(&expected.join(", "));
    } else {
        // we have nothing
        message.push_str("syntax error");
    }

    (message, cause)
}

impl ParseError {
    /// Creates a `ParseError` from winnow's `ErrMode<ContextError>` and optional offset.
    pub fn from_winnow_error(
        err: ErrMode<ContextError>,
        context: ParserContext,
        offset: Option<usize>,
    ) -> Self {
        match err {
            ErrMode::Incomplete(needed) => {
                let msg = match needed {
                    Needed::Size(n) => {
                        format!("unexpected end of input, requires {n} more bytes")
                    }
                    Needed::Unknown => "unexpected end of input".to_string(),
                };

                Self {
                    context,
                    offset,
                    message: msg,
                    cause: None,
                }
            }

            ErrMode::Backtrack(c) | ErrMode::Cut(c) => {
                let (message, cause) = format_context_error(&c);

                Self {
                    context,
                    offset,
                    message,
                    cause,
                }
            }
        }
    }

    /// Creates a `ParseError` from winnow's top-level `ParseError<Stream, ContextError>`.
    pub fn from_winnow_parse_error(
        err: winnow::error::ParseError<Stream<'_>, ContextError>,
        context: ParserContext,
    ) -> Self {
        let offset = err.offset();
        let inner = err.into_inner();
        let (message, cause) = format_context_error(&inner);

        Self {
            context,
            offset: Some(offset),
            message,
            cause,
        }
    }
}

pub trait ParseResultExt<T>: Sized {
    fn with_context(self, context: ParserContext) -> Result<T, TinygitError>;
}

impl<T> ParseResultExt<T> for Result<T, winnow::error::ParseError<Stream<'_>, ContextError>> {
    fn with_context(self, context: ParserContext) -> Result<T, TinygitError> {
        self.map_err(|err| ParseError::from_winnow_parse_error(err, context).into())
    }
}

pub trait ParseNextExt<T>: Sized {
    fn context_err(self, context: ParserContext, stream: &Stream<'_>) -> Result<T, TinygitError> {
        self.context_err_at(context, Some(stream.current_token_start()))
    }

    fn context_err_at(
        self,
        context: ParserContext,
        offset: Option<usize>,
    ) -> Result<T, TinygitError>;
}

impl<T> ParseNextExt<T> for Result<T, ErrMode<ContextError>> {
    fn context_err_at(
        self,
        context: ParserContext,
        offset: Option<usize>,
    ) -> Result<T, TinygitError> {
        self.map_err(|err| ParseError::from_winnow_error(err, context, offset).into())
    }
}

#[cfg(test)]
mod tests {
    use super::{Stream, extra_property, mode, multiline_property};
    use crate::object::commit::{PropertyKey, PropertyValue};
    use std::assert_matches;
    use winnow::{Parser, error::ErrMode};

    #[test]
    fn mode_parses_valid_modes() {
        assert_eq!(
            mode.parse_peek(Stream::new(b"000000"))
                .map(|(r, v)| (*r, v)),
            Ok((&b""[..], 0))
        );
        assert_eq!(
            mode.parse_peek(Stream::new(b"100644"))
                .map(|(r, v)| (*r, v)),
            Ok((&b""[..], 0o100644))
        );
    }

    #[test]
    fn multiline_property_parses_valid_signature() {
        let input = b"gpgsig -----BEGIN PGP SIGNATURE-----\n \n wsFcBAABCAAQBQJqc1jACRC1aQ7uu5UhlAAAFfgQACyD2HIkYM5SeaWNsgzpZsVu\n -----END PGP SIGNATURE-----\n \n";
        assert_eq!(
            multiline_property("gpgsig")
                .parse_peek(Stream::new(input))
                .map(|(r, v)| (*r, v)),
            Ok((
                &b""[..],
                "-----BEGIN PGP SIGNATURE-----\n\nwsFcBAABCAAQBQJqc1jACRC1aQ7uu5UhlAAAFfgQACyD2HIkYM5SeaWNsgzpZsVu\n-----END PGP SIGNATURE-----\n".to_string()
            ))
        );
    }

    #[test]
    fn extra_property_parses_single_line_property() {
        let input = b"change-id xnxouqnvmpzvuvkotwynowookslovtno\n\nmessage";
        assert_eq!(
            extra_property
                .parse_peek(Stream::new(input))
                .map(|(r, v)| (*r, v)),
            Ok((
                &b"\nmessage"[..],
                (
                    PropertyKey::try_new("change-id".to_string()).unwrap(),
                    PropertyValue::try_new("xnxouqnvmpzvuvkotwynowookslovtno".to_string()).unwrap()
                )
            ))
        );
    }

    #[test]
    fn extra_property_parses_multi_line_property() {
        let input = b"gpgsig -----BEGIN PGP SIGNATURE-----\n \n wsFcBAABCAAQBQJqc1jACRC1aQ7uu5UhlAAAFfgQACyD2HIkYM5SeaWNsgzpZsVu\n -----END PGP SIGNATURE-----\n \n";
        assert_eq!(
            extra_property
                .parse_peek(Stream::new(input))
                .map(|(r, v)| (*r, v)),
            Ok((
                &b""[..],
                (
                    PropertyKey::try_new("gpgsig".to_string()).unwrap(),
                    PropertyValue::try_new("-----BEGIN PGP SIGNATURE-----\n\nwsFcBAABCAAQBQJqc1jACRC1aQ7uu5UhlAAAFfgQACyD2HIkYM5SeaWNsgzpZsVu\n-----END PGP SIGNATURE-----\n".to_string()).unwrap()
                )
            ))
        );
    }

    #[test]
    fn extra_property_rejects_empty_line() {
        assert_matches!(
            extra_property.parse_peek(Stream::new(b"\n")),
            Err(ErrMode::Backtrack(_))
        );
        assert_matches!(
            extra_property.parse_peek(Stream::new(b"\ncommit message\n")),
            Err(ErrMode::Backtrack(_))
        );
    }

    #[test]
    fn parse_error_snapshot() {
        use crate::error::{ParseError, ParserContext};
        use std::{io, path::PathBuf};
        use winnow::error::{ContextError, FromExternalError, StrContext};

        let io_err = io::Error::new(io::ErrorKind::InvalidInput, "corrupt deflate stream");
        let input: &[u8] = b"";
        let mut ctx_err = ContextError::from_external_error(&input, io_err);
        ctx_err.push(StrContext::Label("undeltified object"));
        ctx_err.push(StrContext::Label("object"));

        let parse_err = ParseError::from_winnow_error(
            ErrMode::Cut(ctx_err),
            ParserContext::File(PathBuf::from(
                "pack-c547e79e77bfc46a9f8024f984014da3cda61c7c.pack",
            )),
            Some(0x1234),
        );

        insta::assert_snapshot!(parse_err.to_string());
    }
}
