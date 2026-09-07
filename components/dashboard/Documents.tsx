'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/Button'
import { Empty } from '@/components/dashboard/Empty'
import { api, explain } from '@/lib/dashboard-client'
import { isInProgress, type DocumentSummary, type ListOf } from '@/lib/platform-types'
import { formatBytes, timeAgo } from '@/lib/format'

/**
 * How often to ask again while the worker is busy.
 *
 * Ingestion is asynchronous by design — parsing, chunking and embedding a
 * document takes seconds to minutes, and holding an HTTP request open for it
 * would be worse in every way. So the page polls, but only while something is
 * actually in progress: a settled list asks nothing.
 */
const POLL_MS = 2000

type Source = 'file' | 'text' | 'url'

export function Documents({
  knowledgeBaseId,
  initial,
}: {
  knowledgeBaseId: string
  initial: DocumentSummary[]
}) {
  const [documents, setDocuments] = useState(initial)
  const [source, setSource] = useState<Source>('file')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const formRef = useRef<HTMLFormElement>(null)

  const working = documents.some(isInProgress)

  const refresh = useCallback(async () => {
    try {
      const { data } = await api.get<ListOf<DocumentSummary>>(
        `/knowledge_bases/${knowledgeBaseId}/documents`
      )
      setDocuments(data)
    } catch {
      // A failed poll is not worth interrupting anyone over. The next one is
      // two seconds away, and the list on screen is still the last true answer.
    }
  }, [knowledgeBaseId])

  useEffect(() => {
    if (!working) return
    const timer = setInterval(refresh, POLL_MS)
    return () => clearInterval(timer)
  }, [working, refresh])

  async function upload(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(null)
    setBusy(true)

    const form = new FormData(event.currentTarget)
    const body = new FormData()

    // Order matters. The platform reserves the document and checks the plan's
    // size limit when it sees the knowledge base id, so it has to arrive before
    // the bytes do — otherwise an oversized file is stored and then rejected.
    body.set('knowledge_base_id', knowledgeBaseId)

    if (source === 'file') {
      const file = form.get('file')
      if (!(file instanceof File) || file.size === 0) {
        setError('Choose a file first.')
        setBusy(false)
        return
      }
      body.set('file', file)
    } else if (source === 'text') {
      body.set('title', String(form.get('title') ?? '').trim())
      body.set('text', String(form.get('text') ?? ''))
    } else {
      body.set('url', String(form.get('url') ?? '').trim())
    }

    try {
      const created = await api.post<DocumentSummary>('/documents', body)
      setDocuments((current) => [created, ...current])
      formRef.current?.reset()
    } catch (failure) {
      setError(explain(failure))
    } finally {
      setBusy(false)
    }
  }

  async function remove(id: string) {
    const previous = documents
    setDocuments((current) => current.filter((doc) => doc.id !== id))
    try {
      await api.delete(`/documents/${id}`)
    } catch (failure) {
      // Put it back rather than leave the list telling a comfortable lie.
      setDocuments(previous)
      setError(explain(failure))
    }
  }

  async function retry(id: string) {
    try {
      const updated = await api.post<DocumentSummary>(`/documents/${id}/retry`)
      setDocuments((current) =>
        current.map((doc) => (doc.id === id ? updated : doc))
      )
    } catch (failure) {
      setError(explain(failure))
    }
  }

  return (
    <div className="space-y-8">
      <form
        ref={formRef}
        onSubmit={upload}
        className="bg-bg-2 border border-white/[0.06] rounded-lg p-8"
      >
        <div className="flex gap-6 mb-6">
          {(['file', 'text', 'url'] as const).map((option) => (
            <button
              key={option}
              type="button"
              onClick={() => {
                setSource(option)
                setError(null)
              }}
              className={`text-[11px] font-medium tracking-[0.2em] uppercase pb-2 border-b transition-colors ${
                source === option
                  ? 'text-gold border-gold'
                  : 'text-white-30 border-transparent hover:text-white-60'
              }`}
            >
              {option === 'file' ? 'Upload' : option === 'text' ? 'Paste' : 'Fetch a page'}
            </button>
          ))}
        </div>

        {source === 'file' && (
          <div>
            <input
              type="file"
              name="file"
              accept=".pdf,.docx,.txt,.md,.markdown,.json,.csv,.html,.htm"
              className="block w-full text-sm text-white-60 file:mr-4 file:py-2 file:px-4 file:rounded-sm file:border file:border-gold-border file:bg-gold-dim file:text-gold file:text-[11px] file:tracking-[0.2em] file:uppercase file:cursor-pointer"
            />
            <p className="text-xs text-white-30 mt-3">
              PDF, Word, Markdown, text, JSON, CSV or HTML.
            </p>
          </div>
        )}

        {source === 'text' && (
          <div className="space-y-4">
            <input
              name="title"
              required
              placeholder="Title"
              className="w-full bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border"
            />
            <textarea
              name="text"
              required
              rows={6}
              placeholder="Paste the text an agent should be able to answer from."
              className="w-full bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border resize-y"
            />
          </div>
        )}

        {source === 'url' && (
          <div>
            <input
              name="url"
              type="url"
              required
              placeholder="https://example.com/handbook"
              className="w-full bg-bg-3 border border-white/[0.08] rounded-md px-4 py-3 text-white placeholder:text-white-30 focus:outline-none focus:border-gold-border"
            />
            <p className="text-xs text-white-30 mt-3">
              We fetch the page and keep its readable text — navigation, footers
              and scripts are left behind. Addresses that are not reachable from
              the public internet are refused.
            </p>
          </div>
        )}

        {error && <p className="text-sm text-gold mt-4">{error}</p>}

        <Button type="submit" className="mt-6" disabled={busy}>
          {busy ? 'Sending…' : 'Add document'}
        </Button>
      </form>

      {documents.length === 0 ? (
        <Empty
          title="Nothing in here yet"
          body="Add a document and the worker parses, chunks and indexes it. It takes a few seconds; you can watch it happen."
        />
      ) : (
        <ul className="space-y-3">
          {documents.map((doc) => (
            <DocumentRow
              key={doc.id}
              document={doc}
              onDelete={() => remove(doc.id)}
              onRetry={() => retry(doc.id)}
            />
          ))}
        </ul>
      )}
    </div>
  )
}

