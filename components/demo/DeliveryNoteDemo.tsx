'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'
import { OCR_DEMO_URL } from '@/lib/ocr-demo'

type OcrPage = { page: number; markdown: string; ms: number }
type OcrResult = { model: string; pages: OcrPage[]; total_ms: number }
type LineItem = {
  description: string
  qty?: number
  qty_raw?: string
  net_qty?: number
  unit?: string
  amount?: number
}
type Extracted = {
  doc_type?: string
  doc_no?: string
  date?: string
  site?: string
  receiver?: string
  tax_ids?: string[]
  line_items?: LineItem[]
  totals?: { subtotal?: number; vat?: number; grand_total?: number }
  needs_review?: boolean
  review_reasons?: string[]
  checks?: Record<string, boolean>
}

type Sample = { name: string; label: string }

const CHECK_NAMES: Record<string, string> = {
  items_match_subtotal: 'ผลรวมรายการตรงกับยอดรวมเป็นเงิน',
  vat_is_7pct: 'ภาษีมูลค่าเพิ่มเท่ากับ 7%',
  total_adds_up: 'ยอดรวม + ภาษี ตรงกับยอดสุทธิ',
  has_doc_no: 'อ่านเลขที่เอกสารได้',
  has_date: 'อ่านวันที่ได้',
  date_plausible: 'วันที่อยู่ในช่วงที่สมเหตุสมผล',
  has_items: 'พบตารางรายการ',
  rows_sequential: 'ลำดับแถวต่อเนื่อง',
  all_rows_have_qty: 'ทุกแถวมีจำนวน',
  all_rows_have_unit: 'ทุกแถวมีหน่วย',
  units_recognized: 'หน่วยเป็นหน่วยที่รู้จัก',
  no_ambiguous_qty: 'ไม่มีจำนวนที่กำกวม',
}

const DOC_TYPES: Record<string, string> = {
  invoice_like: 'ใบกำกับภาษี / ใบเสนอราคา',
  delivery_note: 'ใบส่งของ',
  text: 'เอกสารข้อความ',
  unknown: 'ไม่ทราบชนิด',
}

function money(n: number) {
  return n.toLocaleString('th-TH', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })
}

function toBase64(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = String(reader.result ?? '')
      resolve(result.split(',')[1] ?? '')
    }
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
  })
}

