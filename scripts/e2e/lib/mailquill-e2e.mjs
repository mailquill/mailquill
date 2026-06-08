import { setTimeout as delay } from 'node:timers/promises'

export function requiredEnv(name) {
  const value = process.env[name]
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`)
  }
  return value
}

export function optionalEnv(name, fallback) {
  return process.env[name] || fallback
}

export function assert(condition, message) {
  if (!condition) {
    throw new Error(message)
  }
}

export function baseUrl() {
  return optionalEnv('MAILQUILL_BASE_URL', 'http://127.0.0.1:8080').replace(/\/$/, '')
}

export async function request(path, options = {}) {
  const response = await fetch(`${baseUrl()}${path}`, {
    ...options,
    headers: {
      'content-type': 'application/json',
      ...(options.headers || {}),
    },
  })

  const text = await response.text()
  const body = text ? JSON.parse(text) : null

  if (!response.ok) {
    throw new Error(`${options.method || 'GET'} ${path} failed: ${response.status} ${text}`)
  }

  return { response, body }
}

export async function registerOrLogin(email, password) {
  const register = await fetch(`${baseUrl()}/api/auth/register`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email, password }),
  })

  if (register.status === 201) {
    const body = await register.json()
    return body.access_token
  }

  if (register.status !== 409) {
    throw new Error(`register failed: ${register.status} ${await register.text()}`)
  }

  const login = await fetch(`${baseUrl()}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email, password }),
  })
  if (!login.ok) {
    throw new Error(`login failed: ${login.status} ${await login.text()}`)
  }
  const body = await login.json()
  return body.access_token
}

export function authHeaders(token) {
  return {
    authorization: `Bearer ${token}`,
  }
}

export async function poll(description, fn, { attempts = 24, intervalMs = 5000 } = {}) {
  let lastError
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      const result = await fn(attempt)
      if (result) {
        return result
      }
    } catch (error) {
      lastError = error
    }
    await delay(intervalMs)
  }

  throw new Error(`Timed out waiting for ${description}${lastError ? `: ${lastError.message}` : ''}`)
}

export function uniqueEmail(prefix) {
  return `${prefix}.${Date.now()}@example.test`
}
