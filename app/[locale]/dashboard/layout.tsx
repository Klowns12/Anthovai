import type { Metadata } from 'next'
import { redirectTo } from '@/lib/navigate'
import { redirect as hardRedirect } from 'next/navigation'
import { currentUser, currentOrgId, fromPlatform, type Organization } from '@/lib/session'
import { DashboardNav } from '@/components/dashboard/DashboardNav'

export const metadata: Metadata = {
  title: { default: 'Dashboard', template: '%s · Anthovai' },
  robots: { index: false, follow: false },
}

type Props = {
  children: React.ReactNode
  params: Promise<{ locale: string }>
}

/**
 * Everything under `/dashboard` needs three things to be true, in order: a
 * signed-in user, an organization that user belongs to, and that organization
 * chosen for this browser. Each missing one has its own door.
 *
 * Checked here rather than in each page so a new page cannot forget.
 */
export default async function DashboardLayout({ children, params }: Props) {
  const { locale } = await params
  const account = await currentUser()

  if (!account) {
    redirectTo({ href: '/signin', locale })
  }

  // Signing up does not create an organization — the first one is a deliberate
  // step, because its name and address are what a customer's own users will
  // eventually see.
  if (account.organizations.length === 0) {
    redirectTo({ href: '/start', locale })
  }

  const chosen = await currentOrgId()
  const valid = account.organizations.some((org) => org.id === chosen)

  if (!valid) {
    // A cookie cannot be set from a layout, so the choice is made by a route
    // handler that can. With one organization there is nothing to choose, so
    // this is invisible; with several the chooser asks.
    const [only, ...rest] = account.organizations

    if (only && rest.length === 0) {
      hardRedirect(`/api/session/org?id=${only.id}`)
    }
    redirectTo({ href: '/organizations', locale })
  }

  // Unwrapped, unlike `POST /organizations`, which answers with
  // `{ organization, default_workspace }` because it creates both.
  const organization = await fromPlatform<Organization>('/organizations/current')

  return (
    <div className="min-h-screen pt-32 pb-24">
      <DashboardNav
        organization={organization}
        user={account.user}
        memberships={account.organizations.length}
      />
      <div className="mx-auto max-w-7xl px-6 lg:px-8">{children}</div>
    </div>
  )
}
