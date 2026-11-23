// Tipos del modelo de dominio (espejo de crates/fakes-domain) y vistas de la API.

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS' | '*'

export const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', '*']

export interface ServerSettings {
  host: string
  port: number
  cors: boolean
  maxBodyBytes: number
}

export interface HistorySettings {
  enabled: boolean
  maxEntries: number
  redactHeaders: string[]
  maxBodyCaptureBytes: number
}

export interface Workspace {
  schemaVersion?: number
  id: string
  name: string
  description?: string
  server: ServerSettings
  variables?: Record<string, string>
  history: HistorySettings
}

export type AuthConfig =
  | { type: 'none' }
  | { type: 'bearer'; token?: string | null }
  | { type: 'apiKey'; header: string; value?: string | null }
  | { type: 'basic'; username?: string | null; password?: string | null }

export interface Service {
  schemaVersion?: number
  id: string
  name: string
  description?: string
  upstreamUrl?: string | null
  prefix: string
  port?: number | null
  autoStart?: boolean
  variables?: Record<string, string>
  auth?: AuthConfig
  endpoints?: Endpoint[]
}

export interface ParamSpec {
  name: string
  required?: boolean
  description?: string
  pattern?: string | null
  example?: string | null
}

export type BodyKind = 'any' | 'json' | 'xml' | 'text' | 'form' | 'multipart' | 'binary'

export interface BodySpec {
  kind: BodyKind
  required?: boolean
  schema?: unknown
  example?: unknown
}

export interface RequestSpec {
  headers?: ParamSpec[]
  query?: ParamSpec[]
  pathParams?: ParamSpec[]
  body?: BodySpec | null
}

export type RuleSource = 'header' | 'query' | 'path' | 'body' | 'method' | 'variable'
export type Operator =
  | 'equals'
  | 'notEquals'
  | 'contains'
  | 'notContains'
  | 'exists'
  | 'notExists'
  | 'greaterThan'
  | 'lessThan'
  | 'regex'

export interface Rule {
  source: RuleSource
  key: string
  operator: Operator
  value?: unknown
}

export type ResponseBody =
  | { type: 'none' }
  | { type: 'json'; content: unknown }
  | { type: 'text'; content: string; contentType?: string | null }
  | { type: 'binary'; base64: string; contentType?: string | null }
  | { type: 'file'; path: string; contentType?: string | null }

export interface ResponseDef {
  status: number
  headers?: Record<string, string>
  body?: ResponseBody
  delayMs?: number
  jitterMs?: number
}

export interface Scenario {
  id: string
  name: string
  description?: string
  enabled?: boolean
  ruleMode?: 'all' | 'any'
  rules?: Rule[]
  response: ResponseDef
}

export interface Endpoint {
  schemaVersion?: number
  id: string
  name?: string
  description?: string
  enabled: boolean
  method: HttpMethod
  path: string
  request?: RequestSpec
  scenarios: Scenario[]
  activeScenario?: string | null
  onValidationError?: ResponseDef | null
}

// -- vistas ---------------------------------------------------------------

export interface Issue {
  field: string
  message: string
}

export interface LoadProblem {
  file: string
  message: string
}

export interface ServiceSummary {
  id: string
  name: string
  description: string
  prefix: string
  port: number | null
  running: boolean
  url: string
  endpointCount: number
}

export interface ServiceView {
  service: Service
  running: boolean
  url: string
  portUrl: string | null
}

export interface EndpointSummary {
  id: string
  name: string
  method: HttpMethod
  path: string
  enabled: boolean
  scenarioCount: number
  activeScenario: string | null
  url: string
}

export interface EndpointView {
  serviceId: string
  endpoint: Endpoint
  url: string
}

export interface WorkspaceInfo {
  id: string
  name: string
  description: string
  path: string
  active: boolean
  available: boolean
}

export interface WorkspaceView {
  path: string
  workspace: Workspace
  services: ServiceSummary[]
  problems: LoadProblem[]
}

export interface ListenerStatus {
  host: string
  port: number
  serviceId: string | null
  url: string
  listening: boolean
  error: string | null
}

export interface AppStatus {
  version: string
  workspaceId: string
  workspaceName: string
  workspacePath: string
  centralUrl: string
  listeners: ListenerStatus[]
  servicesTotal: number
  servicesRunning: number
  endpointsTotal: number
  requestsTotal: number
  problems: LoadProblem[]
}

export interface ValidationReport {
  valid: boolean
  issues: Issue[]
}

export type Outcome =
  | 'matched'
  | 'no_scenario'
  | 'not_found'
  | 'method_not_allowed'
  | 'validation_failed'
  | 'unauthorized'
  | 'service_stopped'
  | 'preflight'
  | 'error'

export interface RequestSummary {
  id: string
  tsMs: number
  serviceId: string | null
  endpointId: string | null
  scenarioId: string | null
  method: string
  path: string
  query: string
  status: number
  durationMs: number
  outcome: Outcome
  message: string | null
}

export interface CapturedBody {
  size: number
  truncated: boolean
  encoding: 'utf8' | 'base64'
  content: string
}

export interface RequestRecord extends RequestSummary {
  workspaceId: string
  remoteAddr: string | null
  requestHeaders: [string, string][]
  requestBody: CapturedBody
  responseHeaders: [string, string][]
  responseBody: CapturedBody
}

export type AppEvent =
  | { type: 'request'; data: RequestSummary }
  | { type: 'config_changed'; data: { workspace_id: string } }
  | { type: 'runtime_changed'; data: { workspace_id: string } }
