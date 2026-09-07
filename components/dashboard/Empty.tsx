interface EmptyProps {
  title: string
  body: string
  children?: React.ReactNode
}

/**
 * What a section looks like before anything is in it.
 *
 * A sentence about what the thing is for, not "No items found". On first use
 * every list here is empty, and an empty list is the worst moment to say
 * nothing useful.
 */
export function Empty({ title, body, children }: EmptyProps) {
  return (
    <div className="bg-bg-2 border border-white/[0.06] rounded-lg p-10 text-center">
      <h3 className="font-display text-2xl text-white mb-3">{title}</h3>
      <p className="text-white-60 leading-relaxed max-w-md mx-auto mb-6">{body}</p>
      {children}
    </div>
  )
}
