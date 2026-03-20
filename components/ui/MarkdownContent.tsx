'use client'

import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { CodeBlock } from './CodeBlock'

interface MarkdownContentProps {
  content: string
}

export function MarkdownContent({ content }: MarkdownContentProps) {
  return (
    <div className="prose prose-invert prose-p:text-white-60 prose-headings:text-white prose-headings:font-medium prose-a:text-gold hover:prose-a:text-gold-light prose-strong:text-white prose-code:text-gold-light prose-code:bg-gold/[0.05] prose-code:px-1.5 prose-code:py-0.5 prose-code:rounded-sm prose-pre:bg-transparent prose-pre:p-0 prose-pre:m-0 prose-li:text-white-60 max-w-none">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ node, inline, className, children, ...props }: any) {
            const match = /language-(\w+)/.exec(className || '')
            const language = match ? match[1] : ''
            
            if (!inline && language) {
              return (
                <div className="my-6">
                  <CodeBlock 
                    code={String(children).replace(/\n$/, '')} 
                    language={language} 
                  />
                </div>
              )
            }
            return (
              <code className={className} {...props}>
                {children}
              </code>
            )
          }
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
}
