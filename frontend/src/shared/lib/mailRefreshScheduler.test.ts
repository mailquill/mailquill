import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  PROGRESS_REFRESH_INTERVAL_MS,
  REFRESH_DEBOUNCE_MS,
  REFRESH_MAX_WAIT_MS,
  createMailRefreshScheduler,
} from './mailRefreshScheduler'

beforeEach(() => vi.useFakeTimers())
afterEach(() => vi.useRealTimers())

describe('createMailRefreshScheduler', () => {
  it('debounces a burst into one run', () => {
    const run = vi.fn()
    const scheduler = createMailRefreshScheduler(run)
    scheduler.request()
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS - 100)
    scheduler.request()
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS - 100)
    expect(run).not.toHaveBeenCalled()
    vi.advanceTimersByTime(100)
    expect(run).toHaveBeenCalledTimes(1)
  })

  it('runs within the max wait even while requests keep arriving', () => {
    const run = vi.fn()
    const scheduler = createMailRefreshScheduler(run)
    for (let elapsed = 0; elapsed <= REFRESH_MAX_WAIT_MS; elapsed += 100) {
      scheduler.request()
      vi.advanceTimersByTime(100)
    }
    expect(run).toHaveBeenCalledTimes(1)
  })

  it('spaces progress-only refreshes by their minimum interval', () => {
    const run = vi.fn()
    const scheduler = createMailRefreshScheduler(run)
    scheduler.request()
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS)
    expect(run).toHaveBeenCalledTimes(1)

    // Requested right after a run: fires once the interval since that run is up.
    scheduler.request({ minInterval: PROGRESS_REFRESH_INTERVAL_MS })
    vi.advanceTimersByTime(PROGRESS_REFRESH_INTERVAL_MS - 1)
    expect(run).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(1)
    expect(run).toHaveBeenCalledTimes(2)
  })

  it('lets an urgent request override a pending throttled one', () => {
    const run = vi.fn()
    const scheduler = createMailRefreshScheduler(run)
    scheduler.request()
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS)
    scheduler.request({ minInterval: PROGRESS_REFRESH_INTERVAL_MS })
    scheduler.request()
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS)
    expect(run).toHaveBeenCalledTimes(2)
  })

  it('cancel drops the pending run', () => {
    const run = vi.fn()
    const scheduler = createMailRefreshScheduler(run)
    scheduler.request()
    scheduler.cancel()
    vi.advanceTimersByTime(REFRESH_MAX_WAIT_MS)
    expect(run).not.toHaveBeenCalled()
  })
})
