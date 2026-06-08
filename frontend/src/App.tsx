import { Navigate, Route, Routes } from 'react-router-dom'
import { LoginPage } from '@/pages/LoginPage'
import { RegisterPage } from '@/pages/RegisterPage'
import { MailLayout } from '@/pages/MailLayout'
import { MailFolderPage } from '@/pages/MailFolderPage'
import { SearchPage } from '@/pages/SearchPage'
import { UnifiedMailboxPage } from '@/pages/UnifiedMailboxPage'
import { ProtectedRoute } from '@/app/ProtectedRoute'
import { useAuthStore } from '@/app/store'

function RootRedirect() {
  const accessToken = useAuthStore((state) => state.accessToken)

  return <Navigate to={accessToken ? '/mail/unified' : '/login'} replace />
}

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<RootRedirect />} />
      <Route path="/login" element={<LoginPage />} />
      <Route path="/register" element={<RegisterPage />} />
      <Route
        path="/mail"
        element={
          <ProtectedRoute>
            <MailLayout />
          </ProtectedRoute>
        }
      >
        <Route index element={<Navigate to="/mail/unified" replace />} />
        <Route path="unified" element={<UnifiedMailboxPage />} />
        <Route path="search" element={<SearchPage />} />
        <Route path=":accountId/:folder" element={<MailFolderPage />} />
        <Route path=":accountId/:folder/:threadId" element={<MailFolderPage />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
