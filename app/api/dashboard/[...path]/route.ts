/**
 * The only door between the browser and the platform.
 *
 * Everything the dashboard does goes through here. Three things happen that
 * could not happen if the browser called the platform directly:
 *
 *   - The session cookie becomes first-party. The platform sets it, we pass it
 *     on under our own domain, and the browser never has to be persuaded to
 *     keep a cross-site cookie.
 *   - `X-Org-Id`, which the platform requires on every dashboard call, is added
 *     here from a cookie rather than trusted from the page. A tab that has been
 *     open since before the user switched organizations cannot act on the old
 *     one.
 *   - The platform's address stays on the server.
 *
 * What is deliberately *not* here is any decision about what a request may do.
 * Authorisation is the platform's, and duplicating a slice of it in a proxy is
 * how the two drift apart until one of them is wrong.
 */

import { NextRequest, NextResponse } from 'next/server'
import { API_URL } from '@/lib/anthovai'
import { ORG_COOKIE } from '@/lib/session'

/** Headers we send onward. Anything not listed is dropped. */
const FORWARD_TO_PLATFORM = ['content-type', 'cookie', 'accept-language']

/**
 * Headers we send back. `set-cookie` is the important one — it is how signing
 * in works — and it is handled separately because a response may carry several.
 */
const FORWARD_TO_BROWSER = ['content-type', 'cache-control', 'x-request-id']

/**
 * Uploads are streamed rather than buffered, so a hundred-megabyte PDF does not
 * become a hundred megabytes of memory in a serverless function.
 */
const STREAMED_METHODS = new Set(['POST', 'PATCH', 'PUT'])

async function proxy(request: NextRequest, path: string[]) {
  const target = new URL(`${API_URL}/dashboard/v1/${path.join('/')}`)
  target.search = request.nextUrl.search

  const headers = new Headers()
  for (const name of FORWARD_TO_PLATFORM) {
    const value = request.headers.get(name)
    if (value) headers.set(name, value)
  }

  // From our own cookie, never from the request body or a header the page set.
  const orgId = request.cookies.get(ORG_COOKIE)?.value
  if (orgId) headers.set('x-org-id', orgId)

  // The platform checks this against its allow-list on every state-changing
  // dashboard call. It is the same-origin check that makes the session cookie
  // safe to use, so it has to be the real origin of this deployment.
  headers.set('origin', request.nextUrl.origin)

  const body = STREAMED_METHODS.has(request.method) ? request.body : undefined

  let upstream: Response
  try {
    upstream = await fetch(target, {
      method: request.method,
      headers,
      body,
      // Required by undici whenever a stream is sent.
      ...(body ? { duplex: 'half' } : {}),
      redirect: 'manual',
      cache: 'no-store',
    })
  } catch {
    // The platform is not answering. Said plainly, because during development
    // it is usually just not running — and a 502 with no explanation sends
    // people looking for a bug in the dashboard instead.
    return NextResponse.json(
      {
        error: {
          type: 'service_unavailable',
          code: 'platform_unreachable',
          message: `The Anthovai platform did not answer at ${API_URL}.`,
          request_id: '',
          doc_url: '',
        },
      },
      { status: 503 }
    )
  }

  const response = new NextResponse(upstream.body, { status: upstream.status })

  for (const name of FORWARD_TO_BROWSER) {
    const value = upstream.headers.get(name)
    if (value) response.headers.set(name, value)
  }

  // A sign-in sets a session cookie and a sign-out clears one; both arrive
  // here. `getSetCookie` is used rather than `get` because a response may
  // carry more than one and `get` would silently keep only the last.
  for (const cookie of upstream.headers.getSetCookie()) {
    response.headers.append('set-cookie', cookie)
  }

  // Nothing behind this door is cacheable: it is all one customer's data.
  response.headers.set('cache-control', 'private, no-store')

  return response
}

type Context = { params: Promise<{ path: string[] }> }

export async function GET(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path)
}

export async function POST(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path)
}

export async function PATCH(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path)
}

export async function PUT(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path)
}

export async function DELETE(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path)
}
