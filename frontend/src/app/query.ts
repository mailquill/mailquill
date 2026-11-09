import { QueryClient } from '@tanstack/react-query'
import { ApiError } from '@/shared/api'

/**
 * The app-wide TanStack Query client. Queries are fresh for 30 s and retry
 * once, except on 4xx (a retry cannot succeed) and on 502/503/504 (the
 * backend or dev proxy is down; retrying every mounted query only adds noise).
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (failureCount, error) => {
        // 4xx (401/403/404 …) won't succeed on retry — fail fast instead of
        // hammering the endpoint and spamming the console with repeats.
        if (error instanceof ApiError && error.status >= 400 && error.status < 500) return false
        // Gateway errors usually mean the dev proxy/backend is unavailable.
        // Retrying every mounted query multiplies console noise without fixing
        // the underlying service state.
        if (error instanceof ApiError && [502, 503, 504].includes(error.status)) return false
        return failureCount < 2
      },
    },
  },
})
