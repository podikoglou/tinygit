//! This module deals with the validation of timestamps.
//!
//! It offers the [`Timestamp`] newtime, which models timestamps which are valid in Git.

use chrono::{DateTime, FixedOffset};
use nutype::nutype;
use winnow::{
    ModalResult, Parser,
    ascii::digit1,
    combinator::{alt, seq},
    error::{StrContext, StrContextValue},
    token::take,
};

use crate::{ende::Decode, parsing::Stream};

#[nutype(
    derive(Debug, PartialEq, Eq, Clone, Display, AsRef, Deref),
    validate(predicate = |datetime| datetime.offset().local_minus_utc() % 60 == 0 && datetime.timestamp_subsec_nanos() == 0 ),
)]
pub struct Timestamp(DateTime<FixedOffset>);

impl Timestamp {
    /// Creates a new [`Timestamp`], rounding up any sub-second precision to the next whole second.
    ///
    /// If the timestamp contains sub-second nanoseconds, it rounds up to the next second
    /// and resets nanoseconds to zero before validating with [`Timestamp::try_new`].
    pub fn try_new_rounded_up(datetime: DateTime<FixedOffset>) -> Result<Self, TimestampError> {
        let subsec_nanos = datetime.timestamp_subsec_nanos();

        let datetime = if subsec_nanos > 0 {
            let secs = datetime
                .timestamp()
                .checked_add(1)
                .ok_or(TimestampError::PredicateViolated)?;

            let offset = *datetime.offset();

            DateTime::from_timestamp(secs, 0)
                .ok_or(TimestampError::PredicateViolated)?
                .with_timezone(&offset)
        } else {
            datetime
        };

        Self::try_new(datetime)
    }
}

impl Decode for Timestamp {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        seq!(
            digit1.parse_to::<i64>(),
            _: " ",
            alt((b'+'.value(1), b'-'.value(-1))),
            take(2usize).parse_to::<i32>(),
            take(2usize).parse_to::<i32>(),
        )
        .verify_map(|(secs, sign, hours, minutes)| {
            let offset = FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))?;

            DateTime::from_timestamp(secs, 0).map(|dt| dt.with_timezone(&offset))
        })
        .map(Timestamp::try_new)
        .verify_map(Result::ok)
        .context(StrContext::Label("timestamp"))
        .context(StrContext::Expected(StrContextValue::Description(
            "<unix time> <offset>",
        )))
        .parse_next(input)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, FixedOffset, NaiveDateTime};

    use crate::{
        ende::Decode,
        time::{Timestamp, TimestampError},
    };

    #[test]
    fn timestamp_parses_basic_timestamps() {
        assert_eq!(
            Timestamp::decode_bytes(b"1789057194 +0300").unwrap(),
            Timestamp::try_new(DateTime::<FixedOffset>::from_naive_utc_and_offset(
                #[allow(deprecated)]
                NaiveDateTime::from_timestamp(1789057194, 0),
                FixedOffset::east_opt(3 * 3600).unwrap(),
            ))
            .unwrap()
        );
    }

    #[test]
    fn timestamp_try_new_rejects_subsecond_precision() {
        let offset = FixedOffset::east_opt(3 * 3600).unwrap();
        let dt_with_subseconds = DateTime::from_timestamp(1789057194, 500_000_000)
            .unwrap()
            .with_timezone(&offset);

        assert_eq!(
            Timestamp::try_new(dt_with_subseconds),
            Err(TimestampError::PredicateViolated)
        );
    }

    #[test]
    fn timestamp_try_new_rounded_up_rounds_up_subsecond_precision() {
        let offset = FixedOffset::east_opt(3 * 3600).unwrap();
        let dt_with_subseconds = DateTime::from_timestamp(1789057194, 500_000_000)
            .unwrap()
            .with_timezone(&offset);

        let rounded = Timestamp::try_new_rounded_up(dt_with_subseconds)
            .expect("should succeed by rounding up");

        let expected = Timestamp::try_new(
            DateTime::from_timestamp(1789057195, 0)
                .unwrap()
                .with_timezone(&offset),
        )
        .unwrap();

        assert_eq!(rounded, expected);
        assert_eq!(rounded.timestamp(), 1789057195);
        assert_eq!(rounded.timestamp_subsec_nanos(), 0);

        // Exact seconds should remain unchanged
        let dt_exact = DateTime::from_timestamp(1789057194, 0)
            .unwrap()
            .with_timezone(&offset);

        let rounded_exact =
            Timestamp::try_new_rounded_up(dt_exact).expect("exact seconds should succeed");

        assert_eq!(rounded_exact.timestamp(), 1789057194);
    }
}
