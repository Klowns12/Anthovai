import { Link } from '@/i18n/navigation'
import { fromPlatform } from '@/lib/session'
import type {
  AgentSummary,
  ApiKeySummary,
  KnowledgeBase,
  ListOf,
} from '@/lib/platform-types'

/**
 * What is here, and what is missing.
 *
 * An agent answers questions from knowledge, and a key is what reaches it — so
 * the three counts below are also the three steps of setting one up. Rendered
 * as a path rather than as statistics, because on the first visit every number
 * is zero and a row of zeroes tells a new customer nothing about what to do.
 */
export default async function DashboardPage() {
  const [agents, bases, keys] = await Promise.all([
    fromPlatform<ListOf<AgentSummary>>('/agents'),
    fromPlatform<ListOf<KnowledgeBase>>('/knowledge_bases'),
    fromPlatform<ListOf<ApiKeySummary>>('/api_keys'),
  ])

  const published = agents.data.filter((agent) => agent.published).length
  const documents = bases.data.reduce((total, base) => total + base.document_count, 0)
  const live = keys.data.filter((key) => key.status === 'active').length

  const steps = [
    {
      href: '/dashboard/knowledge',
      label: 'Knowledge',
      done: documents > 0,
      todo: 'Upload what your agent should know',
      done_text:
        documents === 1 ? '1 document' : `${documents} documents`,
      detail:
        bases.data.length === 1
          ? 'in 1 knowledge base'
          : `in ${bases.data.length} knowledge bases`,
    },
    {
      href: '/dashboard/agents',
      label: 'Agents',
      done: published > 0,
      todo: 'Create an agent and publish it',
      done_text: published === 1 ? '1 published' : `${published} published`,
      detail:
        agents.data.length > published
          ? `${agents.data.length - published} still in draft`
          : 'ready to answer',
    },
    {
      href: '/dashboard/keys',
      label: 'API keys',
      done: live > 0,
      todo: 'Create a key for your own site to use',
      done_text: live === 1 ? '1 active key' : `${live} active keys`,
      detail: 'call /v1/chat with it',
    },
  ]

  return (
    <div className="space-y-12">
      <div className="grid gap-4 md:grid-cols-3">
        {steps.map((step) => (
          <Link
            key={step.href}
            href={step.href}
            className="group relative bg-bg-2 border border-white/[0.06] rounded-lg p-6 transition-all duration-300 hover:border-gold-border hover:shadow-gold"
          >
            <p className="text-[10px] font-medium tracking-[0.2em] uppercase text-white-30 mb-4">
              {step.label}
            </p>

            {step.done ? (
              <>
                <p className="font-display text-3xl text-white leading-none">
                  {step.done_text}
                </p>
                <p className="text-sm text-white-60 mt-3">{step.detail}</p>
              </>
            ) : (
              <>
                <p className="text-white leading-snug">{step.todo}</p>
                <p className="text-sm text-gold mt-3 group-hover:text-gold-light transition-colors">
                  Start →
                </p>
              </>
            )}
          </Link>
        ))}
      </div>

      {documents > 0 && published > 0 && live > 0 && (
        <section className="bg-bg-2 border border-white/[0.06] rounded-lg p-8">
          <h2 className="font-display text-2xl text-white mb-3">
            Everything is in place
          </h2>
          <p className="text-white-60 leading-relaxed max-w-2xl">
            Your agent is published and there is a key that can reach it. Try a
            question in the playground before you wire it into your own site —
            it runs the draft, so you can change the instructions and ask again
            without publishing.
          </p>
          <Link
            href="/dashboard/agents"
            className="inline-block text-gold hover:text-gold-light transition-colors mt-5 text-sm"
          >
            Open an agent →
          </Link>
        </section>
      )}
    </div>
  )
}
