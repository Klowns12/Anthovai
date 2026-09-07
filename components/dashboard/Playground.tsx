'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/Button'
import { api, explain } from '@/lib/dashboard-client'
import type { TestAnswer } from '@/lib/platform-types'

/**
 * Asking the draft a question.
 *
 * This is the endpoint the platform built for exactly this: it runs the draft
 * rather than the published version, it is not billed against the message
 * quota, and it comes back with the passages it retrieved and what they scored.
 *
 * Those scores are the point. When an answer is wrong the useful question is
 * almost never "what did the model say" — it is "what did it read", and this is
 * the only place that shows it.
 */
export function Playground({
  agentId,
  disabled,
}: {
  agentId: string
  disabled: boolean
}) {
  const [question, setQuestion] = useState('')
  const [answer, setAnswer] = useState<TestAnswer | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)
  const [showPassages, setShowPassages] = useState(false)

  async function ask(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!question.trim()) return

    setError(null)
    setAsking(true)

    try {
      const result = await api.post<TestAnswer>(`/agents/${agentId}/test`, {
        message: question,
        debug: true,
      })
      setAnswer(result)
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setAsking(false)
    }
  }

  return (
    <aside className="bg-bg-2 border border-white/[0.06] rounded-lg p-8 lg:sticky lg:top-32">
      <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-2">
        Playground
      </h3>
      <p className="text-sm text-white-60 mb-6 leading-relaxed">
        Runs the draft, not the live version. Nothing here counts against your
        allowance.
      </p>

      <form onSubmit={ask}>
        <textarea
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          rows={3}
          disabled={disabled}
          placeholder={
            disabled
              ? 'Attach a knowledge base first'
              : 'Ask something your documents can answer'
          }
          className="w-full bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border resize-y disabled:opacity-50"
        />
        <Button
          type="submit"
          size="sm"
          className="mt-4 w-full"
          disabled={disabled || asking || !question.trim()}
        >
          {asking ? 'Thinking…' : 'Ask'}
        </Button>
      </form>

      {error && <p className="text-sm text-gold mt-4">{error}</p>}

      {answer && (
        <div className="mt-8 pt-8 border-t border-white/[0.06] space-y-5">
          <p className="text-white leading-relaxed whitespace-pre-wrap">
            {answer.answer}
          </p>

          {!answer.grounded && (
            // Not a failure — it is the agent doing what it was told. Worth
            // saying, because an empty-handed answer with no explanation looks
            // like something went wrong.
            <p className="text-sm text-gold">
              Nothing relevant was found, so it used your fallback message
              instead of answering.
            </p>
          )}

          {answer.sources.length > 0 && (
            <div>
              <p className="text-[10px] tracking-[0.2em] uppercase text-white-30 mb-3">
                Answered from
              </p>
              <ul className="space-y-3">
                {answer.sources.map((source) => (
                  <li key={source.chunk_id} className="text-sm">
                    <span className="text-gold font-mono">[{source.index}]</span>{' '}
                    <span className="text-white-60">{source.title}</span>
                    {source.page && (
                      <span className="text-white-30"> · page {source.page}</span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}

          <p className="text-xs text-white-30">
            {answer.usage.input_tokens} in · {answer.usage.output_tokens} out ·{' '}
            {answer.latency_ms} ms
            {answer.model && ` · ${answer.model.name}`}
          </p>

          {answer.retrieval && answer.retrieval.passages.length > 0 && (
            <div>
              <button
                onClick={() => setShowPassages((value) => !value)}
                className="text-xs tracking-[0.15em] uppercase text-white-30 hover:text-gold transition-colors"
              >
                {showPassages ? 'Hide' : 'Show'} what it read (
                {answer.retrieval.passages.length})
              </button>

              {showPassages && (
                <ul className="mt-4 space-y-4">
                  {answer.retrieval.passages.map((passage, index) => (
                    <li
                      key={passage.chunk_id}
                      className="bg-bg-3 rounded-md p-4 text-xs"
                    >
                      <p className="text-white-30 font-mono mb-2">
                        {/*
                          Similarity first, because it is the only one of the
                          two that means anything on its own: how close this
                          passage is to the question, and the number the
                          relevance floor is applied to.

                          `score` is reciprocal-rank fusion, which is derived
                          from position rather than closeness — the top hit
                          scores 1/61 whether it was a perfect match or a
                          desperate one. Leading with it, as this did, invites
                          a customer to read 0.016 as "barely relevant" and go
                          looking for a problem that is not there.
                        */}
                        {passage.similarity !== undefined
                          ? `similarity ${passage.similarity.toFixed(3)}`
                          : `rank score ${passage.score.toFixed(3)}`}
                        {passage.similarity !== undefined &&
                          ` · rank ${index + 1} of ${answer.retrieval!.passages.length}`}
                      </p>
                      <p className="text-white-60 leading-relaxed whitespace-pre-wrap">
                        {passage.snippet}
                      </p>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      )}
    </aside>
  )
}
