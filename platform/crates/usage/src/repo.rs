//! Recording what was used, and checking what is left.
//!
//! Two shapes for two questions. Individual records answer "what happened on
//! this request", for support and for a bill a customer can check. A rolled-up
//! counter answers "is this tenant over their allowance", which is asked on
//! every request and cannot be a sum over millions of rows.

use anthovai_core::{OrgId, Result, UsageRecordId};
use anthovai_db::{SystemDb, TenantDb};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::Row;

use crate::{period_start, UsageCounters, UsageKind, UsageRecord};

/// Write the record and move the counter together.
///
/// In the caller's transaction, so a request cannot be billed for work that was
/// rolled back, nor go unbilled for work that was not.
pub async fn record(db: &mut TenantDb<'_>, usage: &UsageRecord) -> Result<UsageRecordId> {
    let id = UsageRecordId::new();
    let tenant = db.tenant_key();

    sqlx::query(
        "INSERT INTO usage_records
           (id, tenant_id, workspace_id, agent_id, api_key_id, request_id, kind,
            provider, model, input_tokens, output_tokens, embedding_tokens,
            latency_ms, cost_usd_micro, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(id.to_db())
    .bind(&tenant)
    .bind(Option::<String>::None)
    .bind(usage.agent_id.map(|a| a.to_db()))
    .bind(usage.api_key_id.map(|k| k.to_db()))
    .bind(usage.request_id.to_db())
    .bind(usage.kind.as_str())
    .bind(&usage.provider)
    .bind(&usage.model)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .bind(usage.embedding_tokens)
    .bind(usage.latency_ms)
    .bind(usage.cost_usd_micro)
    .bind("ok")
    .execute(db.conn())
    .await?;

    bump_counters(db, usage).await?;
    Ok(id)
}

/// Add one request to this month's totals.
///
/// `ON CONFLICT ... DO UPDATE` rather than read-then-write: two requests
/// arriving together would otherwise both read the same number and one of them
/// would go uncounted.
async fn bump_counters(db: &mut TenantDb<'_>, usage: &UsageRecord) -> Result<()> {
    let tenant = db.tenant_key();
    let period = period_start(usage.created_at);
    let messages = i64::from(usage.kind.counts_towards_message_quota());

    sqlx::query(
        "INSERT INTO usage_counters
           (tenant_id, period, messages, input_tokens, output_tokens, cost_usd_micro)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, period) DO UPDATE SET
           messages       = usage_counters.messages + EXCLUDED.messages,
           input_tokens   = usage_counters.input_tokens + EXCLUDED.input_tokens,
           output_tokens  = usage_counters.output_tokens + EXCLUDED.output_tokens,
           cost_usd_micro = usage_counters.cost_usd_micro + EXCLUDED.cost_usd_micro",
    )
    .bind(&tenant)
    .bind(period)
    .bind(messages)
    .bind(i64::from(usage.input_tokens))
    .bind(i64::from(usage.output_tokens))
    .bind(usage.cost_usd_micro)
    .execute(db.conn())
    .await?;

    Ok(())
}

/// This month's totals for the tenant, zero when nothing has been used yet.
pub async fn counters(db: &mut TenantDb<'_>, now: DateTime<Utc>) -> Result<UsageCounters> {
    let tenant = db.tenant_key();
    let row = sqlx::query(
        // `messages` is an INT column; decoding it as i64 would fail, and the
        // failure would look exactly like a tenant who had used nothing.
        "SELECT messages::bigint, input_tokens, output_tokens, cost_usd_micro
         FROM usage_counters WHERE tenant_id = $1 AND period = $2",
    )
    .bind(&tenant)
    .bind(period_start(now))
    .fetch_optional(db.conn())
    .await?;

    Ok(match row {
        None => UsageCounters::default(),
        Some(row) => UsageCounters {
            messages: row.try_get("messages")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            cost_usd_micro: row.try_get("cost_usd_micro")?,
        },
    })
}

/// The same, read on the authentication path before a request is admitted.
pub async fn counters_for(
    db: &mut SystemDb<'_>,
    org_id: OrgId,
    now: DateTime<Utc>,
) -> Result<UsageCounters> {
    let row = sqlx::query(
        // `messages` is an INT column; decoding it as i64 would fail, and the
        // failure would look exactly like a tenant who had used nothing.
        "SELECT messages::bigint, input_tokens, output_tokens, cost_usd_micro
         FROM usage_counters WHERE tenant_id = $1 AND period = $2",
    )
    .bind(org_id.to_db())
    .bind(period_start(now))
    .fetch_optional(db.conn())
    .await?;

    Ok(match row {
        None => UsageCounters::default(),
        Some(row) => UsageCounters {
            messages: row.try_get("messages")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            cost_usd_micro: row.try_get("cost_usd_micro")?,
        },
    })
}

/// One row per day, for the usage chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyUsage {
    pub date: NaiveDate,
    pub messages: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub async fn daily(
    db: &mut TenantDb<'_>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DailyUsage>> {
    let tenant = db.tenant_key();
    let rows = sqlx::query(
        "SELECT date_trunc('day', created_at)::date AS day,
                count(*) FILTER (WHERE kind = 'chat') AS messages,
                coalesce(sum(input_tokens), 0)::bigint  AS input_tokens,
                coalesce(sum(output_tokens), 0)::bigint AS output_tokens
         FROM usage_records
         WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3
         GROUP BY day
         ORDER BY day",
    )
    .bind(&tenant)
    .bind(from)
    .bind(to)
    .fetch_all(db.conn())
    .await?;

    Ok(rows
        .iter()
        .map(|row| DailyUsage {
            date: row.try_get("day").unwrap_or_default(),
            messages: row.try_get("messages").unwrap_or(0),
            input_tokens: row.try_get("input_tokens").unwrap_or(0),
            output_tokens: row.try_get("output_tokens").unwrap_or(0),
        })
        .collect())
}

/// What one kind of work has cost this tenant this month.
pub async fn total_for_kind(
    db: &mut TenantDb<'_>,
    kind: UsageKind,
    now: DateTime<Utc>,
) -> Result<i64> {
    let tenant = db.tenant_key();
    let total: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(cost_usd_micro), 0)::bigint FROM usage_records
         WHERE tenant_id = $1 AND kind = $2 AND created_at >= $3",
    )
    .bind(&tenant)
    .bind(kind.as_str())
    .bind(
        period_start(now)
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc())
            .unwrap_or(now),
    )
    .fetch_one(db.conn())
    .await?;

    Ok(total)
}
