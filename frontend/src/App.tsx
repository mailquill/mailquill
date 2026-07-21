import { lazy } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { LoginPage } from '@/pages/LoginPage'
import { RegisterPage } from '@/pages/RegisterPage'
import { MailLayout } from '@/pages/MailLayout'
import { ProtectedRoute } from '@/app/ProtectedRoute'
import { useAuthStore } from '@/app/store'

const CalendarPage = lazy(() => import('@/pages/CalendarPage').then((module) => ({ default: module.CalendarPage })))
const ContactsPage = lazy(() => import('@/pages/ContactsPage').then((module) => ({ default: module.ContactsPage })))
const MailFolderPage = lazy(() => import('@/pages/MailFolderPage').then((module) => ({ default: module.MailFolderPage })))
const SearchPage = lazy(() => import('@/pages/SearchPage').then((module) => ({ default: module.SearchPage })))
const SettingsPage = lazy(() => import('@/pages/SettingsPage').then((module) => ({ default: module.SettingsPage })))
const UnifiedMailboxPage = lazy(() => import('@/pages/UnifiedMailboxPage').then((module) => ({ default: module.UnifiedMailboxPage })))

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
        <Route path="unified/:view" element={<UnifiedMailboxPage />} />
        <Route path="contacts" element={<ContactsPage />} />
        <Route path="calendar" element={<CalendarPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="accounts" element={<SettingsPage />} />
        <Route path="search" element={<SearchPage />} />
        <Route path=":accountId/:folder" element={<MailFolderPage />} />
        <Route path=":accountId/:folder/:threadId" element={<MailFolderPage />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
