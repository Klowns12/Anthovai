'use client'

import { useLocale } from 'next-intl'
import { Link, usePathname } from '@/i18n/navigation'
import { cn } from '@/lib/utils'
import type { Organization, User } from '@/lib/session'

const SECTIONS = [
  { href: '/dashboard', label: 'Overview' },
  { href: '/dashboard/agents', label: 'Agents' },
  { href: '/dashboard/knowledge', label: 'Knowledge' },
  { href: '/dashboard/keys', label: 'API keys' },
] as const

interface Props {
  organization: Organization
  user: User
  memberships: number
}

export function DashboardNav({ organization, user, memberships }: Props) {
  const pathname = usePathname()
  const locale = useLocale()

  return (
    <div className="border-b border-white/[0.06] mb-12">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <div className="flex flex-wrap items-baseline justify-between gap-4 pb-6">
          <div>
            <h1 className="font-display text-[clamp(28px,3.5vw,40px)] leading-tight text-white">
              {organization.name}
            </h1>
            <p className="text-xs tracking-[0.2em] uppercase text-white-30 mt-2">
              {organization.plan} plan
              {memberships > 1 && (
                <>
                  {' · '}
                  <Link
                    href="/dashboard/organizations"
                    className="text-gold hover:text-gold-light transition-colors"
                  >
                    switch
                  </Link>
                </>
              )}
            </p>
          </div>

          <div className="text-right">
            <p className="text-sm text-white-60">{user.name || user.email}</p>
            <SignOut locale={locale} />
            {!user.email_verified && (
              // Worth saying here rather than only when a key is refused: it is
              // the one thing that silently blocks the last step of setup.
              <p className="text-xs text-gold mt-1">Email not confirmed</p>
            )}
          </div>
        </div>

        <nav className="flex gap-8 -mb-px">
          {SECTIONS.map((section) => {
            const active =
              section.href === '/dashboard'
                ? pathname === '/dashboard'
                : pathname.startsWith(section.href)

            return (
              <Link
                key={section.href}
                href={section.href}
                className={cn(
                  'pb-4 text-[11px] font-medium tracking-[0.2em] uppercase transition-colors border-b',
                  active
                    ? 'text-gold border-gold'
                    : 'text-white-30 border-transparent hover:text-white-60'
                )}
              >
                {section.label}
              </Link>
            )
          })}
        </nav>
      </div>
    </div>
  )
}

/**
 * Leaving.
 *
 * A real form rather than a click handler: the route replies with a redirect,
 * so the browser applies the cleared cookies and lands on the sign-in page in
 * one step. Nothing to race, and nothing that needs JavaScript to work.
 */
function SignOut({ locale }: { locale: string }) {
  return (
    <form method="POST" action="/api/session/signout">
      <input type="hidden" name="next" value={`/${locale}/signin`} />
      <button
        type="submit"
        className="text-[11px] tracking-[0.2em] uppercase text-white-30 hover:text-gold transition-colors mt-2"
      >
        Sign out
      </button>
    </form>
  )
}
