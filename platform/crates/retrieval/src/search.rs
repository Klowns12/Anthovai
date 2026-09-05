//! The two searches, in SQL.
//!
//! Both bind `tenant_id` from the transaction and restrict to the knowledge
//! bases the agent is allowed to read. Neither takes a tenant id as an argument,
//! so there is no parameter a caller can get wrong — and row-level security is
//! behind them either way.

use anthovai_core::{DomainError, KnowledgeBaseId, Result};
use anthovai_db::TenantDb;
use pgvector::Vector;
use sqlx::Row;

use crate::fusion::Candidate;

/// How hard PostgreSQL looks through the HNSW graph. Higher finds more of the
/// true nearest neighbours and costs more; 40 is comfortably above the default
/// of 40/ef_construction trade-off for the recall a chat answer needs.
const HNSW_EF_SEARCH: i32 = 40;

/// Which chunks a search may consider, beyond the tenant and the agent's
/// knowledge bases.
#[derive(Clone, Debug, Default)]
pub struct SearchFilters {
    /// Narrow to particular documents, when a caller asks for it.
    pub document_ids: Vec<String>,
}

impl SearchFilters {
    fn document_filter(&self) -> Option<&[String]> {
        (!self.document_ids.is_empty()).then_some(self.document_ids.as_slice())
    }
}

/// Nearest neighbours by cosine distance.
///
/// `1 - (embedding <=> query)` turns pgvector's distance into a similarity, so
/// the number that reaches the relevance threshold reads the way people expect:
/// 1.0 is identical, 0.0 is unrelated.
pub async fn vector_search(
    db: &mut TenantDb<'_>,
    knowledge_base_ids: &[KnowledgeBaseId],
    query_vector: &[f32],
    filters: &SearchFilters,
    limit: i64,
) -> Result<Vec<Candidate>> {
    if knowledge_base_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Per-transaction, so one expensive search cannot change how every other
    // connection behaves.
    sqlx::query(&format!("SET LOCAL hnsw.ef_search = {HNSW_EF_SEARCH}"))
        .execute(db.conn())
        .await?;

    let tenant = db.tenant_key();
    let kb_ids: Vec<String> = knowledge_base_ids.iter().map(|k| k.to_db()).collect();
    let documents = filters.document_filter().map(<[String]>::to_vec);

    let rows = sqlx::query(
        "SELECT id, document_id, content, token_count, metadata,
                1 - (embedding <=> $3) AS similarity
         FROM document_chunks
         WHERE tenant_id = $1
           AND knowledge_base_id = ANY($2)
           AND deleted_at IS NULL
           AND ($5::text[] IS NULL OR document_id = ANY($5))
         ORDER BY embedding <=> $3
         LIMIT $4",
    )
    .bind(&tenant)
    .bind(&kb_ids)
    .bind(Vector::from(query_vector.to_vec()))
    .bind(limit)
    .bind(documents)
    .fetch_all(db.conn())
    .await?;

    rows.iter().map(|row| candidate(row, true)).collect()
}

/// Literal matches, for the terms a vector search glides past: a course code, a
/// person's name, a fee.
///
/// The `simple` dictionary splits on whitespace, which serves European
/// languages and does almost nothing for Thai — see the note in
/// `docs/spec-v0.1/03-rag-flow.md`. Vector search carries Thai in P1.
pub async fn keyword_search(
    db: &mut TenantDb<'_>,
    knowledge_base_ids: &[KnowledgeBaseId],
    query: &str,
    filters: &SearchFilters,
    limit: i64,
) -> Result<Vec<Candidate>> {
    if knowledge_base_ids.is_empty() || query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let tenant = db.tenant_key();
    let kb_ids: Vec<String> = knowledge_base_ids.iter().map(|k| k.to_db()).collect();
    let documents = filters.document_filter().map(<[String]>::to_vec);

    let rows = sqlx::query(
        "SELECT id, document_id, content, token_count, metadata,
                ts_rank_cd(tsv, plainto_tsquery('simple', $3)) AS similarity
         FROM document_chunks
         WHERE tenant_id = $1
           AND knowledge_base_id = ANY($2)
           AND deleted_at IS NULL
           AND ($5::text[] IS NULL OR document_id = ANY($5))
           AND tsv @@ plainto_tsquery('simple', $3)
         ORDER BY similarity DESC
         LIMIT $4",
    )
    .bind(&tenant)
    .bind(&kb_ids)
    .bind(query)
    .bind(limit)
    .bind(documents)
    .fetch_all(db.conn())
    .await?;

    // Keyword scores are on their own scale and are not comparable with cosine
    // similarity, so they are not carried as a relevance score — fusion ranks
    // by position, which is what makes mixing the two lists sound.
    rows.iter().map(|row| candidate(row, false)).collect()
}

fn candidate(row: &sqlx::postgres::PgRow, from_vector: bool) -> Result<Candidate> {
    let chunk_id: String = row.try_get("id").map_err(sql)?;
    let document_id: String = row.try_get("document_id").map_err(sql)?;
    let content: String = row.try_get("content").map_err(sql)?;
    let token_count: i32 = row.try_get("token_count").map_err(sql)?;
    let metadata: serde_json::Value = row.try_get("metadata").map_err(sql)?;

    let similarity: Option<f32> = if from_vector {
        Some(row.try_get::<f64, _>("similarity").map_err(sql)? as f32)
    } else {
        None
    };

    Ok(Candidate {
        chunk_id: with_prefix("chk", &chunk_id),
        document_id: with_prefix("doc", &document_id),
        content,
        token_count: token_count.max(0) as u32,
        vector_score: similarity,
        score: 0.0,
        // Left empty: diversification needs vectors, and fetching 1536 floats
        // per candidate to compare a handful of them costs more than it saves.
        // `rank` falls back to relevance order without them.
        embedding: Vec::new(),
        metadata,
    })
}

/// Ids are stored bare and presented with their prefix.
fn with_prefix(prefix: &str, id: &str) -> String {
    format!("{prefix}_{id}")
}

fn sql(err: sqlx::Error) -> DomainError {
    DomainError::Database(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_document_filter_means_no_restriction() {
        assert!(SearchFilters::default().document_filter().is_none());
    }

    #[test]
    fn a_document_filter_is_passed_through() {
        let filters = SearchFilters {
            document_ids: vec!["01ABC".into()],
        };
        assert_eq!(filters.document_filter().unwrap().len(), 1);
    }

    #[test]
    fn ids_are_returned_in_their_public_form() {
        assert_eq!(with_prefix("chk", "01ABC"), "chk_01ABC");
    }
}
