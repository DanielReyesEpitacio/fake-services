// Estado compartido mínimo: estado de la app, servicios y avisos.

import { reactive, readonly } from 'vue'
import { api, ApiError, subscribeEvents } from './api'
import type { AppStatus, RequestSummary, ServiceSummary } from './types'

interface Toast {
  id: number
  kind: 'ok' | 'error'
  text: string
}

const state = reactive({
  status: null as AppStatus | null,
  services: [] as ServiceSummary[],
  recent: [] as RequestSummary[],
  connected: true,
  toasts: [] as Toast[],
  /** Se incrementa con cada cambio de configuración (para recargar vistas). */
  configVersion: 0,
})

let toastId = 0

export function toast(text: string, kind: Toast['kind'] = 'ok') {
  const id = ++toastId
  state.toasts.push({ id, kind, text })
  setTimeout(() => dismiss(id), kind === 'error' ? 6000 : 2500)
}

export function dismiss(id: number) {
  const i = state.toasts.findIndex((t) => t.id === id)
  if (i >= 0) state.toasts.splice(i, 1)
}

export function toastError(e: unknown) {
  if (e instanceof ApiError) toast(e.message, 'error')
  else toast(String(e), 'error')
}

export async function refresh() {
  try {
    const [status, services] = await Promise.all([api.status(), api.services()])
    state.status = status
    state.services = services
    state.connected = true
  } catch (e) {
    state.connected = false
    if (!(e instanceof ApiError && e.code === 'network')) toastError(e)
  }
}

let started = false
const requestListeners = new Set<(r: RequestSummary) => void>()

export function onRequest(fn: (r: RequestSummary) => void): () => void {
  requestListeners.add(fn)
  return () => requestListeners.delete(fn)
}

export function startLive() {
  if (started) return
  started = true
  refresh()
  api.requests({ limit: 20 }).then((r) => (state.recent = r)).catch(() => {})
  let pending: ReturnType<typeof setTimeout> | undefined
  const debounced = () => {
    clearTimeout(pending)
    pending = setTimeout(refresh, 150)
  }
  subscribeEvents((ev) => {
    if (ev.type === 'request') {
      state.recent.unshift(ev.data)
      state.recent.splice(50)
      if (state.status) state.status.requestsTotal++
      requestListeners.forEach((fn) => fn(ev.data))
    } else {
      if (ev.type === 'config_changed') state.configVersion++
      debounced()
    }
  })
}

export const store = readonly(state)

export async function toggleService(id: string, running: boolean) {
  try {
    if (running) await api.stopService(id)
    else await api.startService(id)
    await refresh()
  } catch (e) {
    toastError(e)
  }
}

export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast('Copiado al portapapeles')
  } catch {
    toast('No se pudo copiar', 'error')
  }
}

export function formatTime(ms: number): string {
  const d = new Date(ms)
  return d.toLocaleTimeString('es-MX', { hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23' })
}

export function formatDateTime(ms: number): string {
  return new Date(ms).toLocaleString('es-MX', {
    dateStyle: 'medium',
    timeStyle: 'medium',
  })
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}
