'use client'

import { useState, useCallback } from 'react'
import { cn } from '@/lib/utils'
import { Copy, Check } from 'lucide-react'

interface CodeBlockProps {
  code: string
  language?: string
  showLineNumbers?: boolean
  className?: string
}

type TokenType = 'keyword' | 'type' | 'string' | 'comment' | 'number' | 'variable' | 'default'

const TOKEN_COLORS: Record<TokenType, string> = {
  keyword: 'text-gold',
  type: 'text-[#267F99]',
  string: 'text-[#A31515]',
  comment: 'text-[#008000]',
  number: 'text-[#098658]',
  variable: 'text-[#001080]',
  default: 'text-white-60',
}

const KEYWORDS = ['fn', 'let', 'mut', 'const', 'ai', 'struct', 'impl', 'match', 'return', 'if', 'else', 'for', 'while', 'use', 'pub', 'mod', 'trait', 'enum', 'type', 'import', 'export', 'async', 'await', 'model', 'infer', 'train']
const TYPES = ['int', 'string', 'bool', 'float', 'void', 'i32', 'i64', 'f32', 'f64', 'u8', 'u32', 'u64', 'String', 'Vec', 'Option', 'Result']

function tokenizeLine(line: string): Array<{ text: string; type: TokenType }> {
  const tokens: Array<{ text: string; type: TokenType }> = []

  if (line.trimStart().startsWith('//')) {
    return [{ text: line, type: 'comment' }]
  }

  const regex = /("(?:[^"\\]|\\.)*")|('(?:[^'\\]|\\.)*')|(\b\d+(?:\.\d+)?\b)|(\b[a-zA-Z_]\w*\b)|(\s+)|([^\s\w])/g
  let match: RegExpExecArray | null

  while ((match = regex.exec(line)) !== null) {
    const [fullMatch] = match

    if (match[1] || match[2]) {
      tokens.push({ text: fullMatch, type: 'string' })
    } else if (match[3]) {
      tokens.push({ text: fullMatch, type: 'number' })
    } else if (match[4]) {
      if (KEYWORDS.includes(fullMatch)) {
        tokens.push({ text: fullMatch, type: 'keyword' })
      } else if (TYPES.includes(fullMatch)) {
        tokens.push({ text: fullMatch, type: 'type' })
      } else {
        tokens.push({ text: fullMatch, type: 'variable' })
      }
    } else {
      tokens.push({ text: fullMatch, type: 'default' })
    }
  }

  return tokens
}

export function CodeBlock({
  code,
  language = '.kkg',
  showLineNumbers = true,
  className,
}: CodeBlockProps) {
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(code)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }, [code])

  const lines = code.split('\n')

  return (
    <div
      className={cn(
        'relative group rounded-lg border border-white/[0.06] bg-code-bg overflow-hidden',
        className
      )}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-white/[0.06]">
        <span className="text-[10px] font-mono tracking-wider text-white-30 uppercase">
          {language}
        </span>
        <button
          onClick={handleCopy}
          className="text-white-30 hover:text-white transition-colors p-1"
          aria-label="Copy code"
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
        </button>
      </div>

      {/* Code */}
      <pre className="overflow-x-auto p-4 text-sm leading-relaxed font-mono">
        <code>
          {lines.map((line, i) => (
            <div key={i} className="flex">
              {showLineNumbers && (
                <span className="select-none mr-6 text-white/[0.15] text-right w-6 shrink-0">
                  {i + 1}
                </span>
              )}
              <span>
                {tokenizeLine(line).map((token, j) => (
                  <span key={j} className={TOKEN_COLORS[token.type]}>
                    {token.text}
                  </span>
                ))}
              </span>
            </div>
          ))}
        </code>
      </pre>
    </div>
  )
}
