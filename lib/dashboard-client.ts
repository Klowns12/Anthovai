'use client'

/**
 * The browser's half of the conversation.
 *
 * Everything goes to `/api/dashboard/*` — same origin, so the session cookie
 * travels on its own and there is no CORS, no `credentials: 'include'`, and no
 * platform address in the bundle.
 */

export interface ApiFailure {
  status: number
  /** The stable code to branch on: `email_taken`, `slug_taken`, and so on. */
  code: string
  message: string
  requestId: string
}

export class DashboardError extends Error implements ApiFailure {
  readonly status: number
  readonly code: string
  readonly requestId: string

  constructor(failure: ApiFailure) {
    super(failure.message)
    this.name = 'DashboardError'
    this.status = failure.status
    this.code = failure.code
    this.requestId = failure.requestId
  }
}

export async function call<T>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const isForm = init.body instanceof FormData

  const response = await fetch(`/api/dashboard${path}`, {
    ...init,
    headers: {
      // `fetch` sets its own `content-type` for FormData, complete with the
      // multipart boundary. Setting one here would overwrite it and the upload
      // would arrive unparseable.
      ...(isForm ? {} : { 'content-type': 'application/json' }),
      ...init.headers,
    },
  })

  if (response.status === 204) return undefined as T

  const body = await response.json().catch(() => null)

  if (!response.ok) {
    throw new DashboardError({
      status: response.status,
      code: body?.error?.code ?? 'unknown_error',
      message: body?.error?.message ?? `Request failed (${response.status}).`,
      requestId: body?.error?.request_id ?? '',
    })
  }

  return body as T
}

export const api = {
  get: <T>(path: string) => call<T>(path),
  post: <T>(path: string, body?: unknown) =>
    call<T>(path, {
      method: 'POST',
      body: body instanceof FormData ? body : JSON.stringify(body ?? {}),
    }),
  patch: <T>(path: string, body: unknown) =>
    call<T>(path, { method: 'PATCH', body: JSON.stringify(body) }),
  put: <T>(path: string, body: unknown) =>
    call<T>(path, { method: 'PUT', body: JSON.stringify(body) }),
  delete: <T>(path: string) => call<T>(path, { method: 'DELETE' }),
}

/**
 * Wording for the failures a customer can actually do something about.
 *
 * Covers two sources with one map: the codes an endpoint returns, and the
 * `error_code` recorded on a document whose ingestion failed. They are the same
 * vocabulary by design, which is why one map is enough.
 *
 * The platform's own messages are written for whoever is integrating against
 * it — precise, and not always what someone signing up wants to read. Codes
 * without an entry here fall through to that message, which is better than a
 * generic apology that says nothing.
 */
const WORDING: Record<string, string> = {
  email_taken: 'That email already has an account. Sign in instead.',
  invalid_credentials: 'That email and password do not match.',
  email_not_verified: 'Confirm your email address before creating an API key.',
  slug_taken: 'That address is taken. Try another.',
  slug_reserved: 'That address is reserved. Try another.',
  session_expired: 'Your session has expired. Sign in again.',
  rate_limited: 'Too many attempts. Wait a minute and try again.',
  agent_limit_reached: 'This plan has reached its limit of agents.',
  document_limit_reached: 'This knowledge base is full for this plan.',
  storage_limit_reached: 'This plan has reached its storage limit.',
  quota_exceeded: 'This month’s message allowance is spent.',
  file_too_large: 'That file is larger than this plan allows.',
  unsupported_file_type: 'That file type cannot be read.',
  url_not_allowed: 'That address cannot be fetched.',
  no_extractable_text: 'No readable text could be found in that file.',
  platform_unreachable:
    'The platform is not answering. If you are running it locally, check that it is started.',
}

export function explain(error: unknown): string {
  if (error instanceof DashboardError) {
    return WORDING[error.code] ?? error.message
  }
  return 'Something went wrong. Try again.'
}
