import type { Metadata } from 'next'
import { redirectTo } from '@/lib/navigate'
import { currentUser } from '@/lib/session'
import { AuthShell, AuthLink } from '@/components/dashboard/AuthShell'
import { SignUpForm } from '@/components/dashboard/SignUpForm'

export const metadata: Metadata = {
  title: 'Create an account',
  robots: { index: false, follow: false },
}

type Props = { params: Promise<{ locale: string }> }

export default async function SignUpPage({ params }: Props) {
  const { locale } = await params

  if (await currentUser()) {
    redirectTo({ href: '/dashboard', locale })
  }

  return (
    <AuthShell
      label="Anthovai Platform"
      title="Create an account"
      intro="Upload what you know. Get an agent that answers from it, with citations back to the passage it used."
      footer={
        <>
          Already have an account? <AuthLink href="/signin">Sign in</AuthLink>.
        </>
      }
    >
      <SignUpForm />
    </AuthShell>
  )
}
