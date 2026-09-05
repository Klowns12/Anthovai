/**
 * Who is signed in, and which organization they are working in.
 *
 * Two separate questions with two separate answers. The session cookie is the
 * platform's and we only pass it along — we cannot read it, and should not want
 * to. The organization is ours: a customer can belong to several, and which one
 * they are looking at is a choice this site remembers on their behalf.
 */

import { cookies } from 'next/headers'
import { API_URL, ApiError, type ApiErrorBody } from './anthovai'

/**
 * Set by the platform, and named by it. Opaque here — we forward it and nothing
 * more.
 *
 * The `__Host-` prefix is not decoration: a browser will only accept a cookie
 * by that name if it is `Secure`, has `Path=/` and carries no `Domain`, which
 * together mean it cannot be planted by a subdomain. It also means the cookie
 * requires a secure origin — `localhost` counts as one, so local development
 * over plain HTTP still works, but a staging box on bare `http://` will not
 * hold a session at all.
 */
export const SESSION_COOKIE = '__Host-av_session'

/** Ours. Read by the proxy and sent onward as `X-Org-Id`. */
export const ORG_COOKIE = 'anthovai_org'

export interface User {
  id: string
  email: string
  name?: string | null
  email_verified: boolean
}

/**
 * What `/me` knows about an organization: that this user is in it, and in what
 * role. Its name and plan live behind `/organizations/current`, which needs the
 * organization to have been chosen first — so this is deliberately thin.
 */
export interface Membership {
  id: string
  role: string
}

export interface Organization {
  id: string
  slug: string
  name: string
  plan: string
  created_at: string
}

export interface Workspace {
  id: string
  name: string
  slug: string
}

/**
 * A server-side call to the platform, carrying the caller's own session.
 *
 * Used by server components, which cannot go through the browser-facing proxy —
 * a page rendering on the server has no origin to fetch from. It is the same
 * request the proxy would have made.
 */
export async function fromPlatform<T>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const jar = await cookies()
  const session = jar.get(SESSION_COOKIE)?.value
  const orgId = jar.get(ORG_COOKIE)?.value

  const headers = new Headers(init.headers)
  headers.set('content-type', 'application/json')
  if (session) headers.set('cookie', `${SESSION_COOKIE}=${session}`)
  if (orgId) headers.set('x-org-id', orgId)

  const response = await fetch(`${API_URL}/dashboard/v1${path}`, {
    ...init,
    headers,
    cache: 'no-store',
  })

  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as ApiErrorBody | null
    throw new ApiError(response.status, body, `${path} returned ${response.status}`)
  }

  return response.json() as Promise<T>
}

/**
 * The signed-in user, or `null`.
 *
 * `null` rather than an exception, because "not signed in" is the ordinary
 * state of most visitors to this site and not an error worth a stack trace.
 * Anything else — the platform being down, a malformed answer — does throw,
 * because treating those as "signed out" would quietly bounce a signed-in
 * customer to the login page during an outage.
 */
export async function currentUser(): Promise<Account | null> {
  const jar = await cookies()
  if (!jar.get(SESSION_COOKIE)) return null

  try {
    return await fromPlatform<Account>('/me')
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) return null
    throw error
  }
}

/** Exactly what `/me` returns. */
export interface Account {
  user: User
  organizations: Membership[]
}

/** The organization this browser is currently working in, if it has chosen one. */
export async function currentOrgId(): Promise<string | null> {
  const jar = await cookies()
  return jar.get(ORG_COOKIE)?.value ?? null
}
