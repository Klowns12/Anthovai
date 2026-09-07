import type { Metadata } from 'next'
import { currentUser, fromPlatform } from '@/lib/session'
import type { AgentSummary, ApiKeySummary, ListOf, Workspace } from '@/lib/platform-types'
import { ApiKeys } from '@/components/dashboard/ApiKeys'

export const metadata: Metadata = { title: 'API keys' }

export default async function KeysPage() {
  const [keys, workspaces, agents, account] = await Promise.all([
    fromPlatform<ListOf<ApiKeySummary>>('/api_keys'),
    fromPlatform<ListOf<Workspace>>('/workspaces'),
    fromPlatform<ListOf<AgentSummary>>('/agents'),
    currentUser(),
  ])

  return (
    <ApiKeys
      initial={keys.data}
      workspaces={workspaces.data}
      agents={agents.data}
      // The platform refuses to mint a live key for an unconfirmed address.
      // Better to say so before the form than after it.
      emailVerified={account?.user.email_verified ?? false}
    />
  )
}
