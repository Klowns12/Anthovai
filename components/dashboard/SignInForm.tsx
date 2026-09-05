'use client'

import { useState } from 'react'
import { useRouter } from '@/i18n/navigation'
import { Button } from '@/components/ui/Button'
import { Field } from '@/components/ui/Field'
import { api, explain } from '@/lib/dashboard-client'

export function SignInForm() {
  const router = useRouter()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(null)
    setBusy(true)

    const form = new FormData(event.currentTarget)

    try {
      await api.post('/auth/login', {
        email: String(form.get('email') ?? ''),
        password: String(form.get('password') ?? ''),
      })

      // The session cookie arrived on that response, through the proxy, under
      // this domain. `refresh` makes the server components re-read it so the
      // navigation and the dashboard see a signed-in user.
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
        label="Email"
        name="email"
        type="email"
        autoComplete="email"
        required
        placeholder="you@company.com"
      />
      <Field
        label="Password"
        name="password"
        type="password"
        autoComplete="current-password"
        required
        error={error}
      />
      <Button type="submit" size="lg" className="w-full" disabled={busy}>
        {busy ? 'Signing in…' : 'Sign in'}
      </Button>
    </form>
  )
}
