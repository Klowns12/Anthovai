import type { Metadata } from 'next'
import { notFound } from 'next/navigation'
import { Link } from '@/i18n/navigation'
import { fromPlatform } from '@/lib/session'
import { ApiError } from '@/lib/anthovai'
import type { AgentDetail, KnowledgeBase, ListOf } from '@/lib/platform-types'
import { AgentEditor } from '@/components/dashboard/AgentEditor'

export const metadata: Metadata = { title: 'Agent' }

type Props = { params: Promise<{ id: string }> }

export default async function AgentPage({ params }: Props) {
  const { id } = await params

  let agent: AgentDetail
  let bases: ListOf<KnowledgeBase>

  try {
    ;[agent, bases] = await Promise.all([
      fromPlatform<AgentDetail>(`/agents/${id}`),
      fromPlatform<ListOf<KnowledgeBase>>('/knowledge_bases'),
    ])
  } catch (error) {
    if (error instanceof ApiError && error.status === 404) notFound()
    throw error
  }

  return (
    <div className="space-y-8">
      <div>
        <Link
          href="/dashboard/agents"
          className="text-xs tracking-[0.2em] uppercase text-white-30 hover:text-gold transition-colors"
        >
          ← Agents
        </Link>
        <h2 className="font-display text-3xl text-white mt-3">{agent.name}</h2>
      </div>

      <AgentEditor agent={agent} bases={bases.data} />
    </div>
  )
}
