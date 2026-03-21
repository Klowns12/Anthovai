'use client'

import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { useState } from 'react'
import { Button } from '@/components/ui/Button'
import { Mail, MapPin, Clock } from 'lucide-react'

export default function ContactPage() {
  const t = useTranslations('contact_page')
  const [status, setStatus] = useState<'idle' | 'loading' | 'success' | 'error'>('idle')

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setStatus('loading')
    
    const form = e.currentTarget
    
    const data = {
      name: (form.querySelector('#name') as HTMLInputElement)?.value || '',
      company: (form.querySelector('#company') as HTMLInputElement)?.value || '',
      email: (form.querySelector('#email') as HTMLInputElement)?.value || '',
      subject: (form.querySelector('#subject') as HTMLSelectElement)?.value || '',
      message: (form.querySelector('#message') as HTMLTextAreaElement)?.value || '',
    }

    try {
      const res = await fetch('/api/contact', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data)
      })
      const result = await res.json()

      if (res.ok && result.success) {
        setStatus('success')
        form.reset()
      } else {
        console.error('Mail error:', result.error)
        setStatus('error')
      }
    } catch (err) {
      console.error('Fetch error:', err)
      setStatus('error')
    }
  }

  const subjects = ['general', 'project', 'partnership', 'careers', 'other'] as const

  return (
    <main className="pt-32 pb-24">
      {/* Hero */}
      <section className="py-24 relative overflow-hidden">
        <div className="absolute inset-0 bg-radial-gradient from-gold/[0.03] to-transparent pointer-events-none" />
        <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10">
          <FadeUp>
            <div className="flex items-center gap-3 mb-8">
              <span className="w-8 h-[1px] bg-gold" />
              <h1 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                {t('hero_label')}
              </h1>
            </div>
            <h2 className="font-display text-[clamp(48px,6vw,80px)] leading-[1.05] tracking-[-0.01em] text-white max-w-4xl mb-8">
              {t('hero_headline')}
            </h2>
            <p className="text-xl text-white-60 leading-relaxed max-w-2xl">
              {t('hero_body')}
            </p>
          </FadeUp>
        </div>
      </section>

      {/* Content */}
      <section className="py-12">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="grid grid-cols-1 lg:grid-cols-[1fr_400px] gap-16 lg:gap-24 items-start">
            
            {/* Form */}
            <FadeUp>
              <form onSubmit={handleSubmit} className="space-y-6 bg-bg-2 p-8 lg:p-10 rounded-lg border border-white/[0.06]">
                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <label htmlFor="name" className="text-sm text-white-60">{t('form.name')}</label>
                    <input 
                      type="text" 
                      id="name" 
                      required 
                      className="w-full bg-bg border border-white/[0.08] rounded-md px-4 py-3 text-white focus:outline-none focus:border-gold transition-colors"
                    />
                  </div>
                  <div className="space-y-2">
                    <label htmlFor="company" className="text-sm text-white-60">{t('form.company')}</label>
                    <input 
                      type="text" 
                      id="company" 
                      className="w-full bg-bg border border-white/[0.08] rounded-md px-4 py-3 text-white focus:outline-none focus:border-gold transition-colors"
                    />
                  </div>
                </div>

                <div className="space-y-2">
                  <label htmlFor="email" className="text-sm text-white-60">{t('form.email')}</label>
                  <input 
                    type="email" 
                    id="email" 
                    required 
                    className="w-full bg-bg border border-white/[0.08] rounded-md px-4 py-3 text-white focus:outline-none focus:border-gold transition-colors"
                  />
                </div>

                <div className="space-y-2">
                  <label htmlFor="subject" className="text-sm text-white-60">{t('form.subject')}</label>
                  <select 
                    id="subject" 
                    required 
                    className="w-full bg-bg border border-white/[0.08] rounded-md px-4 py-3 text-white focus:outline-none focus:border-gold transition-colors appearance-none"
                  >
                    {subjects.map(s => (
                      <option key={s} value={s}>{t(`form.subjects.${s}`)}</option>
                    ))}
                  </select>
                </div>

                <div className="space-y-2">
                  <label htmlFor="message" className="text-sm text-white-60">{t('form.message')}</label>
                  <textarea 
                    id="message" 
                    required 
                    rows={5}
                    className="w-full bg-bg border border-white/[0.08] rounded-md px-4 py-3 text-white focus:outline-none focus:border-gold transition-colors resize-none"
                  />
                </div>

                <Button 
                  type="submit" 
                  disabled={status === 'loading' || status === 'success'}
                  className="w-full mt-4"
                >
                  {status === 'loading' ? '...' : status === 'success' ? t('form.success') : t('form.submit')}
                </Button>

                {status === 'error' && (
                  <p className="text-red-400 text-sm mt-4 text-center">{t('form.error')}</p>
                )}
              </form>
            </FadeUp>

            {/* Info Sidebar */}
            <FadeUp delay={0.2} className="space-y-12 shrink-0">
              <div>
                <h3 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-8 border-b border-white/[0.06] pb-4">
                  {t('info.title')}
                </h3>
                
                <div className="space-y-8">
                  <div className="flex items-start gap-4">
                    <Mail size={20} className="text-gold shrink-0 mt-1" />
                    <div>
                      <p className="text-sm text-white-60 mb-1">Email</p>
                      <a href="mailto:hello@anthovai.com" className="text-lg text-white hover:text-gold transition-colors">
                        {t('info.email')}
                      </a>
                    </div>
                  </div>

                  <div className="flex items-start gap-4">
                    <MapPin size={20} className="text-gold shrink-0 mt-1" />
                    <div>
                      <p className="text-sm text-white-60 mb-1">HQ</p>
                      <p className="text-lg text-white">
                        {t('info.location')}
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-4">
                    <Clock size={20} className="text-gold shrink-0 mt-1" />
                    <div>
                      <p className="text-sm text-white-60 mb-1">Hours</p>
                      <p className="text-lg text-white">
                        {t('info.hours')}
                      </p>
                    </div>
                  </div>
                </div>
              </div>
            </FadeUp>

          </div>
        </div>
      </section>
    </main>
  )
}
