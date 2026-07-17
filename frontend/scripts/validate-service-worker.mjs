import { readFile } from 'node:fs/promises'

const worker = await readFile(new URL('../dist/sw.js', import.meta.url), 'utf8')
const apiCacheMentions = worker.match(/mailquill-api/g)?.length ?? 0

if (worker.includes('networkTimeoutSeconds')) {
  throw new Error('generated service worker must not contain NetworkFirst API caching')
}
if (apiCacheMentions !== 1 || !worker.includes('caches.delete')) {
  throw new Error('generated service worker must only mention mailquill-api for obsolete-cache cleanup')
}
if (!worker.includes('mailquill-static') || !worker.includes('self.location.origin')) {
  throw new Error('generated service worker must retain same-origin static asset caching')
}

console.log('service-worker policy validation passed')
