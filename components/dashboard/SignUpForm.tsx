'use client'

import { useState } from 'react'
import { useRouter } from '@/i18n/navigation'
import { Button } from '@/components/ui/Button'
import { Field } from '@/components/ui/Field'
import { api, explain } from '@/lib/dashboard-client'

/**
 * Mirrors `MIN_LENGTH` in `platform/crates/auth/src/password.rs`, so someone
 * learns the rule while typing rather than after submitting.
 *
 * The platform is the authority — this only saves a round trip. If the two ever
 * disagree the platform still wins, and the form simply asks twice.
 */
const MIN_PASSWORD = 10

export function SignUpForm() {
  const router = useRouter()
  const [error, setError] = useState<string | null>(null)
  const [passwordError, setPasswordError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(null)
    setPasswordError(null)

    const form = new FormData(event.currentTarget)
    const password = String(form.get('password') ?? '')

    if (password.length < MIN_PASSWORD) {
      setPasswordError(`Use at least ${MIN_PASSWORD} characters.`)
      return
    }

    setBusy(true)

    try {
      const email = String(form.get('email') ?? '')
      const name = String(form.get('name') ?? '').trim()

      await api.post('/auth/signup', {
        email,
        password,
        ...(name ? { name } : {}),
      })

      // Signing up does not sign you in — the platform issues a session only on
      // `/auth/login`. Doing it here means one form, not two.
      await api.post('/auth/login', { email, password })

      router.replace('/dashboard')
      router.refresh()
    } catch (failure) {
      setError(explain(failure))
      setBusy(false)
    }
  }

  return (
    <form onSubmit={submit} className="space-y-6">
      <Field
        label="Name"
        name="name"
        autoComplete="name"
        placeholder="Optional"
      />
      <Field
        label="Email"
        name="email"
        type="email"
        autoComplete="email"
        required
        placeholder="you@company.com"
        error={error}
      />
      <Field
        label="Password"
        name="password"
        type="password"
        autoComplete="new-password"
        required
        minLength={MIN_PASSWORD}
        hint={`At least ${MIN_PASSWORD} characters. A phrase beats a puzzle.`}
        error={passwordError}
      />
      <Button type="submit" size="lg" className="w-full" disabled={busy}>
        {busy ? 'Creating your account…' : 'Create account'}
      </Button>
    </form>
  )
}
