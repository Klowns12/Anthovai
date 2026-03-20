'use client'

import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '../animations/FadeUp'
import { Typewriter } from '../animations/Typewriter'
import { CodeBlock } from '../ui/CodeBlock'
import { Badge } from '../ui/Badge'
import { Zap, Brain, Shield, Layers } from 'lucide-react'

const KLANG_CODE_SAMPLE = `// Klang v1.0 — Architecture Example
import std.ai.{Model, infer}
import std.net.{HttpServer, Response}

const MODEL_PATH = "models/reasoning.kmo"

fn main() {
    // Zero-cost abstractions with Python-like readability
    let model = mut Model.load(MODEL_PATH)
    
    // Built-in AI primitives
    let server = HttpServer.new(8080)
    
    server.post("/infer", async fn(req) {
        let payload = req.json()
        
        // Native inference block
        let result = await infer {
            model: &model,
            prompt: payload.query,
            temperature: 0.2
        }
        
        return Response.json({ data: result })
    })
    
    server.listen()
}`

export function KlangSection() {
  const t = useTranslations('klang')

  return (
    <section className="relative w-full overflow-hidden bg-bg-3 py-32 border-y border-white/[0.04]">
      {/* Left accent border */}
      <div className="absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-bg-3 via-gold to-bg-3 opacity-50" />
      
      {/* Background grain + subtle glow */}
      <div className="absolute inset-0 bg-radial-gradient from-gold/[0.03] to-transparent" />

      <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10">
        <div className="grid grid-cols-1 lg:grid-cols-[60fr_40fr] gap-16 lg:gap-20 items-center">
          
          {/* Left Column (Content & Code) */}
          <FadeUp>
            <Badge className="mb-8">{t('tag')}</Badge>
            
            <h2 className="font-display text-[clamp(40px,4vw,56px)] leading-[1.05] tracking-[-0.01em] text-white mb-6">
              {t('headline')}
            </h2>
            
            <p className="text-lg leading-relaxed text-white-60 mb-12 max-w-xl">
              {t('sub')}
            </p>

            {/* Language Pillars */}
            <div className="grid grid-cols-2 gap-8 mb-12">
              <div className="flex items-start gap-4">
                <div className="w-10 h-10 rounded-full bg-bg border border-white/[0.08] flex items-center justify-center shrink-0">
                  <Zap size={18} className="text-gold" />
                </div>
                <div>
                  <h4 className="text-sm font-medium text-white mb-1">{t('pillars.speed')}</h4>
                </div>
              </div>
              <div className="flex items-start gap-4">
                <div className="w-10 h-10 rounded-full bg-bg border border-white/[0.08] flex items-center justify-center shrink-0">
                  <Layers size={18} className="text-gold" />
                </div>
                <div>
                  <h4 className="text-sm font-medium text-white mb-1">{t('pillars.readable')}</h4>
                </div>
              </div>
              <div className="flex items-start gap-4">
                <div className="w-10 h-10 rounded-full bg-bg border border-white/[0.08] flex items-center justify-center shrink-0">
                  <Shield size={18} className="text-gold" />
                </div>
                <div>
                  <h4 className="text-sm font-medium text-white mb-1">{t('pillars.safe')}</h4>
                </div>
              </div>
              <div className="flex items-start gap-4">
                <div className="w-10 h-10 rounded-full bg-bg border border-white/[0.08] flex items-center justify-center shrink-0">
                  <Brain size={18} className="text-gold" />
                </div>
                <div>
                  <h4 className="text-sm font-medium text-white mb-1">{t('pillars.ai')}</h4>
                </div>
              </div>
            </div>

            {/* CTAs */}
            <div className="flex flex-wrap items-center gap-6">
              <Link 
                href="/klang"
                className="inline-flex items-center justify-center px-6 py-3 text-sm tracking-wider uppercase border border-gold-border text-gold hover:bg-gold-dim transition-all"
              >
                {t('cta')} &rarr;
              </Link>
              <Link 
                href="/klang/docs" 
                className="text-sm font-mono text-white-60 hover:text-white transition-colors border-b border-transparent hover:border-white/30 pb-1"
              >
                klang.kkg
              </Link>
            </div>
          </FadeUp>

          {/* Right Column (Stats & Visuals) */}
          <FadeUp delay={0.2} className="relative">
            {/* Visual container with grid background */}
            <div className="bg-bg rounded-xl border border-white/[0.06] p-8 lg:p-10 relative overflow-hidden shadow-2xl">
              <div className="absolute inset-0 dot-grid opacity-50 pointer-events-none" />
              
              {/* Code snippet inside */}
              <div className="mb-10 relative z-10">
                <CodeBlock 
                  code={KLANG_CODE_SAMPLE} 
                  language="server.kkg" 
                  className="shadow-2xl border-white/[0.1] bg-[#0A0A0F]"
                />
              </div>

              {/* Vertical Stats Row */}
              <div className="grid grid-cols-2 gap-8 relative z-10 border-t border-white/[0.06] pt-8">
                <div>
                  <div className="font-display text-4xl text-white mb-2">286k+</div>
                  <div className="text-[10px] tracking-widest uppercase text-white-30">{t('stat_tests')}</div>
                </div>
                <div>
                  <div className="font-display text-4xl text-white mb-2">3</div>
                  <div className="text-[10px] tracking-widest uppercase text-white-30">{t('stat_targets')}</div>
                </div>
              </div>

              {/* Targets Pill list */}
              <div className="flex flex-wrap gap-2 mt-8 relative z-10">
                <span className="px-3 py-1 text-xs font-medium text-gold bg-gold/[0.05] border border-gold/[0.1] rounded-full">
                  {t('target_native')}
                </span>
                <span className="px-3 py-1 text-xs font-medium text-gold bg-gold/[0.05] border border-gold/[0.1] rounded-full">
                  {t('target_llvm')}
                </span>
                <span className="px-3 py-1 text-xs font-medium text-gold bg-gold/[0.05] border border-gold/[0.1] rounded-full">
                  {t('target_wasm')}
                </span>
              </div>
            </div>
          </FadeUp>

        </div>
      </div>
    </section>
  )
}
