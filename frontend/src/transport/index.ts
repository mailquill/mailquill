import { http } from './http'
import { tauri } from './tauri'

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

export const transport = isTauri() ? tauri : http