function DocumentRow({
  document,
  onDelete,
  onRetry,
}: {
  document: DocumentSummary
  onDelete: () => void
  onRetry: () => void
}) {
  const failed = document.status === 'failed'
  const ready = document.status === 'ready'

  return (
    <li className="bg-bg-2 border border-white/[0.06] rounded-lg px-6 py-5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="text-white truncate">{document.title}</p>
          <p className="text-sm text-white-60 mt-1">
            {ready ? (
              <>
                {document.chunk_count === 1
                  ? '1 passage'
                  : `${document.chunk_count} passages`}
                {' · '}
                {formatBytes(document.size_bytes)}
                {document.language && ` · ${document.language}`}
              </>
            ) : failed ? (
              <span className="text-gold">
                {document.error_message ?? document.error_code ?? 'Could not be read'}
              </span>
            ) : (
              <>
                {document.status}
                {' · '}
                {document.progress}%
              </>
            )}
          </p>
          <p className="text-xs text-white-30 mt-2">
            {document.source_type} · {timeAgo(document.updated_at)}
          </p>
        </div>

        <div className="flex items-center gap-3 shrink-0">
          {failed && (
            <button
              onClick={onRetry}
              className="text-xs tracking-[0.15em] uppercase text-gold hover:text-gold-light transition-colors"
            >
              Retry
            </button>
          )}
          <button
            onClick={onDelete}
            className="text-xs tracking-[0.15em] uppercase text-white-30 hover:text-white-60 transition-colors"
          >
            Remove
          </button>
        </div>
      </div>

      {isInProgress(document) && (
        <div className="mt-4 h-px bg-white/[0.06] overflow-hidden">
          <div
            className="h-px bg-gold transition-all duration-500"
            style={{ width: `${Math.max(document.progress, 4)}%` }}
          />
        </div>
      )}
    </li>
  )
}
