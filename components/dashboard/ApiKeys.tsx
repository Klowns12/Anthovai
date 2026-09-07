'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/Button'
import { Field } from '@/components/ui/Field'
import { Empty } from '@/components/dashboard/Empty'
import { api, explain } from '@/lib/dashboard-client'
import type {
  AgentSummary,
  ApiKeySummary,
  IssuedKey,
  Workspace,
} from '@/lib/platform-types'
import { timeAgo } from '@/lib/format'

/**
 * The scopes a key can carry, and what each one lets the holder do.
 *
 * Named in the platform's own vocabulary rather than translated into something
 * friendlier, because these strings appear in error messages the customer's
 * developer will read: a `scope_missing` refusal names the scope, and it should
 * be the same word they ticked here.
 */
const SCOPES = [
  { value: 'chat', label: 'chat', detail: 'Ask questions. This is the one an integration needs.' },
  { value: 'agents:read', label: 'agents:read', detail: 'List agents and see which are live.' },
  { value: 'knowledge:read', label: 'knowledge:read', detail: 'List knowledge bases and documents.' },
  { value: 'knowledge:write', label: 'knowledge:write', detail: 'Upload and remove documents — for a nightly sync.' },
  { value: 'usage:read', label: 'usage:read', detail: 'Read this month’s usage and remaining allowance.' },
] as const

