import { useEffect, useState, type ReactNode } from 'react'
import { Navigate, useLocation } from 'react-router-dom'
import { refreshAccessToken } from '@/shared/api'
import { useAuthStore } from '@/app/store'

interface ProtectedRouteProps {
  children: ReactNode
}

/**
 * Gate for the signed-in part of the app. Renders its children when an access
 * token is present; otherwise tries one silent refresh through the HttpOnly
 * refresh cookie (showing a brief "checking session" screen meanwhile) and,
 * if that fails, redirects to the login page with the attempted location in
 * router state so login can return there.
 * @param props - `children`: the protected route content.
 * @returns The children, the interim session check, or a redirect to `/login`.
 */
export function ProtectedRoute({ children }: ProtectedRouteProps) {
  const location = useLocation()
  const accessToken = useAuthStore((state) => state.accessToken)
  const [refreshAttempted, setRefreshAttempted] = useState(Boolean(accessToken))

  useEffect(() => {
    let isActive = true

    if (accessToken) {
      return
    }

    refreshAccessToken()
      .finally(() => {
        if (isActive) {
          setRefreshAttempted(true)
        }
      })

    return () => {
      isActive = false
    }
  }, [accessToken])

  if (accessToken) {
    return children
  }

  if (!refreshAttempted) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background text-sm text-muted-foreground">
        Checking session...
      </main>
    )
  }

  return <Navigate to="/login" replace state={{ from: location }} />
}
