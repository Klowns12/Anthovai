/**
 * Choosing which organization this browser is working in.
 *
 * A cookie rather than a URL segment, because it is a property of the session
 * and not of the page: every dashboard request needs it, including the ones
 * made from components that have no idea what route they are under.
 *
 * The id is checked against the caller's own memberships before it is written.
 * Without that, anyone could set the cookie by hand and the proxy would
 * faithfully forward someone else's organization id on every request — the
 * platform would refuse it, but relying on that would be asking the last line
 * of defence to be the first.
 */

import { NextRequest, NextResponse } from 'next/server'
import { currentUser, ORG_COOKIE } from '@/lib/session'

/** A year. Long enough that nobody is asked twice; the session expires first. */
const MAX_AGE = 60 * 60 * 24 * 365

export async function GET(request: NextRequest) {
  const account = await currentUser()
  if (!account) {
    return NextResponse.redirect(new URL('/signin', request.nextUrl.origin))
  }

  const id = request.nextUrl.searchParams.get('id')
  const belongs = account.organizations.some((org) => org.id === id)

  if (!id || !belongs) {
    return NextResponse.redirect(new URL('/dashboard', request.nextUrl.origin))
  }

  // Only ever a path on this site: `next` comes from the query string, and an
  // absolute URL there would turn this into an open redirect.
  const requested = request.nextUrl.searchParams.get('next') ?? '/dashboard'
  const next = requested.startsWith('/') && !requested.startsWith('//')
    ? requested
    : '/dashboard'

  const response = NextResponse.redirect(new URL(next, request.nextUrl.origin))

  response.cookies.set(ORG_COOKIE, id, {
    httpOnly: true,
    sameSite: 'lax',
    secure: process.env.NODE_ENV === 'production',
    path: '/',
    maxAge: MAX_AGE,
  })

  return response
}
