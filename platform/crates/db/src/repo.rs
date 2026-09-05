//! Helpers shared by the repositories in the domain crates.
//!
//! Ids are stored as the bare 26-character ULID; the prefixed form
//! (`agt_01J…`) exists only on the wire. These helpers convert one column at a
//! time so a malformed row becomes a clear internal error rather than a panic.

use anthovai_core::{DomainError, Result};
use sqlx::Row;

/// Read a typed id from a column.
pub fn id<T, R>(row: &R, column: &str) -> Result<T>
where
    R: Row,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    T: FromDb,
{
    let raw: String = row
        .try_get(column)
        .map_err(|e| DomainError::Internal(e.into()))?;
    T::from_db_str(&raw).map_err(|_| {
        DomainError::Internal(anyhow::anyhow!("column `{column}` holds a malformed id"))
    })
}

/// Read an optional typed id from a nullable column.
pub fn opt_id<T, R>(row: &R, column: &str) -> Result<Option<T>>
where
    R: Row,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    T: FromDb,
{
    let raw: Option<String> = row
        .try_get(column)
        .map_err(|e| DomainError::Internal(e.into()))?;
    match raw {
        None => Ok(None),
        Some(value) => T::from_db_str(&value).map(Some).map_err(|_| {
            DomainError::Internal(anyhow::anyhow!("column `{column}` holds a malformed id"))
        }),
    }
}

/// Read a column whose text form parses into a domain enum.
pub fn parsed<T, R>(row: &R, column: &str) -> Result<T>
where
    R: Row,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    T: std::str::FromStr<Err = DomainError>,
{
    let raw: String = row
        .try_get(column)
        .map_err(|e| DomainError::Internal(e.into()))?;
    raw.parse().map_err(|e: DomainError| {
        DomainError::Internal(anyhow::anyhow!("column `{column}` is invalid: {e}"))
    })
}

/// Implemented by every typed id, so the helpers above stay generic.
pub trait FromDb: Sized {
    fn from_db_str(raw: &str) -> std::result::Result<Self, anthovai_core::ids::IdError>;
}

macro_rules! impl_from_db {
    ($($ty:ty),* $(,)?) => {
        $(impl FromDb for $ty {
            fn from_db_str(raw: &str) -> std::result::Result<Self, anthovai_core::ids::IdError> {
                <$ty>::from_db(raw)
            }
        })*
    };
}

impl_from_db!(
    anthovai_core::UserId,
    anthovai_core::OrgId,
    anthovai_core::WorkspaceId,
    anthovai_core::AgentId,
    anthovai_core::AgentVersionId,
    anthovai_core::KnowledgeBaseId,
    anthovai_core::DocumentId,
    anthovai_core::ChunkId,
    anthovai_core::ApiKeyId,
    anthovai_core::ConversationId,
    anthovai_core::MessageId,
    anthovai_core::RequestId,
    anthovai_core::JobId,
    anthovai_core::UsageRecordId,
    anthovai_core::AuditLogId,
);
