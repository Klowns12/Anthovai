# 06 — Rust Workspace / Crate Architecture

Toolchain: Rust stable (2024 edition), MSRV = stable ล่าสุด − 1
Async runtime: `tokio` · HTTP: `axum` 0.8 · DB: `sqlx` (postgres, runtime-tokio, tls-rustls) + `pgvector` crate · HTTP client: `reqwest` (rustls) · Serialization: `serde`, `serde_json` · Errors: `thiserror` (library) / `anyhow` (binaries) · Tracing: `tracing`, `tracing-subscriber`, `opentelemetry` (P2) · Config: `figment` หรือ `config` + env · IDs: `ulid` · Validation: `validator` · OpenAPI: `utoipa`

## 1. Monorepo Layout

```
anthovai-ai/
├── Cargo.toml                 # [workspace] members, shared deps ([workspace.dependencies])
├── rust-toolchain.toml
├── .cargo/config.toml
├── apps/
│   ├── api/                   # binary: anthovai-api
│   ├── worker/                # binary: anthovai-worker
│   └── dashboard/             # Next.js (ไม่ใช่ Rust)
├── crates/
│   ├── core/                  # types กลาง, ids, errors, TenantCtx, clock, config structs
│   ├── db/                    # sqlx pool, TenantDb wrapper, migrations runner, repositories
│   ├── auth/                  # password hashing, sessions, api key hashing/verification, RBAC
│   ├── tenant/                # organizations, workspaces, memberships, plans & limits
│   ├── agent/                 # Agent aggregate, AgentConfig schema+validation, versions
│   ├── knowledge/             # KnowledgeBase, Document aggregate, status machine, storage keys
│   ├── ingestion/             # parsers, normalizer, chunkers, ingest pipeline (ใช้โดย worker)
│   ├── embeddings/            # EmbeddingProvider trait + batching + cache-by-hash
│   ├── retrieval/             # hybrid search, RRF, MMR, ContextBuilder
│   ├── inference/             # ChatProvider trait, ModelRouter, policies, circuit breaker, model registry
│   ├── providers/
│   │   ├── openai/            # impl ChatProvider + EmbeddingProvider
│   │   └── anthropic/         # impl ChatProvider (Messages API)
│   ├── conversation/          # conversations, messages, history window
│   ├── rag/                   # orchestration: ChatService (ประกอบ retrieval + inference + conversation + usage)
│   ├── usage/                 # usage records, counters, quota checks, cost calc
│   ├── storage/               # ObjectStorage trait + S3 impl (aws-sdk-s3 / opendal)
│   ├── jobs/                  # Job queue (PG) + JobHandler trait + scheduler
│   ├── guardrails/            # input/output checks
│   ├── api/                   # axum routers, extractors, DTOs, error mapping, OpenAPI (ใช้โดย apps/api)
│   └── testkit/               # test fixtures, testcontainers PG, fake providers
├── migrations/                # sqlx migrations
├── config/
│   ├── default.toml
│   ├── models.toml            # model registry (tiers, prices, context)
│   └── plans.toml             # plan limits (P1 hard-config, P4 → DB)
├── docs/
├── docker/
│   ├── docker-compose.yml
│   └── Dockerfile.api / Dockerfile.worker
└── scripts/
```

## 2. Dependency Rules (บังคับด้วย `cargo-deny` + review)

```
apps/api ──▶ api ──▶ rag, agent, knowledge, tenant, auth, usage, conversation, jobs(enqueue), storage
apps/worker ─▶ jobs, ingestion, embeddings, knowledge, storage, usage
rag ──▶ retrieval, inference, conversation, agent, guardrails, usage
retrieval ─▶ embeddings, db
inference ─▶ providers/* (ผ่าน trait object registry เท่านั้น)
ทุก crate ─▶ core, db (ถ้าต้อง)
```
- **ห้าม**: `providers/*` import อะไรจาก `agent`/`knowledge`; `core` import จาก crate อื่นในระบบ; crate domain import `axum`
- HTTP types (axum, DTO) อยู่ใน `api` เท่านั้น; domain crates ใช้ struct ของตัวเอง แล้ว `api` ทำ mapping
- `sqlx` queries อยู่ใน `db` (repositories) หรือใน crate domain ผ่าน `TenantDb`; ห้ามใน `api`

