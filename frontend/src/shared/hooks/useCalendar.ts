import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Calendar, CalendarAccount, CalendarEvent, MeetingInvitation, NewCalendarEvent } from '@/shared/types'

/** Fields accepted when editing an event: everything but the calendar it lives in. */
export type EventUpdate = Omit<NewCalendarEvent, 'calendar_id'>

/**
 * One calendar collection found during CalDAV discovery.
 * @property url - Absolute URL of the collection, used as the calendar's `dav_url`.
 * @property color - Colour advertised by the server, when it sets one.
 */
export interface DiscoveredCaldavCalendar {
  name: string
  url: string
  color?: string | null
}

/**
 * Result of `GET /accounts/:id/caldav-discover`.
 * @property url - The server base URL whose discovery produced the calendars.
 * @property calendars - Collections the user may add as calendars.
 */
export interface CaldavDiscoverResponse {
  url: string
  calendars: DiscoveredCaldavCalendar[]
}

/**
 * All calendars visible to the user (local and account-backed) from
 * `GET /calendars`, cached under `['calendars']`.
 * @returns A query for the `Calendar[]`.
 */
export function useCalendars() {
  return useQuery({
    queryKey: ['calendars'],
    queryFn: () => apiGet<Calendar[]>('/calendars'),
  })
}

/**
 * Create a calendar through `POST /calendars`, either local or bound to an
 * account's CalDAV collection, then refetch `['calendars']`.
 * @returns A mutation taking `{ name, color?, account_id?, dav_url? }` and
 *   resolving to the new `Calendar`.
 */
export function useCreateCalendar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: { name: string; color?: string; account_id?: string | null; dav_url?: string | null }) =>
      apiPost<Calendar>('/calendars', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['calendars'] }),
  })
}

/**
 * Standalone calendar accounts (CalDAV, Graph, Google, Open-Xchange) from
 * `GET /calendar-accounts`, cached under `['calendar-accounts']`.
 * @returns A query for the `CalendarAccount[]`.
 */
export function useCalendarAccounts() {
  return useQuery({
    queryKey: ['calendar-accounts'],
    queryFn: () => apiGet<CalendarAccount[]>('/calendar-accounts'),
  })
}

/**
 * Connect a calendar account through `POST /calendar-accounts`, then refetch
 * calendar accounts, calendars and events since the first sync adds to all three.
 * @returns A mutation taking the connection form payload and resolving to the
 *   new `CalendarAccount`.
 */
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

/**
 * Disconnect a calendar account through `DELETE /calendar-accounts/:id`, then
 * refetch calendar accounts, calendars and events.
 * @returns A mutation taking the calendar account id.
 */
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

/**
 * Run a sync for one calendar account now through `POST /calendar-accounts/:id`,
 * then refetch calendar accounts, calendars and events.
 * @returns A mutation taking the calendar account id and resolving to the sync
 *   outcome `{ status, last_synced_at, error }`.
 */
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

/**
 * Resolve an account's CalDAV calendar collections (RFC 6764 discovery)
 * through `GET /accounts/:id/caldav-discover`. A mutation rather than a query
 * because discovery is slow and only runs on demand from the dialog.
 * @returns A mutation taking the account id, or `{ accountId, acceptInvalidTls?,
 *   tlsDecision? }` to retry past a certificate error, and resolving to the
 *   discovered calendars.
 */
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

/**
 * Events in a date range from `GET /calendar-events`, cached under
 * `['events', from, to]`. Every event, calendar and account mutation
 * invalidates the `['events']` prefix.
 * @param from - Inclusive range start as an ISO date-time; omit for no lower bound.
 * @param to - Exclusive range end as an ISO date-time; omit for no upper bound.
 * @returns A query for the `CalendarEvent[]`.
 */
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

/**
 * Create an event through `POST /calendar-events`, then refetch `['events']`.
 * @returns A mutation taking a `NewCalendarEvent` and resolving to the new `{ id }`.
 */
export function useCreateEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: NewCalendarEvent) => apiPost<{ id: string }>('/calendar-events', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

/**
 * Replace an event's fields through `PUT /calendar-events/:id`, then refetch `['events']`.
 * @returns A mutation taking `{ id, data }` and resolving to `{ id }`.
 */
export function useUpdateEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: EventUpdate }) =>
      apiPut<{ id: string }>(`/calendar-events/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

/**
 * Delete an event through `DELETE /calendar-events/:id`, then refetch `['events']`.
 * @returns A mutation taking the event id.
 */
export function useDeleteEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/calendar-events/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

/**
 * Meeting invitations extracted from mail, from `GET /meeting-invitations`,
 * cached under `['meeting-invitations', messageId]`. Pass a message id to get
 * the invitations attached to that message; the query stays disabled while
 * the id is undefined.
 * @param messageId - Message to look up, or an empty string for all invitations.
 * @returns A query for the `MeetingInvitation[]`.
 */
export function useMeetingInvitations(messageId?: string) {
  return useQuery({
    queryKey: ['meeting-invitations', messageId],
    queryFn: () => apiGet<MeetingInvitation[]>(`/meeting-invitations${messageId ? `?message_id=${encodeURIComponent(messageId)}` : ''}`),
    enabled: messageId !== undefined,
  })
}

/**
 * Answer a meeting invitation through `POST /meeting-invitations/:id/rsvp`,
 * then refetch invitations and events since accepting adds the event.
 * @returns A mutation taking `{ id, response }` and resolving to the updated `{ id, status }`.
 */
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

/**
 * Rename or recolour a calendar through `PUT /calendars/:id`, then refetch `['calendars']`.
 * @returns A mutation taking `{ id, name?, color? }` and resolving to the updated `Calendar`.
 */
export function useUpdateCalendar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...data }: { id: string; name?: string; color?: string }) =>
      apiPut<Calendar>(`/calendars/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['calendars'] }),
  })
}

/**
 * Delete a calendar and its events through `DELETE /calendars/:id`, then
 * refetch `['calendars']` and `['events']`.
 * @returns A mutation taking the calendar id.
 */
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
