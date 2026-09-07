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

  const response = new NextResponse(null, { status: 204 })

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
