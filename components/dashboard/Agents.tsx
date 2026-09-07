'use client'

import { useState } from 'react'
import { Link } from '@/i18n/navigation'
import { Button } from '@/components/ui/Button'
import { Field } from '@/components/ui/Field'
import { Empty } from '@/components/dashboard/Empty'
import { api, explain } from '@/lib/dashboard-client'
import type { AgentSummary, Workspace } from '@/lib/platform-types'
import { timeAgo } from '@/lib/format'

const STARTING_INSTRUCTIONS =
  'You answer questions from the documents you are given. If the answer is not in them, say so plainly rather than guessing.'

export function Agents({
  initial,
  workspaces,
}: {
  initial: AgentSummary[]
  workspaces: Workspace[]
}) {
  const [agents, setAgents] = useState(initial)
  const [creating, setCreating] = useState(initial.length === 0)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function create(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(null)
    setBusy(true)

    const form = new FormData(event.currentTarget)
    const workspaceId = workspaces[0]?.id

    if (!workspaceId) {
      setError('This organization has no workspace to put it in.')
      setBusy(false)
      return
    }

    try {
      const created = await api.post<AgentSummary>('/agents', {
        workspace_id: workspaceId,
        name: String(form.get('name') ?? '').trim(),
        config: { instructions: STARTING_INSTRUCTIONS },
      })
      setAgents((current) => [created, ...current])
      setCreating(false)
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-8">
      {agents.length === 0 && !creating && (
        <Empty
          title="No agents yet"
          body="An agent is the thing your customers talk to. It reads the knowledge you give it and answers from that — nothing else, unless you tell it otherwise."
        >
          <Button onClick={() => setCreating(true)}>Create one</Button>
        </Empty>
      )}

      {creating && (
        <form
          onSubmit={create}
          className="bg-bg-2 border border-white/[0.06] rounded-lg p-8 max-w-lg"
        >
          <h3 className="font-display text-xl text-white mb-6">New agent</h3>
          <Field
            label="Name"
            name="name"
            required
            placeholder="Admissions assistant"
            hint="What it is for. Only your team sees this."
            error={error}
          />
          <div className="flex gap-3 mt-6">
            <Button type="submit" disabled={busy}>
              {busy ? 'Creating…' : 'Create'}
            </Button>
            {agents.length > 0 && (
              <Button type="button" variant="ghost" onClick={() => setCreating(false)}>
                Cancel
              </Button>
            )}
          </div>
        </form>
      )}

      {agents.length > 0 && (
        <>
          <div className="flex items-center justify-between">
            <h2 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30">
              {agents.length === 1 ? '1 agent' : `${agents.length} agents`}
            </h2>
            {!creating && (
              <Button variant="secondary" size="sm" onClick={() => setCreating(true)}>
                New
              </Button>
            )}
          </div>

          <ul className="space-y-3">
            {agents.map((agent) => (
              <li key={agent.id}>
                <Link
                  href={`/dashboard/agents/${agent.id}`}
                  className="flex items-center justify-between gap-4 bg-bg-2 border border-white/[0.06] rounded-lg px-6 py-5 transition-all duration-300 hover:border-gold-border hover:shadow-gold"
                >
                  <div className="min-w-0">
                    <p className="text-white truncate">{agent.name}</p>
                    <p className="text-xs text-white-30 mt-1">
                      updated {timeAgo(agent.updated_at)}
                    </p>
                  </div>
                  <Status agent={agent} />
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}

/**
 * Published or not, said in those words.
 *
 * The distinction is the one that matters most on this page: an unpublished
 * agent answers nobody. Its status can also be `paused` or `archived`, which
 * are different kinds of not-answering and worth naming separately.
 */
function Status({ agent }: { agent: AgentSummary }) {
  if (agent.status === 'archived') {
    return <span className="text-[10px] tracking-[0.2em] uppercase text-white-30">Archived</span>
  }
  if (agent.status === 'paused') {
    return <span className="text-[10px] tracking-[0.2em] uppercase text-gold">Paused</span>
  }
  if (agent.published) {
    return <span className="text-[10px] tracking-[0.2em] uppercase text-gold">Live</span>
  }
  return <span className="text-[10px] tracking-[0.2em] uppercase text-white-30">Draft</span>
}
