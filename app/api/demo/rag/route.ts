import { NextRequest, NextResponse } from 'next/server'
import { API_URL } from '@/lib/anthovai'
import {
  RAG_DEMO_MAX_MESSAGE,
  RAG_DEMO_RATE_PER_HOUR,
  type RagDemoAnswer,
  type RagDemoSource,
} from '@/lib/rag-demo'

export const maxDuration = 60

const AGENT_ID = process.env.ANTHOVAI_DEMO_AGENT_ID?.trim() ?? ''
const API_KEY = process.env.ANTHOVAI_DEMO_API_KEY?.trim() ?? ''

type Bucket = { count: number; reset: number }
const buckets = new Map<string, Bucket>()

function configured() {
  return Boolean(AGENT_ID && API_KEY)
}

function clientIp(request: NextRequest) {
  const forwarded = request.headers.get('x-forwarded-for')
  const first = forwarded?.split(',')[0]?.trim()
  return first || request.headers.get('x-real-ip') || 'unknown'
}

function allow(ip: string) {
  const now = Date.now()
  const hour = 60 * 60 * 1000
  const existing = buckets.get(ip)
  if (!existing || existing.reset <= now) {
    buckets.set(ip, { count: 1, reset: now + hour })
    if (buckets.size > 5000) {
      for (const [key, bucket] of buckets) {
        if (bucket.reset <= now) buckets.delete(key)
      }
    }
    return true
  }
  if (existing.count >= RAG_DEMO_RATE_PER_HOUR) return false
  existing.count += 1
  return true
}

function jsonError(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status })
}

function readMessage(body: { message?: unknown; conversation_id?: unknown }) {
  const message = typeof body.message === 'string' ? body.message.trim() : ''
  if (!message) return { error: jsonError(400, 'empty_message', 'Message is required') }
  if (message.length > RAG_DEMO_MAX_MESSAGE) {
    return {
      error: jsonError(
        400,
        'message_too_long',
        `Message must be at most ${RAG_DEMO_MAX_MESSAGE} characters`,
      ),
    }
  }
  const conversationId =
    typeof body.conversation_id === 'string' && body.conversation_id.startsWith('cnv_')
      ? body.conversation_id
      : undefined
  return { message, conversationId }
}

function sourcesFrom(parsed: Record<string, unknown>): RagDemoSource[] {
  if (!Array.isArray(parsed.sources)) return []
  const sources: RagDemoSource[] = []
  for (const item of parsed.sources) {
    if (!item || typeof item !== 'object') continue
    const source = item as Record<string, unknown>
    if (typeof source.index !== 'number' || typeof source.title !== 'string') continue
    sources.push({
      index: source.index,
      title: source.title,
      page: typeof source.page === 'number' ? source.page : undefined,
      snippet: typeof source.snippet === 'string' ? source.snippet : '',
    })
  }
  return sources
}

function toDemoAnswer(parsed: Record<string, unknown>): RagDemoAnswer | null {
  if (typeof parsed.answer !== 'string') return null
  const usage =
    parsed.usage && typeof parsed.usage === 'object'
      ? (parsed.usage as { input_tokens?: number; output_tokens?: number })
      : undefined
  const model =
    parsed.model && typeof parsed.model === 'object'
      ? (parsed.model as { name?: string })
      : undefined
  return {
    answer: parsed.answer,
    grounded: parsed.grounded === true,
    conversation_id: typeof parsed.conversation_id === 'string' ? parsed.conversation_id : '',
    sources: sourcesFrom(parsed),
    latency_ms: typeof parsed.latency_ms === 'number' ? parsed.latency_ms : 0,
    usage:
      typeof usage?.input_tokens === 'number' && typeof usage?.output_tokens === 'number'
        ? { input_tokens: usage.input_tokens, output_tokens: usage.output_tokens }
        : undefined,
    model: typeof model?.name === 'string' ? { name: model.name } : undefined,
  }
}

export async function GET() {
  return NextResponse.json({ ok: configured() })
}

export async function POST(request: NextRequest) {
  if (!configured()) {
    return jsonError(503, 'demo_not_configured', 'RAG demo is not configured')
  }
  if (!allow(clientIp(request))) {
    return jsonError(429, 'rate_limited', 'Too many questions from this address')
  }

  let body: { message?: unknown; conversation_id?: unknown }
  try {
    body = await request.json()
  } catch {
    return jsonError(400, 'invalid_json', 'Request body must be JSON')
  }

  const parsedBody = readMessage(body)
  if ('error' in parsedBody) return parsedBody.error

  let upstream: Response
  try {
    upstream = await fetch(`${API_URL}/v1/chat`, {
      method: 'POST',
      headers: {
        authorization: `Bearer ${API_KEY}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        agent_id: AGENT_ID,
        message: parsedBody.message,
        conversation_id: parsedBody.conversationId,
        options: { include_sources: true, include_usage: true },
      }),
      cache: 'no-store',
    })
  } catch {
    return jsonError(503, 'platform_unreachable', 'Could not reach the platform')
  }

  let parsed: Record<string, unknown> | null = null
  try {
    const raw = await upstream.text()
    parsed = raw ? (JSON.parse(raw) as Record<string, unknown>) : null
  } catch {
    parsed = null
  }

  if (!upstream.ok) {
    const err = parsed?.error as { code?: string; message?: string } | undefined
    return jsonError(
      upstream.status,
      err?.code ?? 'chat_failed',
      err?.message ?? 'The question could not be answered',
    )
  }

  const answer = parsed ? toDemoAnswer(parsed) : null
  if (!answer) return jsonError(502, 'bad_upstream', 'Unexpected response from the platform')
  return NextResponse.json(answer)
}
