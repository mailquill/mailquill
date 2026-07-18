import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Calendar, CalendarAccount, CalendarEvent, MeetingInvitation, NewCalendarEvent } from '@/shared/types'

export type EventUpdate = Omit<NewCalendarEvent, 'calendar_id'>

export interface DiscoveredCaldavCalendar {
  name: string
  url: string
  color?: string | null
}

export interface CaldavDiscoverResponse {
  url: string
  calendars: DiscoveredCaldavCalendar[]
}

export function useCalendars() {
  return useQuery({
    queryKey: ['calendars'],
    queryFn: () => apiGet<Calendar[]>('/calendars'),
  })
}

export function useCreateCalendar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: { name: string; color?: string; account_id?: string | null; dav_url?: string | null }) =>
      apiPost<Calendar>('/calendars', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['calendars'] }),
  })
}

export function useCalendarAccounts() {
  return useQuery({
    queryKey: ['calendar-accounts'],
    queryFn: () => apiGet<CalendarAccount[]>('/calendar-accounts'),
  })
}

export function useCreateCalendarAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: {
      display_name: string
      type: 'caldav' | 'graph' | 'google' | 'openxchange'
      base_url?: string | null
      auth_scheme?: 'basic' | 'oauth2'
      username?: string | null
      password?: string | null
      access_token?: string | null
      refresh_token?: string | null
      accept_invalid_tls?: boolean
      tls_decision?: 'accept' | 'accept_always'
      sync_interval_secs?: number
    }) => apiPost<CalendarAccount>('/calendar-accounts', data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['calendar-accounts'] })
      qc.invalidateQueries({ queryKey: ['calendars'] })
      qc.invalidateQueries({ queryKey: ['events'] })
    },
  })
}

export function useDeleteCalendarAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/calendar-accounts/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['calendar-accounts'] })
      qc.invalidateQueries({ queryKey: ['calendars'] })
      qc.invalidateQueries({ queryKey: ['events'] })
    },
  })
}

export function useSyncCalendarAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      apiPost<{ status: string; last_synced_at: string | null; error: string | null }>(
        `/calendar-accounts/${id}`,
      ),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['calendar-accounts'] })
      qc.invalidateQueries({ queryKey: ['calendars'] })
      qc.invalidateQueries({ queryKey: ['events'] })
    },
  })
}

/** Resolve an account's CalDAV calendar collections (RFC 6764 discovery). */
export function useCaldavDiscover() {
  return useMutation({
    mutationFn: (input: string | { accountId: string; acceptInvalidTls?: boolean; tlsDecision?: 'accept' | 'accept_always' }) => {
      const accountId = typeof input === 'string' ? input : input.accountId
      const acceptInvalidTls = typeof input === 'string' ? false : Boolean(input.acceptInvalidTls)
      const tlsDecision = typeof input === 'string' ? undefined : input.tlsDecision
      const params = new URLSearchParams()
      if (acceptInvalidTls) params.set('accept_invalid_tls', 'true')
      if (tlsDecision) params.set('tls_decision', tlsDecision)
      const qs = params.size ? `?${params}` : ''
      return apiGet<CaldavDiscoverResponse>(`/accounts/${accountId}/caldav-discover${qs}`)
    },
  })
}

export function useEvents(from?: string, to?: string) {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  const qs = params.toString()
  return useQuery({
    queryKey: ['events', from, to],
    queryFn: () => apiGet<CalendarEvent[]>(`/calendar-events${qs ? `?${qs}` : ''}`),
  })
}

export function useCreateEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: NewCalendarEvent) => apiPost<{ id: string }>('/calendar-events', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

export function useUpdateEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: EventUpdate }) =>
      apiPut<{ id: string }>(`/calendar-events/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

export function useDeleteEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/calendar-events/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

export function useMeetingInvitations(messageId?: string) {
  return useQuery({
    queryKey: ['meeting-invitations', messageId],
    queryFn: () => apiGet<MeetingInvitation[]>(`/meeting-invitations${messageId ? `?message_id=${encodeURIComponent(messageId)}` : ''}`),
    enabled: messageId !== undefined,
  })
}

export function useRsvpInvitation() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, response }: { id: string; response: 'accepted' | 'tentative' | 'declined' }) =>
      apiPost<{ id: string; status: string }>(`/meeting-invitations/${id}/rsvp`, { response }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['meeting-invitations'] })
      qc.invalidateQueries({ queryKey: ['events'] })
    },
  })
}

export function useUpdateCalendar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...data }: { id: string; name?: string; color?: string }) =>
      apiPut<Calendar>(`/calendars/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['calendars'] }),
  })
}

export function useDeleteCalendar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/calendars/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['calendars'] })
      qc.invalidateQueries({ queryKey: ['events'] })
    },
  })
}