## 3. Core Types (crate `core`)

```rust
// ids
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrgId(Ulid);        // serialize เป็น "org_01J..."
pub struct WorkspaceId(Ulid);  // "ws_"
pub struct AgentId(Ulid);      // "agt_"
pub struct KnowledgeBaseId(Ulid);
pub struct DocumentId(Ulid);
pub struct ChunkId(Ulid);
pub struct ApiKeyId(Ulid);
pub struct ConversationId(Ulid);
pub struct MessageId(Ulid);
pub struct RequestId(Ulid);
// macro `typed_id!(AgentId, "agt")` สร้าง Display/FromStr/sqlx::Type

// tenant context — ทุก service fn รับตัวนี้เป็น arg แรก
#[derive(Clone, Debug)]
pub struct TenantCtx {
    pub org_id: OrgId,
    pub workspace_id: Option<WorkspaceId>,
    pub actor: Actor,                 // User{user_id, role} | ApiKey{key_id, scopes, agent_scope} | System
    pub plan: Plan,
    pub request_id: RequestId,
}

pub enum Actor {
    User { user_id: UserId, role: Role },
    ApiKey { key_id: ApiKeyId, scopes: Scopes, agents: AgentScope },   // AgentScope::All | Only(Vec<AgentId>)
    System,
}

// errors
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("not found: {0}")] NotFound(&'static str),
    #[error("forbidden: {0}")] Forbidden(&'static str),
    #[error("validation: {0}")] Validation(String),
    #[error("conflict: {0}")] Conflict(&'static str),
    #[error("quota exceeded: {0}")] QuotaExceeded(&'static str),
    #[error("provider unavailable")] ProviderUnavailable,
    #[error(transparent)] Db(#[from] sqlx::Error),
    #[error(transparent)] Other(#[from] anyhow::Error),
}
```

## 4. Database Access (crate `db`)

```rust
pub struct Db(PgPool);

/// Connection ที่ตั้ง app.tenant_id แล้ว — วิธีเดียวที่ domain code จะแตะ DB
pub struct TenantDb<'a> { tx: Transaction<'a, Postgres>, tenant: OrgId }

impl Db {
    pub async fn tenant(&self, ctx: &TenantCtx) -> Result<TenantDb<'_>> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
            .bind(ctx.org_id.to_string()).execute(&mut *tx).await?;
        Ok(TenantDb { tx, tenant: ctx.org_id })
    }
    /// สำหรับ auth lookup / worker cross-tenant — role system, ใช้ให้น้อยที่สุด
    pub fn system(&self) -> SystemDb<'_>;
}
```
- Repository functions รับ `&mut TenantDb` และ **ยังคง** ใส่ `WHERE tenant_id = $1` เอง (RLS เป็นชั้นที่สอง)
- ใช้ `sqlx::query_as!` (compile-time checked) กับ `SQLX_OFFLINE=true` + `.sqlx/` committed

## 5. Provider Abstraction (crate `inference`)

