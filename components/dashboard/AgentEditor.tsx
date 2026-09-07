'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/Button'
import { Playground } from '@/components/dashboard/Playground'
import { api, explain } from '@/lib/dashboard-client'
import type { AgentConfig, AgentDetail, KnowledgeBase } from '@/lib/platform-types'

/**
 * Editing an agent, which is always editing its *draft*.
 *
 * Nothing here reaches a customer until it is published. That is the platform's
 * design and this page leans on it: the playground runs the draft, so an
 * instruction can be changed and tried and changed again while the live version
 * carries on answering the version it was given.
 */
export function AgentEditor({
  agent,
  bases,
}: {
  agent: AgentDetail
  bases: KnowledgeBase[]
}) {
  const draft: AgentConfig = agent.draft_config ??
    agent.published_config ?? { instructions: '' }

  const [instructions, setInstructions] = useState(draft.instructions ?? '')
  const [strict, setStrict] = useState(draft.behavior?.strict_knowledge ?? true)
  const [fallback, setFallback] = useState(
    draft.behavior?.fallback_message ?? ''
  )
  const [attached, setAttached] = useState<string[]>(agent.knowledge_base_ids)
  const [publishedVersion, setPublishedVersion] = useState(agent.published_version)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [publishing, setPublishing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  function change<T>(setter: (value: T) => void) {
    return (value: T) => {
      setter(value)
      setDirty(true)
      setSaved(false)
    }
  }

  async function save() {
    setError(null)
    setSaving(true)
    try {
      await api.patch(`/agents/${agent.id}`, {
        config: {
          instructions,
          behavior: {
            strict_knowledge: strict,
            ...(fallback ? { fallback_message: fallback } : {}),
          },
        },
      })
      setDirty(false)
      setSaved(true)
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setSaving(false)
    }
  }

  async function toggleBase(id: string) {
    const next = attached.includes(id)
      ? attached.filter((value) => value !== id)
      : [...attached, id]

    const previous = attached
    setAttached(next)
    setError(null)

    try {
      await api.put(`/agents/${agent.id}/knowledge_bases`, {
        knowledge_base_ids: next,
      })
    } catch (failure) {
      setAttached(previous)
      setError(explain(failure))
    }
  }

  async function publish() {
    setError(null)
    setPublishing(true)
    try {
      // Saving first, so publish never quietly ships the previous draft while
      // the editor on screen shows something newer.
      if (dirty) await save()
      // Publishing answers with the whole agent, the same shape the page was
      // rendered from — so the version below is the platform's own answer
      // rather than an assumption about what it did.
      const published = await api.post<AgentDetail>(`/agents/${agent.id}/publish`)
      setPublishedVersion(published.published_version)
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setPublishing(false)
    }
  }

  const noKnowledge = attached.length === 0

  return (
    <div className="grid gap-8 lg:grid-cols-[1fr_420px] items-start">
      <div className="space-y-8">
        <section className="bg-bg-2 border border-white/[0.06] rounded-lg p-8">
          <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-5">
            Instructions
          </h3>
          <textarea
            value={instructions}
            onChange={(event) => change(setInstructions)(event.target.value)}
            rows={7}
            className="w-full bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border resize-y leading-relaxed"
            placeholder="Tell it who it is talking to and how to answer."
          />
          <p className="text-xs text-white-30 mt-3">
            The knowledge is added automatically — this is about tone, audience
            and what to do when it is unsure.
          </p>
        </section>

        <section className="bg-bg-2 border border-white/[0.06] rounded-lg p-8">
          <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-5">
            Knowledge
          </h3>

          {bases.length === 0 ? (
            <p className="text-white-60">
              No knowledge bases yet. An agent with nothing to read cannot
              answer anything.
            </p>
          ) : (
            <ul className="space-y-2">
              {bases.map((base) => {
                const on = attached.includes(base.id)
                return (
                  <li key={base.id}>
                    <label className="flex items-center gap-3 cursor-pointer py-2">
                      <input
                        type="checkbox"
                        checked={on}
                        onChange={() => toggleBase(base.id)}
                        className="accent-gold w-4 h-4"
                      />
                      <span className={on ? 'text-white' : 'text-white-60'}>
                        {base.name}
                      </span>
                      <span className="text-xs text-white-30">
                        {base.document_count === 1
                          ? '1 document'
                          : `${base.document_count} documents`}
                      </span>
                    </label>
                  </li>
                )
              })}
            </ul>
          )}
        </section>

        <section className="bg-bg-2 border border-white/[0.06] rounded-lg p-8">
          <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-5">
            When it does not know
          </h3>

          <label className="flex items-start gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={strict}
              onChange={(event) => change(setStrict)(event.target.checked)}
              className="accent-gold w-4 h-4 mt-1"
            />
            <span>
              <span className="text-white">Answer only from the knowledge</span>
              <span className="block text-sm text-white-60 mt-1 leading-relaxed">
                With this off, the model may answer from what it happens to know
                — which is how a confident wrong answer reaches a customer.
              </span>
            </span>
          </label>

          <div className="mt-6">
            <label
              className="block text-[11px] font-medium tracking-[0.2em] uppercase text-white-60 mb-3"
              htmlFor="fallback"
            >
              What it says instead
            </label>
            <input
              id="fallback"
              value={fallback}
              onChange={(event) => change(setFallback)(event.target.value)}
              placeholder="ขออภัย ฉันไม่มีข้อมูลเรื่องนี้"
              className="w-full bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border"
            />
            <p className="text-xs text-white-30 mt-2">
              Leave empty to keep the default.
            </p>
          </div>
        </section>

        {error && <p className="text-sm text-gold">{error}</p>}

        <div className="flex flex-wrap items-center gap-4">
          <Button onClick={save} variant="secondary" disabled={saving || !dirty}>
            {saving ? 'Saving…' : dirty ? 'Save draft' : saved ? 'Saved' : 'Saved'}
          </Button>
          <Button onClick={publish} disabled={publishing || noKnowledge}>
            {publishing ? 'Publishing…' : 'Publish'}
          </Button>
          <p className="text-xs text-white-30">
            {publishedVersion
              ? `Live: version ${publishedVersion}`
              : 'Not published — nobody can call this agent yet'}
            {noKnowledge && ' · attach a knowledge base first'}
          </p>
        </div>
      </div>

      <Playground agentId={agent.id} disabled={noKnowledge} />
    </div>
  )
}
