import type { Metadata } from 'next'
import { notFound } from 'next/navigation'
import { Link } from '@/i18n/navigation'
import { fromPlatform } from '@/lib/session'
import { ApiError } from '@/lib/anthovai'
import type { DocumentSummary, KnowledgeBase, ListOf } from '@/lib/platform-types'
import { Documents } from '@/components/dashboard/Documents'

export const metadata: Metadata = { title: 'Knowledge base' }

type Props = { params: Promise<{ id: string }> }

export default async function KnowledgeBasePage({ params }: Props) {
  const { id } = await params

  let base: KnowledgeBase
  let documents: ListOf<DocumentSummary>

  try {
    ;[base, documents] = await Promise.all([
      fromPlatform<KnowledgeBase>(`/knowledge_bases/${id}`),
      fromPlatform<ListOf<DocumentSummary>>(`/knowledge_bases/${id}/documents`),
    ])
  } catch (error) {
    // The platform reports another tenant's knowledge base as missing rather
    // than forbidden, and so does this page: there is nothing here either way.
    if (error instanceof ApiError && error.status === 404) notFound()
    throw error
  }

  return (
    <div className="space-y-8">
      <div>
        <Link
          href="/dashboard/knowledge"
          className="text-xs tracking-[0.2em] uppercase text-white-30 hover:text-gold transition-colors"
        >
          ← Knowledge
        </Link>
        <h2 className="font-display text-3xl text-white mt-3">{base.name}</h2>
        <p className="font-mono text-[11px] text-white-30 mt-2">
          {base.embedding_model}
        </p>
      </div>

      <Documents knowledgeBaseId={base.id} initial={documents.data} />
    </div>
  )
}