```rust
#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> ProviderId;                       // "openai" | "anthropic"
    fn capabilities(&self) -> ProviderCapabilities;   // streaming, vision, tools, max_context, prompt_cache
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatEvent, ProviderError>>, ProviderError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {            // อยู่ใน crate `embeddings`
    fn model_id(&self) -> &str;                       // "openai:text-embedding-3-small"
    fn dimension(&self) -> usize;
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ProviderError>;
}

pub struct ChatRequest {
    pub model: String,                 // provider-specific model name จาก registry
    pub system: String,
    pub messages: Vec<ChatMessage>,    // role: User|Assistant
    pub max_tokens: u32,
    pub reasoning: ReasoningLevel,     // Fast|Balanced|Deep → provider map เอง (เช่น Anthropic effort low/medium/high)
    pub stop: Vec<String>,
    pub metadata: RequestMeta,         // request_id, tenant hash สำหรับ provider-side abuse tracking
}

pub struct ChatResponse { pub text: String, pub finish: FinishReason, pub usage: TokenUsage, pub model: String, pub raw_id: Option<String> }
pub enum ChatEvent { Start{model}, TextDelta(String), Usage(TokenUsage), Done(FinishReason) }
pub enum FinishReason { Stop, Length, ContentFilter, Refusal, Other(String) }

#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("rate limited")] RateLimited { retry_after: Option<Duration> },
    #[error("provider 5xx")] Upstream { status: u16, body: String },
    #[error("timeout")] Timeout,
    #[error("bad request: {0}")] BadRequest(String),   // ไม่ retry
    #[error("auth")] Auth,
    #[error(transparent)] Transport(#[from] reqwest::Error),
}
impl ProviderError { pub fn is_retryable(&self) -> bool { matches!(self, RateLimited{..}|Upstream{..}|Timeout|Transport(_)) } }
```

### 5.1 Model Router
```rust
pub struct ModelRegistry { models: Vec<ModelSpec> }  // โหลดจาก config/models.toml
pub struct ModelSpec { id, provider: ProviderId, name: String, tier: Tier, max_context: u32, in_price_micro: u64, out_price_micro: u64, enabled: bool }

pub struct ModelRouter { registry, providers: HashMap<ProviderId, Arc<dyn ChatProvider>>, health: HealthTracker }

impl ModelRouter {
    /// คืน ordered list ของ candidates ตาม policy (primary ก่อน fallback)
    pub fn plan(&self, policy: &ModelPolicy, hints: &RoutingHints) -> Vec<&ModelSpec>;
    /// เรียกทีละ candidate จนสำเร็จ; บันทึก health; คืน (response, spec, was_fallback)
    pub async fn chat(&self, policy: &ModelPolicy, hints: &RoutingHints, req: ChatRequestTemplate) -> Result<(ChatResponse, &ModelSpec, bool), DomainError>;
    pub async fn chat_stream(...) -> ...;
}
```
- `HealthTracker`: circuit breaker ต่อ `(provider, model)`; state Closed/Open/HalfOpen
- Retry policy: 1 retry ภายใน candidate เดียวถ้า `is_retryable()` แล้วค่อยไป candidate ถัดไป
- `RoutingHints { reasoning, context_tokens, language, needs_streaming }`

### 5.2 Anthropic Provider (crate `providers/anthropic`)
- Endpoint `POST https://api.anthropic.com/v1/messages`, headers `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
- Body: `{ model, max_tokens, system, messages:[{role, content}], stream, thinking:{type:"adaptive"}, output_config:{effort} }` — `effort` map จาก `ReasoningLevel` (Fast→`low`, Balanced→`medium`, Deep→`high`)
- Streaming: parse SSE events `message_start`, `content_block_delta` (`text_delta`), `message_delta` (usage.output_tokens, stop_reason), `message_stop`; ข้าม `thinking` blocks
- Map `stop_reason`: `end_turn`→Stop, `max_tokens`→Length, `refusal`→Refusal
- Usage: `usage.input_tokens`, `usage.output_tokens`, `usage.cache_read_input_tokens`
- Prompt caching (P6): ใส่ `cache_control: {type:"ephemeral"}` ที่ท้าย system block ส่วน instructions (ก่อน knowledge)

### 5.3 OpenAI Provider (crate `providers/openai`)
- Chat: `POST https://api.openai.com/v1/chat/completions` (หรือ Responses API — ตัดสินใจตอน implement ให้เลือกอันที่ stable) headers `Authorization: Bearer`
- Embeddings: `POST /v1/embeddings` `{model, input:[...], dimensions?}`; map `usage.prompt_tokens` → `embedding_tokens`
- Streaming: SSE `data: {...choices[0].delta.content}`, จบด้วย `data: [DONE]`; ขอ `stream_options: {include_usage: true}`
- Map `finish_reason`: `stop`, `length`, `content_filter`

