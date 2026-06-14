import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Calendar, CalendarEvent, NewCalendarEvent } from '@/shared/types'

export type EventUpdate = Omit<NewCalendarEvent, 'calendar_id'>

export function useCalendars() {
  return useQuery({
    queryKey: ['calendars'],
    queryFn: () => apiGet<Calendar[]>('/calendars'),
  })
}

export function useCreateCalendar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: { name: string; color?: string; account_id?: string | null }) =>
      apiPost<Calendar>('/calendars', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['calendars'] }),
  })
}

export function useEvents(from?: string, to?: string) {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  const qs = params.toString()
  return useQuery({
    queryKey: ['events', from, to],
    queryFn: () => apiGet<CalendarEvent[]>(`/calendar/events${qs ? `?${qs}` : ''}`),
  })
}

export function useCreateEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: NewCalendarEvent) => apiPost<{ id: string }>('/calendar/events', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

export function useUpdateEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: EventUpdate }) =>
      apiPut<{ id: string }>(`/calendar/events/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
  })
}

export function useDeleteEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/calendar/events/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['events'] }),
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
