// Modelo de grafo de un workflow: salidas de cada paso y operaciones que
// cambian el orden de los pasos sin cambiar a dónde salta cada uno.
//
// En la definición, una salida sin destino continúa con el paso siguiente
// (salvo «Fallo», que sin destino hace fallar la ejecución). Antes de
// insertar, mover o borrar pasos se hacen explícitos todos los saltos y
// después se vuelven a simplificar.
import type { Step } from '@/infrastructure/api/types.ts'

export type PortTone = 'default' | 'success' | 'error' | 'warning'

export interface Port {
  /** `next`, `true`, `false`, `error`, `otherwise` o `case<i>`. */
  kind: string
  label: string
  tone: PortTone
}

const END = 'end'
const NEXT = 'next'

/** Tipos que muestran la salida «Fallo» por defecto. */
const failing = new Set<Step['type']>(['http', 'resource', 'runWorkflow'])

export const isTerminal = (s: Step) => s.type === 'end' || s.type === 'fail'

/** ¿Puede tener salida «Fallo»? (todos salvo `fail` y `end`). */
export const canHaveErrorPort = (s: Step) => !isTerminal(s)

export function ports(s: Step): Port[] {
  if (isTerminal(s)) return []
  const out: Port[] = []
  if (s.type === 'condition') {
    out.push({ kind: 'true', label: 'Sí', tone: 'success' }, { kind: 'false', label: 'No', tone: 'default' })
  } else if (s.type === 'switch') {
    s.cases.forEach((c, i) => out.push({ kind: `case${i}`, label: c.label || `Caso ${i + 1}`, tone: 'default' }))
    out.push({ kind: 'otherwise', label: 'Ninguno', tone: 'default' })
  } else {
    out.push({ kind: NEXT, label: s.type === 'wait' ? 'Al terminar' : 'Éxito', tone: 'success' })
  }
  if (failing.has(s.type) || s.onError) out.push({ kind: 'error', label: 'Fallo', tone: 'error' })
  return out
}

export function getJump(s: Step, kind: string): string | null | undefined {
  if (kind === 'error') return s.onError
  if (s.type === 'condition') return kind === 'true' ? s.onTrue : kind === 'false' ? s.onFalse : undefined
  if (s.type === 'switch') {
    if (kind === 'otherwise') return s.otherwise
    const i = caseIndex(kind)
    return i >= 0 ? s.cases[i]?.next : undefined
  }
  return kind === NEXT ? s.next : undefined
}

export function setJump(s: Step, kind: string, value: string | undefined) {
  if (kind === 'error') {
    s.onError = value
    if (value !== undefined && s.type === 'http') s.continueOnError = false
    return
  }
  if (s.type === 'condition') {
    if (kind === 'true') s.onTrue = value
    else if (kind === 'false') s.onFalse = value
  } else if (s.type === 'switch') {
    if (kind === 'otherwise') s.otherwise = value
    else {
      const c = s.cases[caseIndex(kind)]
      if (c) c.next = value
    }
  } else if (kind === NEXT) {
    s.next = value
  }
}

function caseIndex(kind: string): number {
  return kind.startsWith('case') ? Number(kind.slice(4)) : -1
}

/** Todas las salidas que el paso podría usar (incluida «Fallo»). */
function allKinds(s: Step): string[] {
  const kinds = ports(s).map((p) => p.kind)
  if (canHaveErrorPort(s) && !kinds.includes('error')) kinds.push('error')
  return kinds
}

/** Índice al que lleva una salida (`null` = termina o falla). */
export function targetIndex(steps: Step[], i: number, kind: string): number | null {
  const v = getJump(steps[i]!, kind)
  if (kind === 'error' && (v == null || v === '')) return null
  if (v === END) return null
  if (v == null || v === '' || v === NEXT) return i + 1 < steps.length ? i + 1 : null
  const j = steps.findIndex((s) => s.id === v)
  return j >= 0 ? j : null
}

export interface Link {
  from: number
  to: number
  kind: string
  label: string
  tone: PortTone
}

export function links(steps: Step[]): Link[] {
  const out: Link[] = []
  steps.forEach((s, i) => {
    for (const p of ports(s)) {
      const to = targetIndex(steps, i, p.kind)
      if (to !== null) out.push({ from: i, to, kind: p.kind, label: p.label, tone: p.tone })
    }
  })
  return out
}

/** Vuelve explícitos los saltos implícitos («siguiente paso»). */
export function materialize(steps: Step[]) {
  steps.forEach((s, i) => {
    const following = steps[i + 1]?.id ?? END
    for (const kind of allKinds(s)) {
      const v = getJump(s, kind)
      if (kind === 'error') {
        if (v === NEXT) setJump(s, kind, following)
      } else if (v == null || v === '' || v === NEXT) {
        setJump(s, kind, following)
      }
    }
  })
}

/** Quita los saltos que equivalen al comportamiento por defecto. */
export function simplify(steps: Step[]) {
  steps.forEach((s, i) => {
    const following = steps[i + 1]?.id ?? END
    for (const kind of allKinds(s)) {
      if (kind === 'error') {
        if (getJump(s, kind) === '') setJump(s, kind, undefined)
      } else if (getJump(s, kind) === following) {
        setJump(s, kind, undefined)
      }
    }
  })
}

/** Agrega un paso al final sin que el último paso empiece a saltar hacia él. */
export function appendStep(steps: Step[], step: Step) {
  materialize(steps)
  steps.push(step)
  simplify(steps)
}

/** Elimina un paso; las salidas que llevaban a él terminan la ejecución. */
export function removeStep(steps: Step[], index: number) {
  const id = steps[index]?.id
  if (id === undefined) return
  materialize(steps)
  steps.splice(index, 1)
  for (const s of steps) {
    for (const kind of allKinds(s)) {
      if (getJump(s, kind) === id) setJump(s, kind, kind === 'error' ? undefined : END)
    }
  }
  simplify(steps)
}

/** Convierte un paso en el primero (el que inicia la ejecución). */
export function moveToStart(steps: Step[], index: number) {
  if (index <= 0 || index >= steps.length) return
  materialize(steps)
  const [s] = steps.splice(index, 1)
  steps.unshift(s!)
  simplify(steps)
}

/** Conecta la salida `kind` de `from` con `to` (`null` = terminar aquí). */
export function connect(steps: Step[], from: number, kind: string, to: number | null) {
  const s = steps[from]
  if (!s) return
  if (to === null) {
    setJump(s, kind, kind === 'error' ? undefined : END)
    if (kind !== 'error' && from === steps.length - 1) setJump(s, kind, undefined)
    return
  }
  const id = steps[to]!.id
  setJump(s, kind, kind !== 'error' && to === from + 1 ? undefined : id)
}

/** Actualiza los saltos al renombrar un paso. */
export function renameRefs(steps: Step[], oldId: string, newId: string) {
  for (const s of steps) {
    for (const kind of allKinds(s)) {
      if (getJump(s, kind) === oldId) setJump(s, kind, newId)
    }
  }
}

/** Ids de los pasos alcanzables desde el inicio. */
export function reachable(steps: Step[]): Set<number> {
  const seen = new Set<number>()
  if (!steps.length) return seen
  const all = links(steps)
  const queue = [0]
  seen.add(0)
  while (queue.length) {
    const i = queue.shift()!
    for (const l of all) {
      if (l.from === i && !seen.has(l.to)) {
        seen.add(l.to)
        queue.push(l.to)
      }
    }
  }
  return seen
}
