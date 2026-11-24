<script setup lang="ts">
// Editor visual de un workflow: cada paso es un nodo; las flechas son el
// orden de ejecución, los saltos (`next`) y las ramas de las condiciones.
// Conectar dos nodos cambia el salto; borrar una flecha termina ahí.
// Las posiciones se guardan en `workflow.layout`.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Handle, MarkerType, Position, VueFlow, useVueFlow, type Connection, type Edge, type Node, type NodeChange } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/controls/dist/style.css'
import type { Step, StepType, Workflow } from '@/infrastructure/api/types.ts'

const props = defineProps<{
  icons: Record<StepType, string>
  summary: (s: Step) => string
  /** Índices de pasos con errores de validación. */
  errors: Set<number>
}>()
const wf = defineModel<Workflow>({ required: true })
const selected = defineModel<number | null>('selected', { default: null })
const { t } = useI18n()
const flowId = `wf-${Math.random().toString(36).slice(2)}`
const { fitView } = useVueFlow(flowId)

const START = '__inicio'
const nodeId = (i: number) => `s${i}`
const indexOf = (nid: string) => (nid.startsWith('s') ? Number(nid.slice(1)) : -1)
const terminal = (s: Step) => s.type === 'end' || s.type === 'fail'

/** Índice destino de un salto (`null` = termina). */
function target(i: number, jump: string | null | undefined): number | null {
  const steps = wf.value.steps
  if (jump === 'end') return null
  if (jump == null || jump === '' || jump === 'next') return i + 1 < steps.length ? i + 1 : null
  const j = steps.findIndex((s) => s.id === jump)
  return j >= 0 ? j : null
}

interface Link {
  from: number
  to: number
  kind: 'next' | 'true' | 'false'
}

const links = computed<Link[]>(() => {
  const out: Link[] = []
  wf.value.steps.forEach((s, i) => {
    if (terminal(s)) return
    if (s.type === 'condition') {
      const a = target(i, s.onTrue)
      const b = target(i, s.onFalse)
      if (a !== null) out.push({ from: i, to: a, kind: 'true' })
      if (b !== null) out.push({ from: i, to: b, kind: 'false' })
      return
    }
    const n = target(i, s.next)
    if (n !== null) out.push({ from: i, to: n, kind: 'next' })
  })
  return out
})

/** Distribución automática por niveles (BFS desde el inicio). */
function autoLayout(): Record<number, { x: number; y: number }> {
  const steps = wf.value.steps
  const level = new Map<number, number>()
  if (steps.length) {
    level.set(0, 0)
    const queue = [0]
    while (queue.length) {
      const i = queue.shift()!
      for (const l of links.value.filter((x) => x.from === i)) {
        if (!level.has(l.to)) {
          level.set(l.to, level.get(i)! + 1)
          queue.push(l.to)
        }
      }
    }
  }
  let extra = Math.max(-1, ...level.values()) + 1
  steps.forEach((_, i) => {
    if (!level.has(i)) level.set(i, extra++)
  })
  const rows = new Map<number, number[]>()
  for (const [i, lv] of level) rows.set(lv, [...(rows.get(lv) ?? []), i])
  const pos: Record<number, { x: number; y: number }> = {}
  for (const [lv, items] of rows) {
    items.sort((a, b) => a - b)
    items.forEach((i, k) => (pos[i] = { x: (k - (items.length - 1) / 2) * 300, y: 110 + lv * 130 }))
  }
  return pos
}

const nodes = computed<Node[]>(() => {
  const auto = autoLayout()
  const layout = wf.value.layout ?? {}
  const list: Node[] = [
    { id: START, type: 'start', position: layout[START] ?? { x: 0, y: 0 }, data: {}, deletable: false },
  ]
  wf.value.steps.forEach((s, i) => {
    list.push({
      id: nodeId(i),
      type: 'step',
      position: (s.id && layout[s.id]) || auto[i]!,
      data: { index: i },
      deletable: false,
    })
  })
  return list
})

const edges = computed<Edge[]>(() => {
  const list: Edge[] = []
  if (wf.value.steps.length) {
    list.push({ id: 'e-start', source: START, target: nodeId(0), deletable: false, markerEnd: MarkerType.ArrowClosed })
  }
  for (const l of links.value) {
    const jump = l.kind !== 'next' || l.to !== l.from + 1
    list.push({
      id: `e${l.from}-${l.kind}`,
      source: nodeId(l.from),
      sourceHandle: l.kind === 'next' ? undefined : l.kind,
      target: nodeId(l.to),
      label: l.kind === 'true' ? 'sí' : l.kind === 'false' ? 'no' : undefined,
      animated: jump && l.to <= l.from,
      class: `edge-${l.kind}${selectedEdge.value === `e${l.from}-${l.kind}` ? ' edge-selected' : ''}`,
      markerEnd: MarkerType.ArrowClosed,
      data: l,
    })
  }
  return list
})

