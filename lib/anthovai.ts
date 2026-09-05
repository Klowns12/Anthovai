/**
 * Talking to the Anthovai platform.
 *
 * The platform is a separate service — Rust, PostgreSQL, a background worker —
 * that cannot run on Vercel and does not deploy from this repository. It lives
 * in `platform/` and answers on its own host.
 *
 * Nothing in the browser ever reaches it directly. Every call goes through the
 * proxy at `/api/dashboard/*`, and the reason is the session cookie: the
 * platform sets one, and a cookie from a different origin needs
 * `SameSite=None`, `Secure`, and CORS with credentials to survive the trip.
 * Proxying makes it a first-party cookie on anthovai.com instead, which is both
 * safer and far less to get wrong. It also keeps the platform's address out of
 * the browser entirely.
 */

/** Where the platform answers. Server-side only — never `NEXT_PUBLIC_`. */
export const API_URL =
  process.env.ANTHOVAI_API_URL?.replace(/\/$/, '') ?? 'http://127.0.0.1:8080'

/**
 * The error body every endpoint returns, from
 * `platform/docs/spec-v0.1/05-api-specification.md` §2.2.
 *
 * `code` is the stable string to branch on. `message` is written for a
 * developer rather than an end user, so the UI shows its own wording for the
 * codes it knows and falls back to this for the ones it does not.
 */
export interface ApiErrorBody {
  error: {
    type: string
    code: string
    message: string
    param?: string
    request_id: string
    doc_url: string
  }
}

export class ApiError extends Error {
  readonly status: number
  readonly code: string
  readonly requestId: string

  constructor(status: number, body: ApiErrorBody | null, fallback: string) {
    super(body?.error?.message ?? fallback)
    this.name = 'ApiError'
    this.status = status
    this.code = body?.error?.code ?? 'unknown_error'
    // Worth surfacing in support: it is the one string that finds a single
    // request in the platform's logs.
    this.requestId = body?.error?.request_id ?? ''
  }
}

/** Whether the platform is reachable at all. Used by the health check. */
export async function platformIsReachable(): Promise<boolean> {
  try {
    const response = await fetch(`${API_URL}/internal/health`, {
      cache: 'no-store',
      signal: AbortSignal.timeout(3000),
    })
    return response.ok
  } catch {
    return false
  }
}
