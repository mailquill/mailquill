const CACHEABLE_STATIC_DESTINATIONS = new Set(['script', 'style', 'font', 'image'])

/**
 * Decides whether a request belongs in the service worker's static asset cache.
 * Cross-origin email images must bypass Workbox so the browser evaluates them
 * as images under `img-src`, rather than as service-worker fetches under
 * `connect-src`.
 *
 * @param request - Request metadata supplied by Workbox.
 * @param url - Parsed request URL supplied by Workbox.
 * @param serviceWorkerOrigin - Origin controlled by the service worker.
 * @returns Whether Workbox should handle the request with its static cache.
 */
export function shouldCacheStaticRequest(
  request: Pick<Request, 'destination'>,
  url: URL,
  serviceWorkerOrigin: string,
): boolean {
  return url.origin === serviceWorkerOrigin && CACHEABLE_STATIC_DESTINATIONS.has(request.destination)
}
