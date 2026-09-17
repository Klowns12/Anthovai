/**
 * The public RAG demo talks to the platform through `/api/demo/rag`.
 * The test key and agent id stay on the server — never `NEXT_PUBLIC_`.
 */

export const RAG_DEMO_MAX_MESSAGE = 500
export const RAG_DEMO_RATE_PER_HOUR = 20

export const RAG_DEMO_QUESTIONS = [
  'q_scale',
  'q_hours',
  'q_gold',
  'q_cancel',
  'q_offtopic',
] as const

export type RagDemoQuestion = (typeof RAG_DEMO_QUESTIONS)[number]

export interface RagDemoSource {
  index: number
  title: string
  page?: number
  snippet: string
}

export interface RagDemoAnswer {
  answer: string
  grounded: boolean
  conversation_id: string
  sources: RagDemoSource[]
  latency_ms: number
  usage?: { input_tokens: number; output_tokens: number }
  model?: { name: string }
}