// -- interacción ----------------------------------------------------------------

function setJump(from: number, kind: Link['kind'], to: number | null) {
  const s = wf.value.steps[from]
  if (!s) return
  const steps = wf.value.steps
  const value = to === null ? 'end' : to === from + 1 ? undefined : steps[to]!.id
  if (s.type === 'condition') {
    if (kind === 'true') s.onTrue = value ?? null
    else if (kind === 'false') s.onFalse = value ?? null
    else return
  } else {
    s.next = value ?? null
  }
}

function onConnect(c: Connection) {
  const from = indexOf(c.source)
  const to = indexOf(c.target)
  if (from < 0 || to < 0 || from === to) return
  const s = wf.value.steps[from]!
  if (terminal(s)) return
  const kind = (c.sourceHandle === 'true' || c.sourceHandle === 'false' ? c.sourceHandle : 'next') as Link['kind']
  setJump(from, s.type === 'condition' && kind === 'next' ? 'true' : kind, to)
}

const selectedEdge = ref<string | null>(null)
const selectedLink = computed(() => edges.value.find((e) => e.id === selectedEdge.value)?.data as Link | undefined)

function removeSelectedEdge() {
  const l = selectedLink.value
  if (l) setJump(l.from, l.kind, null)
  selectedEdge.value = null
}

function onKey(e: KeyboardEvent) {
  if ((e.key === 'Delete' || e.key === 'Backspace') && selectedLink.value) {
    const tag = (e.target as HTMLElement | null)?.tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA') return
    e.preventDefault()
    removeSelectedEdge()
  }
}

/** Movimiento de nodos: las posiciones se guardan en `layout`. */
function onNodesChange(changes: NodeChange[]) {
  for (const ch of changes) {
    if (ch.type !== 'position' || !ch.position) continue
    const key = ch.id === START ? START : wf.value.steps[indexOf(ch.id)]?.id
    if (!key) continue
    wf.value.layout ??= {}
    wf.value.layout[key] = { x: Math.round(ch.position.x), y: Math.round(ch.position.y) }
  }
}

const stepLabel = (i: number) => wf.value.steps[i]?.name || wf.value.steps[i]?.id || `paso ${i + 1}`

function onNodeClick(ev: { node: Node }) {
  const i = indexOf(ev.node.id)
  selected.value = i >= 0 ? i : null
  selectedEdge.value = null
}

function arrange() {
  wf.value.layout = {}
  setTimeout(() => fitView({ padding: 0.2 }), 30)
}

const hint = ref(true)
defineExpose({ arrange })
</script>

