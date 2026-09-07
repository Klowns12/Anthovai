import type { Metadata } from 'next'
import { fromPlatform } from '@/lib/session'
import type { KnowledgeBase, ListOf, Workspace } from '@/lib/platform-types'
import { KnowledgeBases } from '@/components/dashboard/KnowledgeBases'

export const metadata: Metadata = { title: 'Knowledge' }

/**
 * What an agent is allowed to know.
 *
 * A knowledge base is a set of documents embedded by one model — which is why
 * the model is shown rather than hidden: a base built by a stand-in answers
 * questions perfectly happily and means nothing by the answers, and that is not
 * something to discover from a customer complaint.
 */
export default async function KnowledgePage() {
  const [bases, workspaces] = await Promise.all([
    fromPlatform<ListOf<KnowledgeBase>>('/knowledge_bases'),
    fromPlatform<ListOf<Workspace>>('/workspaces'),
  ])

  return (
    <KnowledgeBases initial={bases.data} workspaces={workspaces.data} />
  )
}
