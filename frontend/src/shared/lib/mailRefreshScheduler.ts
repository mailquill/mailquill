// Coalesces the "something changed, reload the mail lists" signals that arrive
// over SSE into as few list refetches as possible, without ever starving one.

/** Quiet period after the last signal before the lists reload. */
export const REFRESH_DEBOUNCE_MS = 500
/** A steady stream of signals must not postpone the reload beyond this. */
export const REFRESH_MAX_WAIT_MS = 2_000
/**
 * Minimum spacing for refreshes caused by sync *progress* alone. A backfill
 * reports progress several times a second, and every list refetch reloads
 * each loaded page of the open infinite list.
 */
export const PROGRESS_REFRESH_INTERVAL_MS = 3_000

export interface MailRefreshScheduler {
  /**
   * Ask for a refresh. Debounced, capped by the max wait, and never earlier
   * than `minInterval` after the previous run when one is given.
   */
  request(options?: { minInterval?: number }): void
  /** Drop a pending refresh (on teardown). */
  cancel(): void
}

/**
 * Create a scheduler that calls `run` at most once per burst of requests.
 * @param run - Performs the refresh.
 * @param now - Clock, injectable for tests.
 * @returns The scheduler.
 */
export function createMailRefreshScheduler(run: () => void, now: () => number = Date.now): MailRefreshScheduler {
  let timer: ReturnType<typeof setTimeout> | undefined
  let deadline = 0
  let notBefore = 0
  let lastRunAt = 0

  const fire = () => {
    timer = undefined
    deadline = 0
    notBefore = 0
    lastRunAt = now()
    run()
  }

  return {
    request(options) {
      const current = now()
      if (!timer) deadline = current + REFRESH_MAX_WAIT_MS
      const requestedNotBefore = options?.minInterval ? lastRunAt + options.minInterval : 0
      // A pending early request keeps its urgency; a throttled one never
      // delays it.
      notBefore = timer ? Math.min(notBefore, requestedNotBefore) : requestedNotBefore
      const wait = Math.max(Math.min(REFRESH_DEBOUNCE_MS, deadline - current), notBefore - current, 0)
      if (timer) clearTimeout(timer)
      timer = setTimeout(fire, wait)
    },
    cancel() {
      if (timer) clearTimeout(timer)
      timer = undefined
      deadline = 0
      notBefore = 0
    },
  }
}