export function DeliveryNoteDemo() {
  const t = useTranslations('demo')
  const inputRef = useRef<HTMLInputElement>(null)
  const [privacy, setPrivacy] = useState(t('privacy_checking'))
  const [privacyError, setPrivacyError] = useState(false)
  const [samples, setSamples] = useState<Sample[]>([])
  const [over, setOver] = useState(false)
  const [status, setStatus] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [ocr, setOcr] = useState<OcrResult | null>(null)
  const [doc, setDoc] = useState<Extracted | null>(null)
  const [fileMeta, setFileMeta] = useState<{ name: string; kb: string; ms: number } | null>(
    null,
  )

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const health = await (await fetch(`${OCR_DEMO_URL}/health`)).json()
        if (cancelled) return
        setPrivacy(
          health.on_this_machine
            ? t('privacy_local', { model: health.model })
            : t('privacy_api', { model: health.model }),
        )
        setPrivacyError(false)
      } catch {
        if (!cancelled) {
          setPrivacy(t('privacy_down'))
          setPrivacyError(true)
        }
      }
      try {
        const found = (await (await fetch(`${OCR_DEMO_URL}/samples`)).json()) as Sample[]
        if (!cancelled && Array.isArray(found)) setSamples(found)
      } catch {
        /* samples are a convenience */
      }
    })()
    return () => {
      cancelled = true
    }
  }, [t])

  const run = useCallback(
    async (file: File) => {
      setBusy(true)
      setError(null)
      setOcr(null)
      setDoc(null)
      setFileMeta(null)
      setStatus(t('reading', { name: file.name }))
      const started = Date.now()
      const isPdf = /\.pdf$/i.test(file.name) || file.type === 'application/pdf'

      let ocrBody: OcrResult
      try {
        const payload = isPdf
          ? { pdf_b64: await toBase64(file) }
          : { image_b64: await toBase64(file) }
        const response = await fetch(`${OCR_DEMO_URL}/ocr`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        })
        const body = await response.json()
        if (!response.ok) {
          const detail =
            typeof body.detail === 'string' ? body.detail : JSON.stringify(body.detail ?? body)
          setError(
            response.status === 422
              ? t('too_small', { detail })
              : t('ocr_fail', { status: response.status, detail }),
          )
          setStatus(null)
          setBusy(false)
          return
        }
        ocrBody = body
      } catch (cause) {
        setError(t('ocr_down', { message: cause instanceof Error ? cause.message : '' }))
        setStatus(null)
        setBusy(false)
        return
      }

      setStatus(t('extracting'))
      const markdown = ocrBody.pages.map((page) => page.markdown).join('\n\n')
      let extracted: Extracted | null = null
      try {
        const response = await fetch(`${OCR_DEMO_URL}/extract`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ markdown }),
        })
        if (response.ok) extracted = await response.json()
      } catch {
        /* text still stands */
      }

      setOcr(ocrBody)
      setDoc(extracted)
      setFileMeta({
        name: file.name,
        kb: (file.size / 1024).toFixed(0),
        ms: Date.now() - started,
      })
      setStatus(null)
      setBusy(false)
    },
    [t],
  )

  useEffect(() => {
    function onPaste(event: ClipboardEvent) {
      const file = event.clipboardData?.files[0]
      if (file) void run(file)
    }
    document.addEventListener('paste', onPaste)
    return () => document.removeEventListener('paste', onPaste)
  }, [run])

  async function runSample(name: string) {
    const blob = await (await fetch(`${OCR_DEMO_URL}/samples/${name}`)).blob()
    await run(new File([blob], name, { type: blob.type }))
  }

  const hasMoney = doc?.line_items?.some((item) => item.amount != null) ?? false
  const kv = doc
    ? (
        [
          [t('kind'), DOC_TYPES[doc.doc_type ?? ''] || doc.doc_type],
          [t('doc_no'), doc.doc_no],
          [t('date'), doc.date],
          [t('site'), doc.site],
          [t('receiver'), doc.receiver],
          [t('tax_ids'), doc.tax_ids?.join(', ')],
        ] as [string, string | undefined][]
      ).filter(([, value]) => value)
    : []

  return (
    <section className="py-16 relative overflow-hidden">
        <div className="absolute inset-0 dot-grid opacity-20 pointer-events-none" />
        <div className="mx-auto max-w-5xl px-6 lg:px-8 relative z-10">
          <FadeUp>
            <div className="flex flex-wrap items-baseline justify-between gap-3 mb-6">
              <div className="flex items-center gap-3">
                <span className="w-8 h-[1px] bg-gold" />
                <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('label')}
                </p>
              </div>
              <p className={`text-sm ${privacyError ? 'text-[#B3402E]' : 'text-gold'}`}>
                {privacy}
              </p>
            </div>

            <h1 className="font-display text-[clamp(32px,5vw,52px)] leading-[1.12] text-white">
              {t('headline')}
            </h1>
            <p className="text-white-60 text-lg leading-relaxed max-w-2xl mt-6">{t('lede')}</p>
          </FadeUp>

          <div
            role="button"
            tabIndex={0}
            aria-label={t('drop_title')}
            onClick={() => inputRef.current?.click()}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault()
                inputRef.current?.click()
              }
            }}
            onDragEnter={(event) => {
              event.preventDefault()
              setOver(true)
            }}
            onDragOver={(event) => {
              event.preventDefault()
              setOver(true)
            }}
            onDragLeave={(event) => {
              event.preventDefault()
              setOver(false)
            }}
            onDrop={(event) => {
              event.preventDefault()
              setOver(false)
              const file = event.dataTransfer.files[0]
              if (file) void run(file)
            }}
            className={`mt-10 cursor-pointer rounded-lg border-2 border-dashed border-gold px-6 py-12 text-center transition-colors ${
              over ? 'bg-gold/20' : 'bg-gold-dim'
            } ${busy ? 'pointer-events-none opacity-70' : ''}`}
          >
            <strong className="block text-lg text-white mb-2">{t('drop_title')}</strong>
            <span className="text-sm text-white-60">{t('drop_hint')}</span>
          </div>
          <input
            ref={inputRef}
            type="file"
            accept=".png,.jpg,.jpeg,.webp,.pdf"
            className="hidden"
            onChange={(event) => {
              const file = event.target.files?.[0]
              if (file) void run(file)
              event.target.value = ''
            }}
          />

          {samples.length > 0 && (
            <p className="mt-4 text-sm text-white-60">
              {t('samples_intro')}{' '}
              {samples.map((sample) => (
                <button
                  key={sample.name}
                  type="button"
                  disabled={busy}
                  onClick={() => void runSample(sample.name)}
                  className="mr-2 mt-2 rounded border border-white/[0.12] px-3 py-1.5 text-sm text-white hover:border-gold hover:text-gold disabled:opacity-50"
                >
                  {sample.label}
                </button>
              ))}
            </p>
          )}

          {(status || error) && (
            <div className="mt-8">
              {status && !error && (
                <div className="h-[3px] overflow-hidden rounded bg-white/[0.08]">
                  <i className="block h-full w-1/3 bg-gold animate-pulse" />
                </div>
              )}
              <p className={`mt-3 text-sm ${error ? 'text-[#B3402E]' : 'text-white-60'}`}>
                {error ?? status}
              </p>
            </div>
          )}

          {ocr && fileMeta && (
            <div className="mt-12 space-y-6">
              <Verdict doc={doc} t={t} />
              <div className="grid gap-6 lg:grid-cols-2">
                <div className="space-y-6">
                  <Card title={t('fields')}>
                    {kv.length > 0 && (
                      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[15px]">
                        {kv.map(([label, value]) => (
                          <span key={label} className="contents">
                            <dt className="text-white-30">{label}</dt>
                            <dd className="m-0">{value}</dd>
                          </span>
                        ))}
                      </dl>
                    )}
                    {doc?.line_items?.length ? (
                      <table className="mt-4 w-full text-sm border-collapse">
                        <thead>
                          <tr>
                            <th className="text-start text-xs tracking-wider text-white-30 font-semibold pb-2">
                              {t('item')}
                            </th>
                            <th className="text-end text-xs tracking-wider text-white-30 font-semibold pb-2">
                              {t('qty')}
                            </th>
                            <th className="text-end text-xs tracking-wider text-white-30 font-semibold pb-2">
                              {hasMoney ? t('amount') : t('unit')}
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {doc.line_items.map((item, index) => {
                            const flagged =
                              item.net_qty != null || (item.qty == null && item.qty_raw)
                            const qty =
                              item.qty != null
                                ? item.qty.toLocaleString('th-TH')
                                : (item.qty_raw ?? '—')
                            return (
                              <tr
                                key={`${item.description}-${index}`}
                                className={flagged ? 'bg-[#B3402E]/[0.06]' : undefined}
                              >
                                <td className="py-1.5 pr-2 align-top">{item.description}</td>
                                <td className="py-1.5 text-end tabular-nums whitespace-nowrap">
                                  {qty}
                                  {item.net_qty != null && (
                                    <span className="text-[#B3402E]"> → {item.net_qty}</span>
                                  )}
                                </td>
                                <td className="py-1.5 text-end">
                                  {hasMoney
                                    ? item.amount != null
                                      ? money(item.amount)
                                      : '—'
                                    : (item.unit ?? '—')}
                                </td>
                              </tr>
                            )
                          })}
                        </tbody>
                      </table>
                    ) : null}
                    {(() => {
                      const totals = doc?.totals ?? {}
                      const rows = [
                        [t('subtotal'), totals.subtotal],
                        [t('vat'), totals.vat],
                        [t('grand_total'), totals.grand_total],
                      ].filter(([, value]) => value != null) as [string, number][]
                      if (!rows.length) return null
                      return (
                        <table className="mt-3 w-full text-sm">
                          <tbody>
                            {rows.map(([label, value]) => (
                              <tr key={label}>
                                <td className="py-1">{label}</td>
                                <td className="py-1 text-end tabular-nums">{money(value)}</td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      )
                    })()}
                    {!kv.length && !doc?.line_items?.length && (
                      <span className="text-white-30">{t('no_fields')}</span>
                    )}
                  </Card>
                  <Card title={t('checks')}>
                    {doc?.checks && Object.keys(doc.checks).length > 0 ? (
                      Object.entries(doc.checks).map(([key, ok]) => (
                        <div key={key} className="flex gap-2 items-baseline py-0.5 text-[14.5px]">
                          <span
                            className={`font-mono font-bold ${ok ? 'text-[#2E7D4F]' : 'text-[#B3402E]'}`}
                          >
                            {ok ? '✓' : '✕'}
                          </span>
                          <span className="text-white-60">{CHECK_NAMES[key] || key}</span>
                        </div>
                      ))
                    ) : (
                      <span className="text-white-30">{t('no_checks')}</span>
                    )}
                  </Card>
                </div>
                <Card title={t('raw')}>
                  <div className="max-h-[460px] overflow-auto">
                    {ocr.pages.map((page) => (
                      <div key={page.page}>
                        <div className="font-mono text-[11px] tracking-widest text-gold mt-3 first:mt-0 mb-1">
                          {t('page')} {page.page}
                        </div>
                        <pre className="m-0 font-mono text-[12.5px] leading-relaxed whitespace-pre-wrap break-words">
                          {page.markdown}
                        </pre>
                      </div>
                    ))}
                  </div>
                </Card>
              </div>
              <p className="text-sm text-white-30">
                {fileMeta.name} · {fileMeta.kb} KB · {ocr.pages.length} {t('page')} ·{' '}
                {t('elapsed', { seconds: (fileMeta.ms / 1000).toFixed(0) })} · {ocr.model}
              </p>
            </div>
          )}

          <p className="mt-12 text-sm text-white-30 leading-relaxed max-w-2xl">{t('footnote')}</p>
          <div className="mt-8 flex flex-wrap gap-4">
            <Link
              href="/signup"
              className="bg-gold text-bg font-medium text-sm tracking-wide px-7 py-3.5 rounded-md hover:bg-gold-light transition-colors"
            >
              {t('cta_signup')}
            </Link>
            <Link
              href="/contact"
              className="text-sm tracking-wide px-7 py-3.5 rounded-md border border-white/[0.12] text-white-60 hover:text-white hover:border-white/[0.24] transition-colors"
            >
              {t('cta_contact')}
            </Link>
          </div>
        </div>
    </section>
  )
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-bg-2 border border-white/[0.08] rounded-lg overflow-hidden">
      <h3 className="m-0 px-5 py-3 text-[13px] tracking-[0.08em] border-b border-white/[0.08] text-white-60 font-semibold">
        {title}
      </h3>
      <div className="px-5 py-4">{children}</div>
    </div>
  )
}