export function ApiKeys({
  initial,
  workspaces,
  agents,
  emailVerified,
}: {
  initial: ApiKeySummary[]
  workspaces: Workspace[]
  agents: AgentSummary[]
  emailVerified: boolean
}) {
  const [keys, setKeys] = useState(initial)
  const [creating, setCreating] = useState(false)
  const [issued, setIssued] = useState<IssuedKey | null>(null)
  const [scopes, setScopes] = useState<string[]>(['chat'])
  const [allAgents, setAllAgents] = useState(true)
  const [agentIds, setAgentIds] = useState<string[]>([])
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
      const key = await api.post<IssuedKey>('/api_keys', {
        workspace_id: workspaceId,
        name: String(form.get('name') ?? '').trim(),
        scopes,
        all_agents: allAgents,
        ...(allAgents ? {} : { agent_ids: agentIds }),
      })

      setIssued(key)
      setCreating(false)

      // The list needs the summary shape, which the create response is not —
      // it carries the secret instead. Re-reading is simpler than guessing at
      // the fields it left out.
      const refreshed = await api.get<{ data: ApiKeySummary[] }>('/api_keys')
      setKeys(refreshed.data)
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setBusy(false)
    }
  }

  async function revoke(id: string) {
    setError(null)
    try {
      await api.post(`/api_keys/${id}/revoke`)
      const refreshed = await api.get<{ data: ApiKeySummary[] }>('/api_keys')
      setKeys(refreshed.data)
    } catch (failure) {
      setError(explain(failure))
    }
  }

  function toggleScope(value: string) {
    setScopes((current) =>
      current.includes(value)
        ? current.filter((scope) => scope !== value)
        : [...current, value]
    )
  }

  return (
    <div className="space-y-8">
      {issued && <IssuedKeyPanel issued={issued} onDismiss={() => setIssued(null)} />}

      {!emailVerified && (
        <div className="bg-gold-dim border border-gold-border rounded-lg p-6">
          <p className="text-white">Confirm your email address first</p>
          <p className="text-sm text-white-60 mt-2 leading-relaxed">
            A live key can call your agents and spend your allowance, so the
            platform will not issue one until the address on the account has
            been confirmed.
          </p>
        </div>
      )}

      {keys.length === 0 && !creating && !issued && (
        <Empty
          title="No keys yet"
          body="A key is what your own website, app or LMS sends with each question. It belongs to this organization and can be limited to particular agents."
        >
          <Button onClick={() => setCreating(true)} disabled={!emailVerified}>
            Create a key
          </Button>
        </Empty>
      )}

      {creating && (
        <form
          onSubmit={create}
          className="bg-bg-2 border border-white/[0.06] rounded-lg p-8 max-w-2xl space-y-8"
        >
          <h3 className="font-display text-xl text-white">New API key</h3>

          <Field
            label="Name"
            name="name"
            required
            placeholder="Production website"
            hint="Where this key will be used. It is how you will recognise it later."
            error={error}
          />

          <div>
            <p className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-60 mb-4">
              What it may do
            </p>
            <ul className="space-y-3">
              {SCOPES.map((scope) => (
                <li key={scope.value}>
                  <label className="flex items-start gap-3 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={scopes.includes(scope.value)}
                      onChange={() => toggleScope(scope.value)}
                      className="accent-gold w-4 h-4 mt-1"
                    />
                    <span>
                      <span className="font-mono text-sm text-white">{scope.label}</span>
                      <span className="block text-sm text-white-60 mt-0.5">
                        {scope.detail}
                      </span>
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          </div>

          {agents.length > 0 && (
            <div>
              <p className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-60 mb-4">
                Which agents
              </p>
              <label className="flex items-center gap-3 cursor-pointer mb-3">
                <input
                  type="radio"
                  checked={allAgents}
                  onChange={() => setAllAgents(true)}
                  className="accent-gold w-4 h-4"
                />
                <span className="text-white">Every agent, including future ones</span>
              </label>
              <label className="flex items-center gap-3 cursor-pointer">
                <input
                  type="radio"
                  checked={!allAgents}
                  onChange={() => setAllAgents(false)}
                  className="accent-gold w-4 h-4"
                />
                <span className="text-white">Only the ones I choose</span>
              </label>

              {!allAgents && (
                <ul className="mt-4 ml-7 space-y-2">
                  {agents.map((agent) => (
                    <li key={agent.id}>
                      <label className="flex items-center gap-3 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={agentIds.includes(agent.id)}
                          onChange={() =>
                            setAgentIds((current) =>
                              current.includes(agent.id)
                                ? current.filter((id) => id !== agent.id)
                                : [...current, agent.id]
                            )
                          }
                          className="accent-gold w-4 h-4"
                        />
                        <span className="text-white-60">{agent.name}</span>
                      </label>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}

          <div className="flex gap-3">
            <Button type="submit" disabled={busy || scopes.length === 0}>
              {busy ? 'Creating…' : 'Create key'}
            </Button>
            <Button type="button" variant="ghost" onClick={() => setCreating(false)}>
              Cancel
            </Button>
          </div>
        </form>
      )}

      {error && !creating && <p className="text-sm text-gold">{error}</p>}

      {keys.length > 0 && (
        <>
          <div className="flex items-center justify-between">
            <h2 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30">
              {keys.length === 1 ? '1 key' : `${keys.length} keys`}
            </h2>
            {!creating && (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setCreating(true)}
                disabled={!emailVerified}
              >
                New
              </Button>
            )}
          </div>

          <ul className="space-y-3">
            {keys.map((key) => (
              <li
                key={key.id}
                className="flex items-start justify-between gap-4 bg-bg-2 border border-white/[0.06] rounded-lg px-6 py-5"
              >
                <div className="min-w-0">
                  <p className="text-white">{key.name}</p>
                  <p className="font-mono text-sm text-white-30 mt-1">
                    {key.prefix}…
                  </p>
                  <p className="text-xs text-white-30 mt-2">
                    {key.scopes.join(' · ')}
                    {!key.all_agents && ' · limited to selected agents'}
                  </p>
                  <p className="text-xs text-white-30 mt-1">
                    {key.last_used_at
                      ? `last used ${timeAgo(key.last_used_at)}`
                      : 'never used'}
                  </p>
                </div>

                {key.status === 'active' ? (
                  <button
                    onClick={() => revoke(key.id)}
                    className="text-xs tracking-[0.15em] uppercase text-white-30 hover:text-gold transition-colors shrink-0"
                  >
                    Revoke
                  </button>
                ) : (
                  <span className="text-[10px] tracking-[0.2em] uppercase text-white-30 shrink-0">
                    {key.status}
                  </span>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}

/**
 * The one screen that shows a secret.
 *
 * The platform stores only a hash, so this is genuinely the last time anyone
 * can read it — which is worth saying in those words rather than with a vague
 * warning icon.
 */
function IssuedKeyPanel({
  issued,
  onDismiss,
}: {
  issued: IssuedKey
  onDismiss: () => void
}) {
  const [copied, setCopied] = useState(false)

  async function copy() {
    try {
      await navigator.clipboard.writeText(issued.secret)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Clipboard access can be refused. The key is on screen and selectable,
      // so there is nothing to recover from — only a button that did nothing.
    }
  }

  return (
    <div className="bg-gold-dim border border-gold-border rounded-lg p-8">
      <h3 className="font-display text-xl text-white mb-2">{issued.name}</h3>
      <p className="text-sm text-white-60 mb-6">
        Copy this now. It is stored only as a hash, so this is the last time it
        can be shown.
      </p>

      <div className="flex items-center gap-3 flex-wrap">
        <code className="font-mono text-sm text-white bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 break-all flex-1 min-w-0">
          {issued.secret}
        </code>
        <Button size="sm" onClick={copy}>
          {copied ? 'Copied' : 'Copy'}
        </Button>
      </div>

      <button
        onClick={onDismiss}
        className="text-xs tracking-[0.15em] uppercase text-white-30 hover:text-white-60 transition-colors mt-6"
      >
        I have saved it
      </button>
    </div>
  )
}
