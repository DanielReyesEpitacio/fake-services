// Cliente de la API de administración. La UI usa exactamente la misma API
// que los agentes: no hay canales privados.

import type {
  AppEvent,
  AppStatus,
  Endpoint,
  EndpointSummary,
  EndpointView,
  Issue,
  RequestRecord,
  RequestSummary,
  Service,
  ServiceSummary,
  ServiceView,
  ValidationReport,
  Workspace,
  WorkspaceInfo,
  WorkspaceView,
} from './types'

const BASE = '/api/v1'

function token(): string {
  return document.querySelector<HTMLMetaElement>('meta[name="fakes-admin-token"]')?.content ?? ''
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public issues: Issue[] = [],
  ) {
    super(message)
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {}
  const t = token()
  if (t) headers.Authorization = `Bearer ${t}`
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  let resp: Response
  try {
    resp = await fetch(BASE + path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    throw new ApiError(0, 'network', 'No se pudo conectar con Fake Services.')
  }
  if (resp.status === 204) return undefined as T
  const data = await resp.json().catch(() => null)
  if (!resp.ok) {
    throw new ApiError(
      resp.status,
      data?.error ?? 'error',
      data?.message ?? `Error HTTP ${resp.status}`,
      data?.issues ?? [],
    )
  }
  return data as T
}

const enc = encodeURIComponent

export const api = {
  status: () => request<AppStatus>('GET', '/status'),
  reload: () => request<WorkspaceView>('POST', '/reload'),

  // Espacios
  workspaces: () => request<WorkspaceInfo[]>('GET', '/workspaces'),
  createWorkspace: (body: Partial<Workspace> & { name: string; path?: string; activate?: boolean }) =>
    request<WorkspaceInfo>('POST', '/workspaces', body),
  registerWorkspace: (path: string, activate: boolean) =>
    request<WorkspaceInfo>('POST', '/workspaces/register', { path, activate }),
  activateWorkspace: (id: string) => request<WorkspaceView>('POST', `/workspaces/${enc(id)}/activate`),
  forgetWorkspace: (id: string) => request<void>('DELETE', `/workspaces/${enc(id)}`),
  activeWorkspace: () => request<WorkspaceView>('GET', '/workspace'),
  updateWorkspace: (ws: Workspace) => request<Workspace>('PUT', '/workspace', ws),

  // Servicios
  services: () => request<ServiceSummary[]>('GET', '/services'),
  service: (id: string) => request<ServiceView>('GET', `/services/${enc(id)}`),
  createService: (s: Partial<Service>) => request<ServiceView>('POST', '/services', s),
  updateService: (id: string, s: Service) => request<ServiceView>('PUT', `/services/${enc(id)}`, s),
  deleteService: (id: string) => request<void>('DELETE', `/services/${enc(id)}`),
  startService: (id: string) => request<ServiceView>('POST', `/services/${enc(id)}/start`),
  stopService: (id: string) => request<ServiceView>('POST', `/services/${enc(id)}/stop`),
  startAll: () => request<void>('POST', '/services/start-all'),
  stopAll: () => request<void>('POST', '/services/stop-all'),

  // Endpoints
  endpoints: (sid: string) => request<EndpointSummary[]>('GET', `/services/${enc(sid)}/endpoints`),
  endpoint: (sid: string, eid: string) =>
    request<EndpointView>('GET', `/services/${enc(sid)}/endpoints/${enc(eid)}`),
  createEndpoint: (sid: string, e: Partial<Endpoint>) =>
    request<EndpointView>('POST', `/services/${enc(sid)}/endpoints`, e),
  updateEndpoint: (sid: string, eid: string, e: Endpoint) =>
    request<EndpointView>('PUT', `/services/${enc(sid)}/endpoints/${enc(eid)}`, e),
  deleteEndpoint: (sid: string, eid: string) =>
    request<void>('DELETE', `/services/${enc(sid)}/endpoints/${enc(eid)}`),
  setEndpointEnabled: (sid: string, eid: string, enabled: boolean) =>
    request<EndpointView>('POST', `/services/${enc(sid)}/endpoints/${enc(eid)}/${enabled ? 'enable' : 'disable'}`),
  setActiveScenario: (sid: string, eid: string, scenarioId: string | null) =>
    request<EndpointView>('PUT', `/services/${enc(sid)}/endpoints/${enc(eid)}/active-scenario`, { scenarioId }),
  validateEndpoint: (sid: string, e: Partial<Endpoint>) =>
    request<ValidationReport>('POST', `/services/${enc(sid)}/endpoints/validate`, e),

  // Historial
  requests: (filter: {
    serviceId?: string
    endpointId?: string
    errorsOnly?: boolean
    search?: string
    beforeMs?: number
    limit?: number
  } = {}) => {
    const q = new URLSearchParams()
    for (const [k, v] of Object.entries(filter)) {
      if (v !== undefined && v !== '' && v !== false) q.set(k, String(v))
    }
    const qs = q.toString()
    return request<RequestSummary[]>('GET', `/requests${qs ? `?${qs}` : ''}`)
  },
  request: (id: string) => request<RequestRecord>('GET', `/requests/${enc(id)}`),
  clearRequests: (serviceId?: string) =>
    request<{ deleted: number }>('DELETE', `/requests${serviceId ? `?serviceId=${enc(serviceId)}` : ''}`),
}

/** Suscripción a eventos en vivo con reconexión automática. */
export function subscribeEvents(onEvent: (ev: AppEvent) => void): () => void {
  let source: EventSource | null = null
  let closed = false
  let retry: ReturnType<typeof setTimeout> | undefined
  const connect = () => {
    const t = token()
    source = new EventSource(`${BASE}/events${t ? `?token=${enc(t)}` : ''}`)
    source.onmessage = (m) => {
      try {
        onEvent(JSON.parse(m.data) as AppEvent)
      } catch {
        /* evento malformado: se ignora */
      }
    }
    source.onerror = () => {
      source?.close()
      if (!closed) retry = setTimeout(connect, 2000)
    }
  }
  connect()
  return () => {
    closed = true
    clearTimeout(retry)
    source?.close()
  }
}
