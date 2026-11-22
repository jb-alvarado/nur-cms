use std::{
    fmt,
    ops::{Add, Deref, Sub},
    str::FromStr,
};

use jiff::SignedDuration;
use serde::{Deserialize, Serialize};
use sqlx::{
    Decode, Encode, Postgres, Type,
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef},
};
use ts_rs::{Config, TS};

/// UTC instant shared by SQLx models and JSON responses. PostgreSQL encoding is
/// delegated to Jiff's official adapter; TypeScript continues to see a string.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(jiff::Timestamp);

impl Timestamp {
    pub fn now() -> Self {
        Self(jiff::Timestamp::now())
    }
}

impl Deref for Timestamp {
    type Target = jiff::Timestamp;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<jiff::Timestamp> for Timestamp {
    fn from(value: jiff::Timestamp) -> Self {
        Self(value)
    }
}

impl FromStr for Timestamp {
    type Err = jiff::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Add<SignedDuration> for Timestamp {
    type Output = Self;
    fn add(self, rhs: SignedDuration) -> Self {
        Self(self.0 + rhs)
    }
}

impl Sub<SignedDuration> for Timestamp {
    type Output = Self;
    fn sub(self, rhs: SignedDuration) -> Self {
        Self(self.0 - rhs)
    }
}

impl Sub for Timestamp {
    type Output = SignedDuration;
    fn sub(self, rhs: Self) -> SignedDuration {
        self.0 - rhs.0
    }
}

impl Type<Postgres> for Timestamp {
    fn type_info() -> PgTypeInfo {
        <jiff_sqlx::Timestamp as Type<Postgres>>::type_info()
    }
    fn compatible(ty: &PgTypeInfo) -> bool {
        <jiff_sqlx::Timestamp as Type<Postgres>>::compatible(ty)
    }
}

impl PgHasArrayType for Timestamp {
    fn array_type_info() -> PgTypeInfo {
        <jiff_sqlx::Timestamp as PgHasArrayType>::array_type_info()
    }
}

impl Encode<'_, Postgres> for Timestamp {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <jiff_sqlx::Timestamp as Encode<Postgres>>::encode_by_ref(&self.0.into(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Timestamp {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        <jiff_sqlx::Timestamp as Decode<Postgres>>::decode(value).map(|value| Self(value.to_jiff()))
    }
}

impl TS for Timestamp {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(cfg: &Config) -> String {
        String::name(cfg)
    }
    fn inline(cfg: &Config) -> String {
        String::inline(cfg)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    #[test]
    fn timestamps_remain_utc_strings_and_preserve_subsecond_precision() {
        let timestamp: Timestamp = "2026-11-22T12:34:56.123456789+02:00"
            .parse()
            .expect("timestamp");
        assert_eq!(timestamp.to_string(), "2026-11-22T10:34:56.123456789Z");
        let json = serde_json::to_string(&timestamp).expect("serialize");
        assert_eq!(json, "\"2026-11-22T10:34:56.123456789Z\"");
        assert_eq!(
            serde_json::from_str::<Timestamp>(&json).expect("deserialize"),
            timestamp
        );
        assert_eq!(Timestamp::name(&Config::default()), "string");
        assert_eq!(
            Option::<Timestamp>::inline(&Config::default()),
            "string | null"
        );
        assert!("not-a-date".parse::<Timestamp>().is_err());
    }

    #[sqlx::test]
    async fn postgres_roundtrips_timestamps_arrays_and_nullable_composites(pool: PgPool) {
        for value in [
            "1969-12-31T23:59:59.123456Z",
            "2000-01-01T00:00:00Z",
            "2026-11-22T12:34:56.654321+02:00",
        ] {
            let timestamp: Timestamp = value.parse().expect("timestamp");
            let result: Timestamp = sqlx::query_scalar("SELECT $1::timestamptz")
                .bind(timestamp)
                .fetch_one(&pool)
                .await
                .expect("timestamp roundtrip");
            assert_eq!(result, timestamp);
            let array: Vec<Timestamp> = sqlx::query_scalar("SELECT $1::timestamptz[]")
                .bind(vec![timestamp])
                .fetch_one(&pool)
                .await
                .expect("array roundtrip");
            assert_eq!(array, vec![timestamp]);
            let composite: (Option<Timestamp>, Option<Timestamp>) =
                sqlx::query_scalar("SELECT ROW($1::timestamptz, NULL::timestamptz)")
                    .bind(timestamp)
                    .fetch_one(&pool)
                    .await
                    .expect("entry meta composite");
            assert_eq!(composite, (Some(timestamp), None));
        }
    }
}
