//! Typed, prefixed ULID identifiers.
//!
//! Wire format is `<prefix>_<26-char ULID>` (e.g. `agt_01J8Z...`). The database
//! stores only the bare 26-character ULID; the prefix is added when serialising.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ulid::Ulid;

/// Error returned when a typed id cannot be parsed from a string.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("expected prefix `{expected}_`, got `{got}`")]
    WrongPrefix { expected: &'static str, got: String },
    #[error("invalid ULID: {0}")]
    InvalidUlid(String),
}

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Ulid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            /// Generate a new id from the current time.
            pub fn new() -> Self {
                Self(Ulid::new())
            }

            /// The bare 26-character ULID, as stored in the database.
            pub fn to_db(self) -> String {
                self.0.to_string()
            }

            /// Rebuild from the bare ULID stored in the database.
            pub fn from_db(s: &str) -> Result<Self, IdError> {
                Ulid::from_string(s)
                    .map(Self)
                    .map_err(|_| IdError::InvalidUlid(s.to_owned()))
            }

            pub fn timestamp_ms(self) -> u64 {
                self.0.timestamp_ms()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({}_{})", stringify!($name), $prefix, self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let rest =
                    s.strip_prefix(concat!($prefix, "_"))
                        .ok_or_else(|| IdError::WrongPrefix {
                            expected: $prefix,
                            got: s.to_owned(),
                        })?;
                Self::from_db(rest)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        #[cfg(feature = "sqlx")]
        impl sqlx::Type<sqlx::Postgres> for $name {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                <String as sqlx::Type<sqlx::Postgres>>::type_info()
            }
        }

        #[cfg(feature = "sqlx")]
        impl sqlx::Encode<'_, sqlx::Postgres> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut sqlx::postgres::PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <String as sqlx::Encode<sqlx::Postgres>>::encode(self.to_db(), buf)
            }
        }

        #[cfg(feature = "sqlx")]
        impl sqlx::Decode<'_, sqlx::Postgres> for $name {
            fn decode(
                value: sqlx::postgres::PgValueRef<'_>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let s = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
                Ok(Self::from_db(s)?)
            }
        }
    };
}

typed_id!(UserId, "usr");
typed_id!(OrgId, "org");
typed_id!(WorkspaceId, "ws");
typed_id!(AgentId, "agt");
typed_id!(AgentVersionId, "agtv");
typed_id!(KnowledgeBaseId, "kb");
typed_id!(DocumentId, "doc");
typed_id!(ChunkId, "chk");
typed_id!(ApiKeyId, "key");
typed_id!(ConversationId, "conv");
typed_id!(MessageId, "msg");
typed_id!(RequestId, "req");
typed_id!(JobId, "job");
typed_id!(UsageRecordId, "use");
typed_id!(AuditLogId, "aud");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        let id = AgentId::new();
        let text = id.to_string();
        assert!(text.starts_with("agt_"));
        assert_eq!(text.parse::<AgentId>().unwrap(), id);
    }

    #[test]
    fn round_trips_through_db_form() {
        let id = DocumentId::new();
        let db = id.to_db();
        assert_eq!(db.len(), 26);
        assert_eq!(DocumentId::from_db(&db).unwrap(), id);
    }

    #[test]
    fn rejects_a_foreign_prefix() {
        let agent = AgentId::new().to_string();
        let err = agent.parse::<DocumentId>().unwrap_err();
        assert!(matches!(
            err,
            IdError::WrongPrefix {
                expected: "doc",
                ..
            }
        ));
    }

    #[test]
    fn rejects_a_malformed_ulid() {
        assert!("agt_not-a-ulid".parse::<AgentId>().is_err());
    }

    #[test]
    fn serde_uses_the_prefixed_form() {
        let id = OrgId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<OrgId>(&json).unwrap(), id);
    }
}
