#!/usr/bin/env node
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { setTimeout as delay } from 'node:timers/promises'
import { assert, optionalEnv } from './lib/mailquill-e2e.mjs'

const appUrl = optionalEnv('MAILQUILL_APP_URL', optionalEnv('MAILQUILL_BASE_URL', 'http://127.0.0.1:8080'))
const chromePath = optionalEnv('CHROME_PATH', await findChrome())
assert(chromePath, 'Set CHROME_PATH to a Chromium/Chrome executable')
assert(typeof WebSocket === 'function', 'This script requires a Node runtime with global WebSocket support')

const userDataDir = await mkdtemp(join(tmpdir(), 'mailquill-csp-'))
const chrome = spawn(chromePath, [
  '--headless=new',
  '--disable-gpu',
  '--no-first-run',
  '--no-default-browser-check',
  `--user-data-dir=${userDataDir}`,
  '--remote-debugging-port=0',
  'about:blank',
], {
  stdio: ['ignore', 'ignore', 'pipe'],
})

let browserWsUrl
let stderr = ''
chrome.stderr.setEncoding('utf8')
chrome.stderr.on('data', (chunk) => {
  stderr += chunk
  const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/)
  if (match) {
    browserWsUrl = match[1]
  }
})

try {
  await waitFor(() => browserWsUrl, 'Chrome DevTools endpoint')
  const versionUrl = browserWsUrl.replace(/^ws:/, 'http:').replace(/\/devtools\/browser\/.*/, '/json/version')
  const version = await (await fetch(versionUrl)).json()
  const browser = createCdp(version.webSocketDebuggerUrl)

  await browser.open()
  const { targetId } = await browser.send('Target.createTarget', { url: 'about:blank' })
  const targets = await (await fetch(versionUrl.replace('/json/version', '/json/list'))).json()
  const target = targets.find((entry) => entry.id === targetId)
  assert(target?.webSocketDebuggerUrl, 'Could not find page DevTools endpoint')

  const page = createCdp(target.webSocketDebuggerUrl)
  await page.open()

  const consoleMessages = []
  page.on('Runtime.consoleAPICalled', (params) => {
    consoleMessages.push(params.args.map((arg) => arg.value || arg.description || '').join(' '))
  })
  page.on('Log.entryAdded', (params) => {
    consoleMessages.push(params.entry.text || '')
  })
  page.on('Security.securityStateChanged', (params) => {
    if (params.explanations) {
      for (const explanation of params.explanations) {
        consoleMessages.push(explanation.summary || explanation.description || '')
      }
    }
  })

  await page.send('Runtime.enable')
  await page.send('Log.enable')
  await page.send('Security.enable')
  await page.send('Page.enable')
  const loaded = waitForPageLoad(page)
  await page.send('Page.navigate', { url: appUrl })
  await loaded
  await delay(Number(optionalEnv('E2E_CSP_SETTLE_MS', '1500')))

  const cspMessages = consoleMessages.filter((message) =>
    /content security policy|violates the following content security policy|csp/i.test(message),
  )
  assert(cspMessages.length === 0, `CSP violations found:\n${cspMessages.join('\n')}`)

  console.log(JSON.stringify({
    ok: true,
    flow: 'csp-browser-console-check',
    url: appUrl,
    console_message_count: consoleMessages.length,
  }))
} finally {
  chrome.kill('SIGTERM')
  await rm(userDataDir, { recursive: true, force: true })
}

async function findChrome() {
  const candidates = [
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ]

  for (const candidate of candidates) {
    try {
      const response = await import('node:fs/promises').then((fs) => fs.access(candidate))
      if (response === undefined) {
        return candidate
      }
    } catch {
      // try the next candidate
    }
  }
  return ''
}

async function waitFor(fn, description, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const value = fn()
    if (value) {
      return value
    }
    await delay(50)
  }
  throw new Error(`Timed out waiting for ${description}`)
}

async function waitForPageLoad(page) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Timed out waiting for page load')), 15000)
    page.on('Page.loadEventFired', () => {
      clearTimeout(timeout)
      resolve()
    })
  })
}

function createCdp(url) {
  let id = 0
  let socket
  const pending = new Map()
  const handlers = new Map()

  return {
    async open() {
      socket = new WebSocket(url)
      socket.addEventListener('message', (event) => {
        const message = JSON.parse(event.data)
        if (message.id && pending.has(message.id)) {
          const { resolve, reject } = pending.get(message.id)
          pending.delete(message.id)
          if (message.error) {
            reject(new Error(message.error.message))
          } else {
            resolve(message.result || {})
          }
          return
        }
        const eventHandlers = handlers.get(message.method) || []
        for (const handler of eventHandlers) {
          handler(message.params || {})
        }
      })
      await new Promise((resolve, reject) => {
        socket.addEventListener('open', resolve, { once: true })
        socket.addEventListener('error', reject, { once: true })
      })
    },
    send(method, params = {}) {
      const messageId = ++id
      socket.send(JSON.stringify({ id: messageId, method, params }))
      return new Promise((resolve, reject) => {
        pending.set(messageId, { resolve, reject })
      })
    },
    on(method, handler) {
      const eventHandlers = handlers.get(method) || []
      eventHandlers.push(handler)
      handlers.set(method, eventHandlers)
    },
  }
}
