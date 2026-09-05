import createMiddleware from 'next-intl/middleware'
import { NextRequest, NextResponse } from 'next/server'
import { routing } from './i18n/routing'

const intl = createMiddleware(routing)

/**
 * Cookie names, repeated rather than imported: middleware runs on the edge
 * runtime, and importing `lib/session` would drag `next/headers` in with it.
 */
const SESSION = '__Host-av_session'
const ORG = 'anthovai_org'

/**
 * Guarding `/dashboard` before anything renders.
 *
 * The layout under `/dashboard` checks the same things and is the authority —
 * it asks the platform, so it catches a session that has expired and a cookie
 * naming an organization the user has since left. What it cannot do is stop a
 * page from running: in the App Router a layout and its page render at the same
 * time, so `redirect()` in a layout decides what is *displayed* while the page
 * has already begun fetching. A page whose fetches need an organization then
 * fails before the redirect lands, and the visitor gets an error where they
 * should have got the sign-in screen. That is not theoretical — it is what this
 * dashboard did the first time it was opened.
 *
 * So the cheap half of the check moves here, where it runs first: are the
 * cookies even there? No platform call, no round trip — just enough to keep a
 * request that obviously cannot succeed from reaching a page at all.
 */
export default function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl

  if (pathname.startsWith('/dashboard')) {
    if (!request.cookies.has(SESSION)) {
      return NextResponse.redirect(new URL('/signin', request.url))
    }
    if (!request.cookies.has(ORG)) {
      // `/organizations` sorts out which one — including creating the first,
      // and skipping the question when there is only one to ask about.
      return NextResponse.redirect(new URL('/organizations', request.url))
    }
  }

  return intl(request)
}

export const config = {
  matcher: ['/((?!api|_next|.*\\..*).*)'],
}