function Verdict({
  doc,
  t,
}: {
  doc: Extracted | null
  t: ReturnType<typeof useTranslations>
}) {
  if (!doc) {
    return (
      <div className="rounded-lg border border-white/[0.08] bg-bg-2 p-5">
        <h2 className="text-lg font-semibold m-0">{t('verdict_text')}</h2>
        <p className="text-white-60 mt-1 mb-0">{t('verdict_text_body')}</p>
      </div>
    )
  }
  if (doc.needs_review) {
    return (
      <div className="rounded-lg border border-[#8A6D2A] bg-bg-2 p-5">
        <h2 className="text-lg font-semibold m-0 text-[#8A6D2A]">{t('verdict_review')}</h2>
        <p className="text-white-60 mt-1">{t('verdict_review_body')}</p>
        {doc.review_reasons?.length ? (
          <ul className="mt-2 mb-0 text-white-60 text-[15px]">
            {doc.review_reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        ) : null}
      </div>
    )
  }
  return (
    <div className="rounded-lg border border-[#2E7D4F] bg-bg-2 p-5">
      <h2 className="text-lg font-semibold m-0 text-[#2E7D4F]">{t('verdict_pass')}</h2>
      <p className="text-white-60 mt-1 mb-0">{t('verdict_pass_body')}</p>
    </div>
  )
}
