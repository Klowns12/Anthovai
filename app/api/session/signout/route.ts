/**
 * Ending a session.
 *
 * Two cookies end here, not one. The platform owns `__Host-av_session` and
 * clears it itself — we forward its `Set-Cookie` untouched. The organization
 * cookie is ours, and nothing upstream knows it exists; leaving it behind would
 * mean the next person to sign in on this browser starts inside the previous
 * customer's organization until they happen to switch.
 *
 * A POST, because a GET would let any image tag on any page sign a customer out.
 *
 * It answers with a redirect and is driven by a plain form, not by `fetch` and
 * a client-side navigation. That was the first version, and it signed people
 * out only sometimes: the router's navigation cancelled the request, Chrome
 * reported `ERR_ABORTED`, and a discarded response takes its `Set-Cookie`
 * headers with it. A form post has no such race — the browser applies the
 * cookies on the redirect it is already following — and it works without
 * JavaScript.
 */

import { NextRequest, NextResponse } from 'next/server'
import { API_URL } from '@/lib/anthovai'
import { SESSION_COOKIE, ORG_COOKIE } from '@/lib/session'

export async function POST(request: NextRequest) {
  const session = request.cookies.get(SESSION_COOKIE)?.value

  if (session) {
    try {
      await fetch(`${API_URL}/dashboard/v1/auth/logout`, {
        method: 'POST',
        headers: { cookie: `${SESSION_COOKIE}=${session}` },
      })
    } catch {
      // The platform being unreachable must not trap someone in a session they
      // are trying to leave. Clearing the cookies below still signs them out of
      // this browser; the token stays valid upstream until it expires, which is
      // the lesser of the two problems.
    }
  }

  // 303 so the browser follows with GET rather than repeating the POST.
  const form = await request.formData().catch(() => null)
  const requested = String(form?.get('next') ?? '/signin')
  // Only ever a path on this site; an absolute URL here would be an open
  // redirect on a route anyone can post to.
  const next =
    requested.startsWith('/') && !requested.startsWith('//') ? requested : '/signin'

  const response = NextResponse.redirect(new URL(next, request.nextUrl.origin), 303)

  for (const name of [SESSION_COOKIE, ORG_COOKIE]) {
    response.cookies.set(name, '', {
      httpOnly: true,
      sameSite: 'lax',
      secure: name.startsWith('__Host-') || process.env.NODE_ENV === 'production',
      path: '/',
      maxAge: 0,
    })
  }

  return response
}