### 5.4 Fake Provider (crate `testkit`)
- `FakeChatProvider` ตอบ deterministic จาก prompt (echo chunk ids) ใช้ใน integration tests; `FakeEmbeddingProvider` ทำ hash-based vectors

## 6. RAG Orchestration (crate `rag`)

```rust
pub struct ChatService { db, retriever, router, embeddings, conversations, usage, guardrails, clock }

impl ChatService {
    pub async fn chat(&self, ctx: &TenantCtx, input: ChatInput) -> Result<ChatOutput, DomainError> {
        let agent = self.agents.load_published(ctx, input.agent_id).await?;          // ตรวจ AgentScope ที่นี่
        self.usage.check_quota(ctx).await?;
        let conv = self.conversations.get_or_create(ctx, &agent, input.conversation_id, &input.user).await?;
        let history = self.conversations.window(ctx, &conv, agent.config.behavior.history_turns).await?;
        let query = self.guardrails.check_input(&input.message)?;
        let retrieved = self.retriever.retrieve(ctx, &agent, &query, &input.filters).await?;   // hybrid + RRF + MMR
        if retrieved.is_empty() && agent.config.behavior.strict_knowledge {
            return Ok(self.fallback_response(ctx, &agent, &conv, &input).await?);
        }
        let prompt = PromptBuilder::new(&agent, &retrieved, &history).build(&input.message);
        let (resp, spec, fallback) = self.router.chat(&agent.model_policy(), &hints(&prompt), prompt.into()).await?;
        let out = PostProcessor::new(&retrieved).finalize(resp.text);                // citations, grounded
        self.persist_async(ctx, &conv, &input, &out, &resp.usage, spec, fallback);   // tokio::spawn + retry
        Ok(out)
    }
    pub async fn chat_stream(...) -> impl Stream<Item = ChatStreamEvent>;
}
```

## 7. Ingestion (crate `ingestion`) & Worker (apps/worker)

```rust
pub trait Parser: Send + Sync { fn supports(&self, source: &SourceType) -> bool; async fn parse(&self, input: ParseInput) -> Result<ParsedDocument>; }
pub struct ParsedDocument { pub title: Option<String>, pub language: Option<String>, pub blocks: Vec<Block> } // Block::Heading{level,text} | Paragraph{text,page} | Record{key,fields}
pub trait Chunker: Send + Sync { fn chunk(&self, doc: &ParsedDocument, cfg: &ChunkConfig) -> Vec<ChunkDraft>; }
pub struct IngestPipeline { storage, parsers, chunker, embeddings, chunk_repo, doc_repo }
impl IngestPipeline { pub async fn run(&self, job: IngestDocumentJob) -> Result<()>; }  // อัปเดต status ทุกขั้น
```

```rust
// crate jobs
#[async_trait] pub trait JobHandler: Send + Sync { fn kind(&self) -> &'static str; async fn handle(&self, job: Job) -> Result<(), JobError>; }
pub struct JobQueue { db }   // enqueue(), fetch_batch(worker_id, n) ด้วย FOR UPDATE SKIP LOCKED, complete(), fail(with backoff)
pub struct WorkerRuntime { queue, handlers: HashMap<&'static str, Arc<dyn JobHandler>>, concurrency: usize }
```
- Worker main loop: poll ทุก 500ms เมื่อว่าง, `LISTEN jobs_new` (PG NOTIFY) เพื่อ wake เร็ว
- Graceful shutdown: SIGTERM → หยุด fetch → รอ job ที่ค้างจบ (timeout 60s) → ปล่อย lock

## 8. API Layer (crate `api`)

