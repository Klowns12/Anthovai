'use client'

import { FormEvent, useCallback, useEffect, useState } from 'react'
import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'
import { RAG_DEMO_QUESTIONS, type RagDemoAnswer } from '@/lib/rag-demo'

export function RagDemo() {
  const t = useTranslations('demo.rag')
  const [privacy, setPrivacy] = useState(t('privacy_checking'))
  const [privacyError, setPrivacyError] = useState(false)
  const [question, setQuestion] = useState('')
  const [conversationId, setConversationId] = useState<string | undefined>()
  const [answer, setAnswer] = useState<RagDemoAnswer | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const health = await (await fetch('/api/demo/rag', { cache: 'no-store' })).json()
        if (cancelled) return
        if (health?.ok) {
          setPrivacy(t('privacy'))
          setPrivacyError(false)
        } else {
          setPrivacy(t('privacy_down'))
          setPrivacyError(true)
        }
      } catch {
        if (!cancelled) {
          setPrivacy(t('privacy_down'))
          setPrivacyError(true)
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [t])

  const ask = useCallback(
    async (text: string) => {
      const message = text.trim()
      if (!message || asking) return
      setAsking(true)
      setError(null)
      setAnswer(null)
      setQuestion(message)
      try {
        const response = await fetch('/api/demo/rag', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ message, conversation_id: conversationId }),
        })
        const body = await response.json()
        if (!response.ok) {
          const detail =
            typeof body?.error?.message === 'string'
              ? body.error.message
              : JSON.stringify(body?.error ?? body)
          setError(t('fail', { status: response.status, detail }))
          return
        }
        setAnswer(body as RagDemoAnswer)
        if (typeof body.conversation_id === 'string' && body.conversation_id) {
          setConversationId(body.conversation_id)
        }
      } catch (cause) {
        setError(t('down', { message: cause instanceof Error ? cause.message : '' }))
      } finally {
        setAsking(false)
      }
    },
    [asking, conversationId, t],
  )

  async function onSubmit(event: FormEvent) {
    event.preventDefault()
    await ask(question)
  }

  return (
    <section className="py-16 relative overflow-hidden">
      <div className="absolute inset-0 dot-grid opacity-20 pointer-events-none" />
      <div className="mx-auto max-w-5xl px-6 lg:px-8 relative z-10">
        <FadeUp>
          <div className="flex flex-wrap items-baseline justify-between gap-3 mb-6">
            <div className="flex items-center gap-3">
              <span className="w-8 h-[1px] bg-gold" />
              <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                {t('label')}
              </p>
            </div>
            <p className={`text-sm ${privacyError ? 'text-[#B3402E]' : 'text-gold'}`}>{privacy}</p>
          </div>

          <h1 className="font-display text-[clamp(32px,5vw,52px)] leading-[1.12] text-white">
            {t('headline')}
          </h1>
          <p className="text-white-60 text-lg leading-relaxed max-w-2xl mt-6">{t('lede')}</p>
        </FadeUp>

        <form onSubmit={onSubmit} className="mt-10">
          <textarea
            value={question}
            onChange={(event) => setQuestion(event.target.value)}
            rows={3}
            disabled={asking || privacyError}
            placeholder={t('placeholder')}
            className="w-full bg-bg-2 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border resize-y disabled:opacity-50"
          />
          <button
            type="submit"
            disabled={asking || privacyError || !question.trim()}
            className="mt-4 bg-gold text-bg font-medium text-sm tracking-wide px-7 py-3.5 rounded-md hover:bg-gold-light transition-colors disabled:opacity-50"
          >
            {asking ? t('asking') : t('ask')}
          </button>
        </form>

        <p className="mt-6 text-sm text-white-60">
          {t('try')}{' '}
          {RAG_DEMO_QUESTIONS.map((key) => (
            <button
              key={key}
              type="button"
              disabled={asking || privacyError}
              onClick={() => void ask(t(key))}
              className="mr-2 mt-2 rounded border border-white/[0.12] px-3 py-1.5 text-sm text-white hover:border-gold hover:text-gold disabled:opacity-50"
            >
              {t(key)}
            </button>
          ))}
        </p>

        {(asking || error) && (
          <div className="mt-8">
            {asking && !error && (
              <div className="h-[3px] overflow-hidden rounded bg-white/[0.08]">
                <i className="block h-full w-1/3 bg-gold animate-pulse" />
              </div>
            )}
            <p className={`mt-3 text-sm ${error ? 'text-[#B3402E]' : 'text-white-60'}`}>
              {error ?? t('asking')}
            </p>
          </div>
        )}

        {answer && (
          <div className="mt-10 space-y-6">
            <div className="rounded-lg border border-white/[0.08] bg-bg-2 p-5">
              <p className="text-white leading-relaxed whitespace-pre-wrap">{answer.answer}</p>
              {!answer.grounded && (
                <p className="text-sm text-gold mt-4 mb-0">{t('ungrounded')}</p>
              )}
            </div>

            {answer.sources.length > 0 && (
              <div className="bg-bg-2 border border-white/[0.08] rounded-lg overflow-hidden">
                <h3 className="m-0 px-5 py-3 text-[13px] tracking-[0.08em] border-b border-white/[0.08] text-white-60 font-semibold">
                  {t('sources')}
                </h3>
                <ul className="px-5 py-4 space-y-4">
                  {answer.sources.map((source) => (
                    <li key={`${source.index}-${source.title}`} className="text-sm">
                      <p className="text-white">
                        <span className="text-gold font-mono">[{source.index}]</span>{' '}
                        {source.title}
                        {source.page ? (
                          <span className="text-white-30">
                            {' '}
                            · {t('page')} {source.page}
                          </span>
                        ) : null}
                      </p>
                      {source.snippet ? (
                        <p className="text-white-60 mt-1 mb-0 leading-relaxed">{source.snippet}</p>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            <p className="text-xs text-white-30">
              {t('usage', {
                in: answer.usage?.input_tokens ?? 0,
                out: answer.usage?.output_tokens ?? 0,
                ms: answer.latency_ms,
              })}
              {answer.model?.name ? ` · ${answer.model.name}` : ''}
            </p>
          </div>
        )}

        <p className="mt-12 text-sm text-white-30 leading-relaxed max-w-2xl">{t('footnote')}</p>
        <div className="mt-8 flex flex-wrap gap-4">
          <Link
            href="/signup"
            className="bg-gold text-bg font-medium text-sm tracking-wide px-7 py-3.5 rounded-md hover:bg-gold-light transition-colors"
          >
            {t('cta_signup')}
          </Link>
          <Link
            href="/contact"
            className="text-sm tracking-wide px-7 py-3.5 rounded-md border border-white/[0.12] text-white-60 hover:text-white hover:border-white/[0.24] transition-colors"
          >
            {t('cta_contact')}
          </Link>
        </div>
      </div>
    </section>
  )
}
