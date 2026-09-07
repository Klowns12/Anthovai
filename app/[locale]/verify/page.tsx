import type { Metadata } from 'next'
import { Link } from '@/i18n/navigation'
import { API_URL } from '@/lib/anthovai'
import { AuthShell } from '@/components/dashboard/AuthShell'

export const metadata: Metadata = {
  title: 'Confirm your address',
  robots: { index: false, follow: false },
}

type Props = {
  params: Promise<{ locale: string }>
  searchParams: Promise<{ token?: string }>
}

/**
 * The page a confirmation link lands on.
 *
 * The work happens here, on the server, rather than in a button the visitor
 * has to press: they already pressed one, in their mail client, and asking
 * again would be asking them to confirm that they meant to confirm.
 *
 * No session is required or consulted. A link is opened in whichever browser
 * was holding the mail, which is very often not the one that signed up — and
 * bouncing them to a sign-in page would spend the single-use token on the
 * redirect.
 */
export default async function VerifyPage({ params, searchParams }: Props) {
  const { locale } = await params
  const { token } = await searchParams

  const outcome = token ? await confirm(token) : 'missing'

  const copy = {
    ok: {
      title: 'Address confirmed',
      intro:
        'That is the last of the setup. You can create a live API key now — the one your own site or app will use.',
    },
    invalid: {
      title: 'That link did not work',
      intro:
        'Confirmation links work once and expire after a day. Sign in and ask for a new one; it takes a moment.',
    },
    missing: {
      title: 'Nothing to confirm',
      intro:
        'This page expects a confirmation link. Open the one in the email we sent you.',
    },
    unreachable: {
      title: 'We could not reach the platform',
      intro:
        'Your link is fine — this is our side. Try again in a minute, and ask for a new link if it has been more than a day.',
    },
  }[outcome]

  return (
    <AuthShell label="Anthovai Platform" title={copy.title} intro={copy.intro}>
      <Link
        href={outcome === 'ok' ? '/dashboard/keys' : '/signin'}
        className="text-sm text-gold hover:text-gold-light transition-colors"
      >
        {outcome === 'ok' ? 'Go to API keys →' : 'Sign in →'}
      </Link>
    </AuthShell>
  )
}

type Outcome = 'ok' | 'invalid' | 'missing' | 'unreachable'

/**
 * Called straight from the platform rather than through the browser proxy:
 * this runs on the server and has no origin to fetch from.
 */
async function confirm(token: string): Promise<Outcome> {
  try {
    const response = await fetch(`${API_URL}/dashboard/v1/auth/verify`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ token }),
      cache: 'no-store',
    })

    if (response.ok) return 'ok'
    // Anything the platform actually answered is a verdict on the token. Only
    // a failure to reach it at all is our problem to apologise for.
    if (response.status >= 500) return 'unreachable'
    return 'invalid'
  } catch {
    return 'unreachable'
  }
}
