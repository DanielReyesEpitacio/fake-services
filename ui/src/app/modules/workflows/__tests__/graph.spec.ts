import { describe, expect, it } from 'vitest'
import type { Step } from '@/infrastructure/api/types.ts'
import { appendStep, connect, links, moveToStart, ports, reachable, removeStep, renameRefs } from '../graph.ts'

const wait = (id: string, extra: Partial<Step> = {}): Step => ({ id, type: 'wait', durationMs: 1, ...extra }) as Step
const flow = (steps: Step[]) => links(steps).map((l) => `${steps[l.from]!.id}-${l.kind}->${steps[l.to]!.id}`)

describe('grafo de workflows', () => {
  it('las salidas dependen del tipo', () => {
    expect(ports(wait('a')).map((p) => p.kind)).toEqual(['next'])
    expect(ports({ id: 'h', type: 'http', method: 'POST', url: 'x' }).map((p) => p.kind)).toEqual(['next', 'error'])
    expect(ports({ id: 's', type: 'switch', cases: [{ conditions: [] }, { label: 'B', conditions: [] }] }).map((p) => p.label)).toEqual([
      'Caso 1', 'B', 'Ninguno',
    ])
    expect(ports({ id: 'e', type: 'end' })).toEqual([])
  })

  it('agregar un paso no lo conecta al último', () => {
    const steps = [wait('a'), wait('b')]
    appendStep(steps, wait('c'))
    expect(flow(steps)).toEqual(['a-next->b'])
    expect(steps[1]!.next).toBe('end')
  })

  it('mover al inicio conserva los saltos', () => {
    const steps = [wait('a'), wait('b'), wait('c')]
    moveToStart(steps, 2)
    expect(steps.map((s) => s.id)).toEqual(['c', 'a', 'b'])
    expect(flow(steps).sort()).toEqual(['a-next->b', 'b-next->c'])
    expect(steps[1]!.next).toBeUndefined()
  })

  it('eliminar un paso termina las salidas que llevaban a él', () => {
    const steps = [wait('a'), wait('b'), wait('c')]
    removeStep(steps, 1)
    expect(flow(steps)).toEqual([])
    expect(reachable(steps)).toEqual(new Set([0]))
  })

  it('conectar y renombrar', () => {
    const steps: Step[] = [{ id: 'h', type: 'http', method: 'POST', url: 'x', continueOnError: true }, wait('ok'), wait('mal')]
    connect(steps, 0, 'error', 2)
    expect(steps[0]).toMatchObject({ onError: 'mal', continueOnError: false })
    connect(steps, 0, 'next', 1)
    expect(steps[0]!.next).toBeUndefined()
    renameRefs(steps, 'mal', 'rescate')
    expect(steps[0]!.onError).toBe('rescate')
    connect(steps, 0, 'error', null)
    expect(steps[0]!.onError).toBeUndefined()
  })
})
