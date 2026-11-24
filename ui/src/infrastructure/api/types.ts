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

export interface WebhookSettings {
  allowLoopback: boolean
  allowPrivateNetworks: boolean
  allowedHosts?: string[]
}

export interface Workspace {
  schemaVersion?: number
  id: string
  name: string
  description?: string
  server: ServerSettings
  variables?: Record<string, string>
  history: HistorySettings
  webhooks: WebhookSettings
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
  workflows?: Workflow[]
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

export type RuleSource = 'header' | 'query' | 'path' | 'body' | 'method' | 'variable' | 'callCount'
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
  workflows?: WorkflowLink[]
  /** Probabilidad (0, 1] de que el escenario aplique cuando sus reglas se cumplen. */
  probability?: number | null
  /** Operación sobre un recurso persistente antes de responder. */
  resource?: ResourceAction | null
}

export type ResourceOp = 'create' | 'upsert' | 'update' | 'get' | 'delete' | 'list'

export interface ResourceAction {
  collection: string
  operation: ResourceOp
  recordId?: string
  data?: unknown
  onMissing?: ResponseDef | null
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
  workflowCount: number
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
  executionsActive: number
  workflowsTotal: number
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
  | { type: 'execution'; data: ExecutionSummary }
  | {
      type: 'workflow_event'
      data: { execution_id: string; workflow_id: string; service_id: string; event: string; data: unknown }
    }
  | { type: 'config_changed'; data: { workspace_id: string } }
  | { type: 'runtime_changed'; data: { workspace_id: string } }

// -- workflows ------------------------------------------------------------

export type StartWhen = 'requestReceived' | 'responseSent'

export interface WorkflowLink {
  workflowId: string
  when?: StartWhen
  input?: Record<string, unknown>
  enabled?: boolean
}

export interface Retry {
  maxAttempts: number
  backoffMs?: number
  multiplier?: number
}

export type StepBody =
  | { type: 'none' }
  | { type: 'json'; content: unknown }
  | { type: 'text'; content: string; contentType?: string | null }
  | { type: 'form'; fields: Record<string, string> }

export interface WorkflowCondition {
  path: string
  operator: Operator
  value?: unknown
}

interface StepBase {
  id: string
  name?: string
  next?: string | null
}

export type Step = StepBase &
  (
    | { type: 'wait'; durationMs: number }
    | {
        type: 'http'
        method: HttpMethod
        url: string
        headers?: Record<string, string>
        query?: Record<string, string>
        body?: StepBody
        timeoutMs?: number
        delayMs?: number
        retry?: Retry
        expectedStatus?: string[]
        continueOnError?: boolean
      }
    | { type: 'emit'; event: string; data?: unknown }
    | { type: 'setVariables'; values: Record<string, unknown> }
    | {
        type: 'condition'
        conditions: WorkflowCondition[]
        mode?: 'all' | 'any'
        onTrue?: string | null
        onFalse?: string | null
      }
    | { type: 'runWorkflow'; workflowId: string; input?: Record<string, unknown>; wait?: boolean }
    | {
        type: 'resource'
        collection: string
        operation: ResourceOp
        recordId?: string
        data?: unknown
        failIfMissing?: boolean
      }
    | { type: 'fail'; message: string }
    | { type: 'end' }
  )

export type StepType = Step['type']

export interface Workflow {
  schemaVersion?: number
  id: string
  name: string
  description?: string
  enabled: boolean
  variables?: Record<string, unknown>
  schedule?: { everyMs: number } | null
  steps: Step[]
  maxSteps?: number
  /** Posiciones de los pasos en el editor visual. */
  layout?: Record<string, { x: number; y: number }>
}

export interface WorkflowUsage {
  endpointId: string
  scenarioId: string
}

export interface WorkflowSummary {
  id: string
  name: string
  description: string
  enabled: boolean
  stepCount: number
  scheduleEveryMs: number | null
  usedBy: WorkflowUsage[]
}

export interface WorkflowView {
  serviceId: string
  workflow: Workflow
  usedBy: WorkflowUsage[]
}

export type ExecutionStatus = 'pending' | 'running' | 'waiting' | 'completed' | 'failed' | 'cancelled'
export type TriggerKind = 'manual' | 'request_received' | 'response_sent' | 'schedule' | 'workflow'

export interface ExecutionSummary {
  id: string
  serviceId: string
  workflowId: string
  workflowName: string
  status: ExecutionStatus
  trigger: TriggerKind
  parentId: string | null
  currentStep: string | null
  stepsExecuted: number
  wakeAtMs: number | null
  error: string | null
  createdMs: number
  updatedMs: number
  finishedMs: number | null
}

export interface ExecutionRecord extends ExecutionSummary {
  workspaceId: string
  depth: number
  pc: number
  attempt: number
  waitingFor: string | null
  context: Record<string, unknown>
  definition: Workflow
}

export interface TimelineEntry {
  seq: number
  tsMs: number
  kind: string
  stepId: string | null
  message: string
  data?: unknown
}

export interface ExecutionDetail {
  execution: ExecutionRecord
  timeline: TimelineEntry[]
}

// -- importar / exportar -----------------------------------------------------

export interface Bundle {
  format: 'fake-services'
  formatVersion: number
  exportedAt?: string
  appVersion?: string
  workspace?: Workspace | null
  services: Service[]
  files?: Record<string, string>
}

export type OnConflict = 'rename' | 'replace' | 'skip'
export type ImportMode = 'merge' | 'newWorkspace'

export interface ImportReport {
  dryRun: boolean
  workspaceId: string
  created: string[]
  replaced: string[]
  skipped: string[]
  renamed: { from: string; to: string }[]
  files: number
  resources?: number
  endpoints: number
  workflows: number
  warnings: string[]
}

export interface SpecImportReport extends ImportReport {
  format: 'openapi' | 'postman'
  serviceId: string
}

export interface TemplateInfo {
  id: string
  name: string
  description: string
  category: string
  tags: string[]
  upstreamUrl?: string | null
  prefix: string
  endpoints: number
  workflows: number
  installed: boolean
}

export interface TemplateInstallReport extends ImportReport {
  templateId: string
  serviceId: string
}

export interface CollectionInfo {
  collection: string
  count: number
  updatedMs: number
}

export interface ResourceRecord {
  id: string
  data: Record<string, unknown>
  createdMs: number
  updatedMs: number
}

export interface DetectedIntegration {
  providerId: string
  name: string
  template?: string | null
  confidence: string
  baseUrls: string[]
  occurrences: number
  evidence: { file: string; line: number; kind: string; snippet: string }[]
}

export interface DetectionReport {
  root: string
  filesScanned: number
  truncated: boolean
  integrations: DetectedIntegration[]
  otherHosts: { host: string; occurrences: number; files: string[] }[]
}

export interface AutostartStatus {
  supported: boolean
  enabled: boolean
  method: string
  command: string | null
}
