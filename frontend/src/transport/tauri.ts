// Tauri IPC transport — stub for v1 (Tauri desktop not in initial release)
// Will delegate to tauri::invoke() in a future change.

/**
 * Placeholder transport for the Tauri desktop shell. Every method throws until
 * the IPC bridge (`tauri::invoke`) is implemented; the transport switch in
 * `./index.ts` only selects it when `window.__TAURI__` exists.
 */
export const tauri = {
  get: <T>(): Promise<T> => {
    throw new Error('Tauri transport not implemented')
  },
  post: <T>(): Promise<T> => {
    throw new Error('Tauri transport not implemented')
  },
  put: <T>(): Promise<T> => {
    throw new Error('Tauri transport not implemented')
  },
  patch: <T>(): Promise<T> => {
    throw new Error('Tauri transport not implemented')
  },
  delete: <T>(): Promise<T> => {
    throw new Error('Tauri transport not implemented')
  },
}
