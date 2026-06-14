import { QueryClient } from '@tanstack/react-query'
import { ApiError } from '@/shared/api'

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (failureCount, error) => {
        // 4xx (401/403/404 …) won't succeed on retry — fail fast instead of
        // hammering the endpoint and spamming the console with repeats.
        if (error instanceof ApiError && error.status >= 400 && error.status < 500) return false
        return failureCount < 2
      },
    },
  },
})
