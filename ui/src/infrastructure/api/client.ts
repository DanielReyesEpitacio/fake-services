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
  Workflow,
  WorkflowSummary,
  WorkflowView,
  ExecutionSummary,
  ExecutionDetail,
  Bundle,
  ImportMode,
  ImportReport,
  OnConflict,
  AutostartStatus,
  SpecImportReport,
  TemplateInfo,
  TemplateInstallReport,
  CollectionInfo,
  ResourceRecord,
  DetectionReport,
} from './types.ts'

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

/** Descarga un archivo de la API (con el token) y lo guarda en el equipo. */
async function download(path: string): Promise<string> {
  const headers: Record<string, string> = {}
  const t = token()
  if (t) headers.Authorization = `Bearer ${t}`
  const resp = await fetch(BASE + path, { headers })
  if (!resp.ok) {
    const data = await resp.json().catch(() => null)
    throw new ApiError(resp.status, data?.error ?? 'error', data?.message ?? `Error HTTP ${resp.status}`, data?.issues ?? [])
  }
  const cd = resp.headers.get('content-disposition') ?? ''
  const filename = /filename="([^"]+)"/.exec(cd)?.[1] ?? 'fake-services.json'
  saveBlob(await resp.blob(), filename)
  return filename
}

/** Guarda un blob como archivo descargado. */
export function saveBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

/** Guarda un valor como archivo JSON. */
export function saveJson(value: unknown, filename: string) {
  saveBlob(new Blob([JSON.stringify(value, null, 2) + '\n'], { type: 'application/json' }), filename)
}

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

  // Workflows
  workflows: (sid: string) => request<WorkflowSummary[]>('GET', `/services/${enc(sid)}/workflows`),
  workflow: (sid: string, wid: string) =>
    request<WorkflowView>('GET', `/services/${enc(sid)}/workflows/${enc(wid)}`),
  createWorkflow: (sid: string, w: Partial<Workflow>) =>
    request<WorkflowView>('POST', `/services/${enc(sid)}/workflows`, w),
  updateWorkflow: (sid: string, wid: string, w: Workflow) =>
    request<WorkflowView>('PUT', `/services/${enc(sid)}/workflows/${enc(wid)}`, w),
  deleteWorkflow: (sid: string, wid: string) =>
    request<void>('DELETE', `/services/${enc(sid)}/workflows/${enc(wid)}`),
  validateWorkflow: (sid: string, w: Partial<Workflow>) =>
    request<ValidationReport>('POST', `/services/${enc(sid)}/workflows/validate`, w),
  runWorkflow: (sid: string, wid: string, input: Record<string, unknown>, context?: unknown) =>
    request<ExecutionSummary>('POST', `/services/${enc(sid)}/workflows/${enc(wid)}/run`, { input, context }),

  // Ejecuciones
  executions: (filter: { serviceId?: string; workflowId?: string; status?: string; beforeMs?: number; limit?: number } = {}) => {
    const q = new URLSearchParams()
    for (const [k, v] of Object.entries(filter)) if (v !== undefined && v !== '') q.set(k, String(v))
    const qs = q.toString()
    return request<ExecutionSummary[]>('GET', `/executions${qs ? `?${qs}` : ''}`)
  },
  execution: (id: string) => request<ExecutionDetail>('GET', `/executions/${enc(id)}`),
  cancelExecution: (id: string) => request<ExecutionSummary>('POST', `/executions/${enc(id)}/cancel`),
  clearExecutions: () => request<{ deleted: number }>('DELETE', '/executions'),

  // Importar / exportar
  exportBundle: (opts: { services?: string[]; workspace?: boolean; files?: boolean; resources?: boolean }) => {
    const q = new URLSearchParams()
    if (opts.services?.length) q.set('services', opts.services.join(','))
    if (opts.workspace) q.set('workspace', 'true')
    if (opts.files) q.set('files', 'true')
    if (opts.resources) q.set('resources', 'true')
    return download(`/export?${q.toString()}`)
  },
  importBundle: (req: {
    bundle: Bundle
    mode?: ImportMode
    onConflict?: OnConflict
    workspaceName?: string
    dryRun?: boolean
  }) => request<ImportReport>('POST', '/import', req),

  importSpec: (req: {
    content: string
    format?: 'openapi' | 'postman'
    name?: string
    prefix?: string
    onConflict?: OnConflict
    dryRun?: boolean
  }) => request<SpecImportReport>('POST', '/import/spec', req),
  validateAll: () => request<ValidationReport>('GET', '/validate'),
  detect: (path: string) => request<DetectionReport>('POST', '/detect', { path }),

  // Plantillas
  templates: () => request<TemplateInfo[]>('GET', '/templates'),
  template: (id: string) => request<Bundle>('GET', `/templates/${enc(id)}`),
  installTemplate: (id: string, opts: { onConflict?: OnConflict; start?: boolean; dryRun?: boolean } = {}) =>
    request<TemplateInstallReport>('POST', `/templates/${enc(id)}/install`, opts),

  // Recursos simulados
  collections: (sid: string) => request<CollectionInfo[]>('GET', `/services/${enc(sid)}/resources`),
  resources: (sid: string, col: string) =>
    request<ResourceRecord[]>('GET', `/services/${enc(sid)}/resources/${enc(col)}`),
  putResource: (sid: string, col: string, id: string, data: unknown) =>
    request<ResourceRecord>('PUT', `/services/${enc(sid)}/resources/${enc(col)}/${enc(id)}`, data),
  deleteResource: (sid: string, col: string, id: string) =>
    request<void>('DELETE', `/services/${enc(sid)}/resources/${enc(col)}/${enc(id)}`),
  clearResources: (sid: string, col?: string) =>
    request<{ deleted: number }>('DELETE', `/services/${enc(sid)}/resources${col ? `/${enc(col)}` : ''}`),

  // Sistema
  autostart: () => request<AutostartStatus>('GET', '/system/autostart'),
  setAutostart: (enabled: boolean) => request<AutostartStatus>('PUT', '/system/autostart', { enabled }),

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
