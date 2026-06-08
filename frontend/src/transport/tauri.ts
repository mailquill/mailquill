// Tauri IPC transport — stub for v1 (Tauri desktop not in initial release)
// Will delegate to tauri::invoke() in a future change.

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