- Router แยก `public_router()` (`/v1`), `dashboard_router()` (`/dashboard/v1`), `internal_router()`
- **Extractors**: `ApiKeyAuth` → `TenantCtx`; `SessionAuth` + `X-Org-Id` → `TenantCtx`; ทั้งสองผ่าน `FromRequestParts`
- Middleware stack (tower): request_id → tracing span → timeout(65s) → body limit (json 1MB, upload ตาม plan) → cors (dashboard origin) → rate limit → auth
- Error mapping: `DomainError` → `ApiError { status, type, code, message }` ที่เดียว (`impl IntoResponse`)
- DTO ทั้งหมด `#[derive(Serialize, Deserialize, ToSchema)]` เพื่อ generate OpenAPI

## 9. Configuration

```toml
# config/default.toml (override ด้วย env ANTHOVAI__SECTION__KEY)
[server]   host="0.0.0.0"  port=8080  request_timeout_secs=65
[database] url="postgres://..."  max_connections=20
[storage]  provider="s3" bucket="anthovai-dev" endpoint="http://minio:9000" region="auto"
[providers.openai]    api_key_env="OPENAI_API_KEY"    base_url="https://api.openai.com/v1"
[providers.anthropic] api_key_env="ANTHROPIC_API_KEY" base_url="https://api.anthropic.com"
[embeddings] default_model="openai:text-embedding-3-small" batch_size=64 concurrency=4
[retrieval]  vector_top=30 keyword_top=30 rrf_k=60
[auth]       session_ttl_hours=168  api_key_cache_secs=60  argon2_memory_kib=65536
[worker]     concurrency=4 poll_interval_ms=500
```
Secrets (`*_API_KEY`, `DATABASE_URL`, `SESSION_SECRET`) มาจาก env เท่านั้น

## 10. Observability

- `tracing` span ต่อ request มี fields: `request_id, tenant_id, agent_id, route`; ห้าม log message content ของลูกค้าที่ level ≥ INFO (DEBUG ได้เฉพาะ dev)
- Metrics (`metrics` crate + Prometheus exporter): `http_requests_total{route,status}`, `http_request_duration_seconds`, `provider_requests_total{provider,model,outcome}`, `provider_latency_seconds`, `retrieval_duration_seconds`, `jobs_pending`, `jobs_failed_total`, `usage_tokens_total{tenant?}` (tenant label เฉพาะ internal)
- Health: `/internal/health` ตรวจ DB ping, storage head bucket, provider circuit state

## 11. Testing Strategy

| ระดับ | เครื่องมือ | ครอบคลุม |
|-------|-----------|----------|
| Unit | `cargo test` | chunker, RRF/MMR, prompt builder, citation parser, id codec, policy planner |
| Integration (DB) | `testcontainers` PG+pgvector, `sqlx::test` | repositories, RLS, tenant isolation, job queue |
| Integration (HTTP) | axum `tower::ServiceExt::oneshot` + fake providers | ทุก endpoint P1, auth paths, error format |
| Contract | snapshot `openapi.json` | API surface ไม่เปลี่ยนโดยไม่ตั้งใจ |
| Provider live | `#[ignore]` tests รัน manual/nightly ด้วย key จริง | OpenAI/Anthropic request/response shape |
| Cross-tenant | test บังคับใน CI | สร้าง 2 org, upload ทั้งคู่, ถามด้วย key ของ A ต้องไม่ได้ chunk ของ B เลย |

## 12. CI/CD
- GitHub Actions: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo deny check`, `cargo test --workspace`, `sqlx migrate` dry-run, build images
- Release: tag → build `api`/`worker` images (distroless) → deploy staging → smoke test → prod

## 13. Coding Conventions
- ไม่ `unwrap()` นอก tests; `expect()` เฉพาะ invariant ที่อธิบายในข้อความ
- Public fn ของ domain crate รับ `&TenantCtx` เป็น arg แรกเสมอ (ยกเว้น auth/system)
- ทุก enum ที่ลง DB มี `impl Display/FromStr` + test round-trip
- Feature flags ผ่าน config ไม่ใช่ cfg features (ยกเว้น provider compile-out)