<template>
  <div class="wf-diagram" tabindex="0" @keydown="onKey">
    <VueFlow
      :id="flowId"
      :nodes="nodes"
      :edges="edges"
      :default-edge-options="{ type: 'smoothstep' }"
      :nodes-connectable="true"
      :apply-changes="false"
      :delete-key-code="null"
      fit-view-on-init
      :min-zoom="0.3"
      :max-zoom="1.6"
      @connect="onConnect"
      @nodes-change="onNodesChange"
      @node-click="onNodeClick"
      @edge-click="(ev) => (selectedEdge = ev.edge.id === 'e-start' ? null : ev.edge.id)"
      @pane-click="(selected = null), (selectedEdge = null)"
    >
      <template #node-start>
        <div class="wf-start">
          <VIcon icon="mdi-play" size="16" class="me-1" /> Inicio
          <Handle type="source" :position="Position.Bottom" :connectable="false" />
        </div>
      </template>

      <template #node-step="{ data }">
        <div
          v-if="wf.steps[data.index]"
          class="wf-node"
          :class="{
            'wf-node--selected': selected === data.index,
            'wf-node--error': props.errors.has(data.index),
            'wf-node--terminal': terminal(wf.steps[data.index]!),
          }"
        >
          <Handle type="target" :position="Position.Top" />
          <div class="d-flex align-center gap-2">
            <VIcon :icon="icons[wf.steps[data.index]!.type]" size="18" color="primary" />
            <span class="font-weight-medium text-truncate">{{ wf.steps[data.index]!.name || wf.steps[data.index]!.id }}</span>
            <VSpacer />
            <VIcon v-if="props.errors.has(data.index)" icon="mdi-alert-circle-outline" color="error" size="16" />
          </div>
          <div class="text-body-small text-medium-emphasis">{{ t(`stepType.${wf.steps[data.index]!.type}`) }}</div>
          <code class="wf-node__summary">{{ summary(wf.steps[data.index]!) }}</code>
          <template v-if="wf.steps[data.index]!.type === 'condition'">
            <Handle id="true" type="source" :position="Position.Bottom" class="wf-handle-true" :style="{ left: '30%' }" />
            <Handle id="false" type="source" :position="Position.Bottom" class="wf-handle-false" :style="{ left: '70%' }" />
          </template>
          <Handle v-else-if="!terminal(wf.steps[data.index]!)" type="source" :position="Position.Bottom" />
        </div>
      </template>

      <Background :gap="18" />
      <Controls :show-interactive="false" />
    </VueFlow>
    <div v-if="selectedLink" class="wf-edge-bar text-body-small">
      {{ stepLabel(selectedLink.from) }}
      <template v-if="selectedLink.kind !== 'next'">({{ selectedLink.kind === 'true' ? 'sí' : 'no' }})</template>
      → {{ stepLabel(selectedLink.to) }}
      <VBtn size="small" color="error" variant="tonal" prepend-icon="mdi-link-variant-off" @click="removeSelectedEdge">Terminar aquí</VBtn>
    </div>
    <div v-if="hint" class="wf-hint text-body-small">
      <span class="flex-1-1">
        Arrastra desde el punto inferior de un paso hasta otro para cambiar a dónde salta. Haz clic en una flecha y pulsa
        Supr (o «Terminar aquí») para que el paso termine ahí. Las condiciones tienen dos salidas: <b>sí</b> (izquierda) y
        <b>no</b> (derecha).
      </span>
      <VBtn icon="mdi-close" size="x-small" variant="text" aria-label="Ocultar ayuda" @click="hint = false" />
    </div>
  </div>
</template>

<style scoped>
.wf-diagram {
  position: relative;
  height: 620px;
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: 12px;
  overflow: hidden;
  background: rgb(var(--v-theme-surface));
}
.wf-start {
  padding: 6px 14px;
  border-radius: 999px;
  background: rgb(var(--v-theme-primary));
  color: rgb(var(--v-theme-on-primary));
  font-weight: 600;
  display: flex;
  align-items: center;
}
.wf-node {
  width: 240px;
  padding: 10px 12px;
  border-radius: 12px;
  background: rgb(var(--v-theme-surface));
  border: 1px solid rgba(var(--v-border-color), 0.25);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  color: rgb(var(--v-theme-on-surface));
}
.wf-node--selected {
  border-color: rgb(var(--v-theme-primary));
  box-shadow: 0 0 0 2px rgba(var(--v-theme-primary), 0.35);
}
.wf-node--error {
  border-color: rgb(var(--v-theme-error));
}
.wf-node--terminal {
  border-style: dashed;
}
.wf-node__summary {
  display: block;
  margin-top: 4px;
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  opacity: 0.75;
}
.wf-hint {
  position: absolute;
  top: 8px;
  left: 8px;
  right: 8px;
  padding: 6px 10px;
  border-radius: 8px;
  background: rgba(var(--v-theme-surface), 0.92);
  border: 1px solid rgba(var(--v-border-color), 0.15);
  display: flex;
  align-items: center;
  gap: 6px;
}
.wf-edge-bar {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  padding: 6px 8px 6px 14px;
  border-radius: 999px;
  background: rgb(var(--v-theme-surface));
  border: 1px solid rgba(var(--v-border-color), 0.2);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.12);
  display: flex;
  align-items: center;
  gap: 8px;
  z-index: 5;
}
:deep(.edge-selected .vue-flow__edge-path) {
  stroke-width: 3;
  stroke: rgb(var(--v-theme-primary));
}
:deep(.wf-handle-true) {
  background: rgb(var(--v-theme-success));
}
:deep(.wf-handle-false) {
  background: rgb(var(--v-theme-error));
}
:deep(.edge-true .vue-flow__edge-path) {
  stroke: rgb(var(--v-theme-success));
}
:deep(.edge-false .vue-flow__edge-path) {
  stroke: rgb(var(--v-theme-error));
}
:deep(.vue-flow__edge-textbg) {
  fill: rgb(var(--v-theme-surface));
}
:deep(.vue-flow__edge-text) {
  fill: rgb(var(--v-theme-on-surface));
}
:deep(.vue-flow__controls-button) {
  background: rgb(var(--v-theme-surface));
  color: rgb(var(--v-theme-on-surface));
  fill: currentColor;
  border-bottom-color: rgba(var(--v-border-color), 0.15);
}
</style>
