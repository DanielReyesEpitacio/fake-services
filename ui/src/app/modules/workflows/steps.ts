// Catálogo de tipos de paso: iconos, colores, valores iniciales y resumen.
import type { Step, StepType } from '@/infrastructure/api/types.ts'

export const stepTypes: StepType[] = ['wait', 'http', 'condition', 'switch', 'setVariables', 'resource', 'emit', 'runWorkflow', 'fail', 'end']

/** Grupos de la paleta del editor visual. */
export const stepGroups: { title: string; types: StepType[] }[] = [
  { title: 'Flujo', types: ['condition', 'switch', 'wait', 'runWorkflow'] },
  { title: 'Acciones', types: ['http', 'resource', 'setVariables', 'emit'] },
  { title: 'Terminar', types: ['end', 'fail'] },
]

export const stepIcons: Record<StepType, string> = {
  wait: 'mdi-timer-sand',
  http: 'mdi-send',
  emit: 'mdi-broadcast',
  setVariables: 'mdi-variable',
  condition: 'mdi-call-split',
  switch: 'mdi-source-branch',
  runWorkflow: 'mdi-sitemap-outline',
  resource: 'mdi-database-outline',
  fail: 'mdi-alert-octagon-outline',
  end: 'mdi-flag-checkered',
}

/** Color de la franja de cada tipo (colores del tema). */
export const stepColors: Record<StepType, string> = {
  wait: 'info',
  http: 'primary',
  emit: 'secondary',
  setVariables: 'secondary',
  condition: 'warning',
  switch: 'warning',
  runWorkflow: 'info',
  resource: 'success',
  fail: 'error',
  end: 'success',
}

const baseIds: Record<StepType, string> = {
  wait: 'espera',
  http: 'webhook',
  emit: 'evento',
  setVariables: 'variables',
  condition: 'condicion',
  switch: 'dividir',
  runWorkflow: 'subflujo',
  resource: 'recurso',
  fail: 'error',
  end: 'fin',
}

export function uniqueStepId(base: string, ids: Iterable<string>): string {
  const used = new Set(ids)
  if (!used.has(base)) return base
  let n = 2
  while (used.has(`${base}-${n}`)) n++
  return `${base}-${n}`
}

export function newStep(type: StepType, ids: Iterable<string>): Step {
  const id = uniqueStepId(baseIds[type], ids)
  switch (type) {
    case 'wait':
      return { id, name: '', type, durationMs: 5000 }
    case 'http':
      return {
        id, name: '', type, method: 'POST', url: '{{ request.body.callback_url }}',
        headers: {}, query: {}, body: { type: 'json', content: { status: 'ok' } },
        timeoutMs: 10000, retry: { maxAttempts: 3, backoffMs: 1000, multiplier: 2 }, continueOnError: false,
      }
    case 'emit':
      return { id, name: '', type, event: 'evento' }
    case 'setVariables':
      return { id, name: '', type, values: { valor: 1 } }
    case 'condition':
      return { id, name: '', type, conditions: [{ path: 'vars.valor', operator: 'equals', value: 1 }], mode: 'all' }
    case 'switch':
      return {
        id, name: '', type,
        cases: [
          { label: 'Aprobado', conditions: [{ path: 'request.body.status', operator: 'equals', value: 'approved' }] },
          { label: 'Rechazado', conditions: [{ path: 'request.body.status', operator: 'equals', value: 'rejected' }] },
        ],
      }
    case 'runWorkflow':
      return { id, name: '', type, workflowId: '', wait: false }
    case 'resource':
      return { id, name: '', type, collection: 'items', operation: 'update', recordId: '{{ response.body.id }}', data: { status: 'done' } }
    case 'fail':
      return { id, name: '', type, message: 'Error simulado' }
    default:
      return { id, name: '', type: 'end' }
  }
}

export function stepSummary(s: Step): string {
  switch (s.type) {
    case 'wait':
      return s.durationMs >= 1000 ? `${s.durationMs / 1000} s` : `${s.durationMs} ms`
    case 'http':
      return `${s.method} ${s.url}`
    case 'emit':
      return s.event
    case 'setVariables':
      return Object.keys(s.values ?? {}).join(', ')
    case 'condition':
      return s.conditions.map((c) => `${c.path} ${c.operator}${c.value !== undefined ? ` ${JSON.stringify(c.value)}` : ''}`).join(s.mode === 'any' ? ' o ' : ' y ')
    case 'switch':
      return `${s.cases.length} caso(s)`
    case 'runWorkflow':
      return `${s.workflowId || '—'}${s.wait ? ' (espera)' : ''}`
    case 'resource':
      return `${s.operation} ${s.collection}${s.recordId ? ` / ${s.recordId}` : ''}`
    case 'fail':
      return s.message
    default:
      return ''
  }
}
