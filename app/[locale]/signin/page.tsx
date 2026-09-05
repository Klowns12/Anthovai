import type { Metadata } from 'next'
import { redirectTo } from '@/lib/navigate'
import { currentUser } from '@/lib/session'
import { AuthShell, AuthLink } from '@/components/dashboard/AuthShell'
import { SignInForm } from '@/components/dashboard/SignInForm'

export const metadata: Metadata = {
  title: 'Sign in',
  // Nothing behind this page belongs in a search result.
  robots: { index: false, follow: false },
}

type Props = { params: Promise<{ locale: string }> }

export default async function SignInPage({ params }: Props) {
  const { locale } = await params

  // Someone already signed in has no use for this page.
  if (await currentUser()) {
    redirectTo({ href: '/dashboard', locale })
  }

  return (
    <AuthShell
      label="Anthovai Platform"
      title="Sign in"
      intro="Your agents, your knowledge, and the keys that reach them."
      footer={
        <>
          No account yet? <AuthLink href="/signup">Create one</AuthLink>.
        </>
      }
    >
      <SignInForm />
    </AuthShell>
  )
}
