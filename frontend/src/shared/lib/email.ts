const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

/**
 * Light syntactic check for an email address (`local@domain.tld`, no whitespace) used by forms before sending a request.
 * @param value - User input; surrounding whitespace is ignored.
 * @returns `true` when the trimmed value looks like an address.
 */
export function isValidEmail(value: string): boolean {
  return EMAIL_RE.test(value.trim())
}
