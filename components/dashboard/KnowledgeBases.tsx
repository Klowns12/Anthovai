'use client'

import { useState } from 'react'
import { Link } from '@/i18n/navigation'
import { Button } from '@/components/ui/Button'
import { Field } from '@/components/ui/Field'
import { Empty } from '@/components/dashboard/Empty'
import { api, explain } from '@/lib/dashboard-client'
import type { KnowledgeBase, Workspace } from '@/lib/platform-types'
import { formatBytes } from '@/lib/format'

interface Props {
  initial: KnowledgeBase[]
  workspaces: Workspace[]
}

export function KnowledgeBases({ initial, workspaces }: Props) {
  const [bases, setBases] = useState(initial)
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
      const created = await api.post<KnowledgeBase>('/knowledge_bases', {
        workspace_id: workspaceId,
        name: String(form.get('name') ?? '').trim(),
      })
      setBases((current) => [...current, created])
      setCreating(false)
      event.currentTarget.reset()
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-8">
      {bases.length === 0 && !creating && (
        <Empty
          title="Nothing to answer from yet"
          body="A knowledge base holds the documents an agent reads. Most customers start with one — a handbook, a price list, a set of policies — and add to it as they go."
        >
          <Button onClick={() => setCreating(true)}>Create one</Button>
        </Empty>
      )}

      {creating && (
        <form
          onSubmit={create}
          className="bg-bg-2 border border-white/[0.06] rounded-lg p-8 max-w-lg"
        >
          <h3 className="font-display text-xl text-white mb-6">
            New knowledge base
          </h3>
          <Field
            label="Name"
            name="name"
            required
            placeholder="Student handbook"
            hint="What is in it, in a few words. Only your team sees this."
            error={error}
          />
          <div className="flex gap-3 mt-6">
            <Button type="submit" disabled={busy}>
              {busy ? 'Creating…' : 'Create'}
            </Button>
            {bases.length > 0 && (
              <Button
                type="button"
                variant="ghost"
                onClick={() => {
                  setCreating(false)
                  setError(null)
                }}
              >
                Cancel
              </Button>
            )}
          </div>
        </form>
      )}

      {bases.length > 0 && (
        <>
          <div className="flex items-center justify-between">
            <h2 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30">
              {bases.length === 1 ? '1 knowledge base' : `${bases.length} knowledge bases`}
            </h2>
            {!creating && (
              <Button variant="secondary" size="sm" onClick={() => setCreating(true)}>
                New
              </Button>
            )}
          </div>

          <ul className="grid gap-4 md:grid-cols-2">
            {bases.map((base) => (
              <li key={base.id}>
                <Link
                  href={`/dashboard/knowledge/${base.id}`}
                  className="block bg-bg-2 border border-white/[0.06] rounded-lg p-6 transition-all duration-300 hover:border-gold-border hover:shadow-gold"
                >
                  <h3 className="font-display text-xl text-white">{base.name}</h3>
                  <p className="text-sm text-white-60 mt-2">
                    {base.document_count === 1
                      ? '1 document'
                      : `${base.document_count} documents`}
                    {base.storage_bytes > 0 && ` · ${formatBytes(base.storage_bytes)}`}
                  </p>
                  <p className="font-mono text-[11px] text-white-30 mt-4">
                    {base.embedding_model}
                    {base.embedding_model.startsWith('fake:') && (
                      // Said plainly rather than left to be discovered: these
                      // vectors encode word overlap, not meaning, and the
                      // answers built on them are worth nothing.
                      <span className="text-gold ml-2">
                        — a stand-in, not a real model
                      </span>
                    )}
                  </p>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}
