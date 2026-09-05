import { redirect } from '@/i18n/navigation'

/**
 * `redirect`, with its type written down.
 *
 * next-intl already declares it as returning `never`, but TypeScript only uses
 * that to narrow control flow when the thing being called has an *explicit*
 * type annotation at its declaration. `createNavigation` returns an object
 * whose members are inferred, so at every call site the code after a redirect
 * still looks reachable and every later use of a value reads as possibly null.
 *
 * Annotating it here once is the whole fix. Guards can then be written as they
 * read — redirect and stop — instead of with a non-null assertion on each line
 * that follows.
 */
export const redirectTo: (args: { href: string; locale: string }) => never =
  redirect
