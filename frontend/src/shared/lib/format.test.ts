import { afterEach, describe, expect, it, vi } from 'vitest'
import { formatDate, listIdToName, parseFromAddr } from './format'

afterEach(() => vi.useRealTimers())

describe('formatDate', () => {
  it('returns an empty string for missing or invalid dates instead of throwing', () => {
    expect(formatDate(null)).toBe('')
    expect(formatDate('')).toBe('')
    expect(formatDate('not a date')).toBe('')
  })

  it('shows a time for today and a day for the rest of the week', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-03-18T15:00:00Z'))
    expect(formatDate('2026-03-18T09:30:00Z')).toMatch(/\d/)
    expect(formatDate('2026-03-18T09:30:00Z')).not.toBe(formatDate('2026-03-16T09:30:00Z'))
    expect(formatDate('2026-01-02T09:30:00Z')).toMatch(/2/)
  })
})

describe('parseFromAddr', () => {
  it('splits a display name from the address and falls back to the raw value', () => {
    expect(parseFromAddr('Alice Example <alice@example.test>')).toEqual({
      name: 'Alice Example',
      email: 'alice@example.test',
    })
    expect(parseFromAddr('bob@example.test')).toEqual({ name: 'bob@example.test', email: 'bob@example.test' })
  })
})

describe('listIdToName', () => {
  it('is empty for messages without a list id', () => {
    expect(listIdToName(null)).toBeNull()
    expect(listIdToName(undefined)).toBeNull()
  })
})
