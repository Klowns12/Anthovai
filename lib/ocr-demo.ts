/**
 * The costing OCR demo talks to the sidecar directly from the browser.
 * A page takes about a minute, longer than a Vercel function may wait.
 */
export const OCR_DEMO_URL = (
  process.env.NEXT_PUBLIC_OCR_DEMO_URL ??
  (process.env.NODE_ENV === 'development'
    ? 'http://127.0.0.1:9090'
    : 'https://demo.152.42.177.130.sslip.io')
).replace(/\/$/, '')
