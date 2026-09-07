import type { Metadata } from 'next'
import { fromPlatform } from '@/lib/session'
import type { AgentSummary, ListOf, Workspace } from '@/lib/platform-types'
import { Agents } from '@/components/dashboard/Agents'

export const metadata: Metadata = { title: 'Agents' }

export default async function AgentsPage() {
  const [agents, workspaces] = await Promise.all([
    fromPlatform<ListOf<AgentSummary>>('/agents'),
    fromPlatform<ListOf<Workspace>>('/workspaces'),
  ])

  return <Agents initial={agents.data} workspaces={workspaces.data} />
}
