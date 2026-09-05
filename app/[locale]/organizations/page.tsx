import type { Metadata } from 'next'
import { redirectTo } from '@/lib/navigate'
import { redirect as hardRedirect } from 'next/navigation'
import { currentUser } from '@/lib/session'
import { AuthShell, AuthLink } from '@/components/dashboard/AuthShell'

export const metadata: Metadata = {
  title: 'Choose an organization',
  robots: { index: false, follow: false },
}

type Props = { params: Promise<{ locale: string }> }

/**
 * Also outside `/dashboard`: that layout needs an organization already chosen,
 * and this is where the choice is made.
 *
 * Plain links rather than a form. Each one goes to the route handler that
 * validates the membership and sets the cookie, so the whole page is a handful
 * of anchors and no client JavaScript at all.
 */
export default async function OrganizationsPage({ params }: Props) {
  const { locale } = await params
  const account = await currentUser()

  if (!account) {
    redirectTo({ href: '/signin', locale })
  }
  if (account.organizations.length === 0) {
    redirectTo({ href: '/start', locale })
  }

  // With one organization there is nothing to choose. Middleware also sends
  // anyone here whose organization cookie is simply missing — a new browser, a
  // cleared cookie jar — and asking them to pick from a list of one would be a
  // question with a single possible answer.
  const [only, ...rest] = account.organizations
  if (only && rest.length === 0) {
    hardRedirect(`/api/session/org?id=${only.id}&next=/dashboard`)
  }

  return (
    <AuthShell
      label="Anthovai Platform"
      title="Choose an organization"
      intro="You belong to more than one. Pick the one you are working in — you can switch at any time."
      footer={<>Signed in as {account.user.email}.</>}
    >
      <ul className="space-y-3">
        {account.organizations.map((org) => (
          <li key={org.id}>
            <a
              href={`/api/session/org?id=${org.id}&next=/dashboard`}
              className="flex items-center justify-between bg-bg-2 border border-white/[0.06] rounded-md px-5 py-4 transition-colors hover:border-gold-border"
            >
              {/* `/me` knows the id and the role; the name lives behind an
                  endpoint that needs the organization chosen first, which is
                  exactly what this page is for. */}
              <span className="font-mono text-sm text-white">{org.id}</span>
              <span className="text-[10px] tracking-[0.2em] uppercase text-gold">
                {org.role}
              </span>
            </a>
          </li>
        ))}
      </ul>

      <p className="text-sm text-white-60 mt-8">
        Or <AuthLink href="/start">create another</AuthLink>.
      </p>
    </AuthShell>
  )
}
