import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach, vi } from 'vitest'
import '@/shared/i18n'

afterEach(cleanup)

// jsdom doesn't implement scrollIntoView (real browsers do).
Element.prototype.scrollIntoView = vi.fn()

Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }),
})

// Node 22+ ships its own experimental `localStorage` global that is undefined
// unless `--localstorage-file` is set, and it shadows jsdom's implementation.
// Give the tests a plain in-memory Storage so persisted-state tests keep working.
function memoryStorage(): Storage {
  const store = new Map<string, string>()
  return {
    get length() {
      return store.size
    },
    clear: () => store.clear(),
    getItem: (key) => store.get(key) ?? null,
    key: (index) => [...store.keys()][index] ?? null,
    removeItem: (key) => {
      store.delete(key)
    },
    setItem: (key, value) => {
      store.set(key, String(value))
    },
  }
}

if (typeof globalThis.localStorage?.clear !== 'function') {
  const storage = memoryStorage()
  for (const target of [globalThis, window]) {
    Object.defineProperty(target, 'localStorage', { configurable: true, value: storage })
  }
}
