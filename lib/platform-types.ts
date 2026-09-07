/**
 * The shapes the platform's dashboard API sends.
 *
 * Written by hand from the handlers in `platform/crates/api/src/dashboard/`,
 * which is a duplication worth naming: these will drift the first time a field
 * is added and nothing here will notice. The platform publishes an OpenAPI
 * document at `/v1/openapi.json`, but only for the public API — the dashboard
 * API is deliberately not published, because publishing it would mean someone
 * builds against it and it stops being ours to change.
 *
 * When it drifts, the fix is to generate these from the same types rather than
 * to keep re-typing them.
 */

export interface AgentSummary {
  id: string
  workspace_id: string
  name: string
  description: string | null
  /** `draft`, `active`, `paused` or `archived`. */
  status: string
  published: boolean
  updated_at: string
}

export interface KnowledgeBase {
  id: string
  workspace_id: string
  name: string
  description: string | null
  /** Which model built the vectors. A `fake:` prefix means a local stand-in. */
  embedding_model: string
  storage_bytes: number
  document_count: number
}

export interface DocumentSummary {
  id: string
  knowledge_base_id: string
  title: string
  source_type: string
  /** `queued`, `processing`, `chunking`, `embedding`, `indexing`, `ready`, `failed`. */
  status: string
  progress: number
  error_code?: string
  error_message?: string
  version: number
  size_bytes: number
  chunk_count: number
  token_count: number
  language?: string
  created_at: string
  updated_at: string
}

export interface ApiKeySummary {
  id: string
  workspace_id: string
  name: string
  /** The first characters of the key, enough to recognise it in a list. */
  prefix: string
  environment: string
  scopes: string[]
  all_agents: boolean
  status: string
  expires_at: string | null
  last_used_at: string | null
  created_at: string
}

/** The one response that carries a secret, and only once. */
export interface IssuedKey {
  id: string
  name: string
  prefix: string
  environment: string
  secret: string
  warning: string
}

export interface Workspace {
  id: string
  name: string
  slug: string
}

export interface ListOf<T> {
  data: T[]
}

/** Statuses that mean the worker is still busy with a document. */
export const IN_PROGRESS = new Set([
  'queued',
  'processing',
  'chunking',
  'embedding',
  'indexing',
])

export function isInProgress(document: DocumentSummary): boolean {
  return IN_PROGRESS.has(document.status)
}

/**
 * An agent's configuration.
 *
 * The platform's `AgentConfig` has more in it than this — retrieval tuning,
 * model policy, guardrails — and every field of it is `#[serde(default)]`
 * except `instructions`. So a partial config is a valid one, and the dashboard
 * sends only what it lets a customer change. The rest keeps the platform's
 * defaults, which are the considered ones.
 */
export interface AgentConfig {
  instructions: string
  language?: 'auto' | 'th' | 'en'
  behavior?: {
    /** Answer only from retrieved passages. Off means the model may fall back
     * on what it knows, which is how a confident wrong answer happens. */
    strict_knowledge?: boolean
    citations?: boolean
    /** What it says when nothing relevant was found. */
    fallback_message?: string
    history_turns?: number
  }
}

export interface AgentVersion {
  version: number
  created_at: string
  published: boolean
}

export interface AgentDetail extends AgentSummary {
  draft_version: number | null
  published_version: number | null
  draft_config: AgentConfig | null
  published_config: AgentConfig | null
  knowledge_base_ids: string[]
  versions: AgentVersion[]
}

/** What the playground answers with. */
export interface TestAnswer {
  id: string
  conversation_id: string
  answer: string
  grounded: boolean
  used_fallback: boolean
  sources: Source[]
  usage: { input_tokens: number; output_tokens: number }
  model?: { provider: string; name: string }
  latency_ms: number
  retrieval?: {
    embedding_tokens: number
    passages: {
      chunk_id: string
      document_id: string
      score: number
      similarity?: number
      snippet: string
    }[]
  }
}

export interface Source {
  index: number
  document_id: string
  chunk_id: string
  title: string
  page?: number
  url?: string
  snippet: string
  score: number
}
