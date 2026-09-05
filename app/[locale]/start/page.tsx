import type { Metadata } from 'next'
import { redirectTo } from '@/lib/navigate'
import { currentUser } from '@/lib/session'
import { AuthShell } from '@/components/dashboard/AuthShell'
import { CreateOrganizationForm } from '@/components/dashboard/CreateOrganizationForm'

export const metadata: Metadata = {
  title: 'Create an organization',
  robots: { index: false, follow: false },
}

type Props = { params: Promise<{ locale: string }> }

/**
 * Outside the `/dashboard` tree on purpose. That layout requires an
 * organization to exist and sends anyone without one here — so a page under it
 * that creates the first organization would redirect to itself, forever.
 */
export default async function StartPage({ params }: Props) {
  const { locale } = await params
  const account = await currentUser()

  if (!account) {
    redirectTo({ href: '/signin', locale })
  }
  if (account.organizations.length > 0) {
    redirectTo({ href: '/dashboard', locale })
  }

  return (
    <AuthShell
      label="One more step"
      title="Name your organization"
      intro="Everything you build lives inside it — agents, knowledge, keys and the people you invite. Most companies need only one."
      footer="You can create more later, and switch between them."
    >
      <CreateOrganizationForm />
    </AuthShell>
  )
}
