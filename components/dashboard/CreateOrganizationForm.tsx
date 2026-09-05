'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/Button'
import { Field } from '@/components/ui/Field'
import { api, explain } from '@/lib/dashboard-client'

/**
 * The platform's rules, from `validate_slug` in
 * `platform/crates/tenant/src/lib.rs`. Mirrored so the form can suggest a slug
 * that will be accepted rather than one that will bounce.
 */
const SLUG_MIN = 2
const SLUG_MAX = 48
const RESERVED = ['api', 'app', 'www', 'admin', 'internal', 'dashboard', 'v1']

export function slugify(name: string): string {
  const slug = name
    .toLowerCase()
    .normalize('NFKD')
    // Anything that is not a lowercase letter or digit becomes a hyphen — which
    // covers Thai, spaces and punctuation alike, since the platform accepts
    // ASCII only here.
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, SLUG_MAX)
    .replace(/-+$/, '')

  return slug
}

export function CreateOrganizationForm() {
  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  // Once someone edits the address themselves, the name stops driving it.
  // Rewriting what they typed under their cursor is a small betrayal.
  const [slugTouched, setSlugTouched] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [slugError, setSlugError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const effectiveSlug = slugTouched ? slug : slugify(name)

  function localSlugProblem(value: string): string | null {
    if (value.length < SLUG_MIN) return `At least ${SLUG_MIN} characters.`
    if (value.length > SLUG_MAX) return `At most ${SLUG_MAX} characters.`
    if (!/^[a-z0-9-]+$/.test(value)) return 'Lowercase letters, digits and hyphens only.'
    if (value.startsWith('-') || value.endsWith('-')) return 'No hyphen at the start or end.'
    if (RESERVED.includes(value)) return 'That address is reserved.'
    return null
  }

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(null)

    const problem = localSlugProblem(effectiveSlug)
    if (problem) {
      setSlugError(problem)
      return
    }
    setSlugError(null)
    setBusy(true)

    try {
      const created = await api.post<{ organization: { id: string } }>(
        '/organizations',
        { name: name.trim(), slug: effectiveSlug }
      )

      // A full navigation rather than a client one: the route handler sets the
      // organization cookie, and every server component below needs to be
      // rendered again with it in place.
      window.location.href = `/api/session/org?id=${created.organization.id}&next=/dashboard`
    } catch (failure) {
      setError(explain(failure))
      setBusy(false)
    }
  }

  return (
    <form onSubmit={submit} className="space-y-6">
      <Field
        label="Organization name"
        name="name"
        required
        value={name}
        onChange={(event) => setName(event.target.value)}
        placeholder="ABC School"
        error={error}
      />
      <Field
        label="Address"
        name="slug"
        required
        value={effectiveSlug}
        onChange={(event) => {
          setSlugTouched(true)
          setSlug(event.target.value)
          setSlugError(null)
        }}
        placeholder="abc-school"
        hint="Lowercase letters, digits and hyphens. This appears in URLs and cannot be changed later."
        error={slugError}
      />
      <Button type="submit" size="lg" className="w-full" disabled={busy || !name.trim()}>
        {busy ? 'Creating…' : 'Create organization'}
      </Button>
    </form>
  )
}
