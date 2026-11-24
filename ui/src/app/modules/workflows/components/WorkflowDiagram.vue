<script setup lang="ts">
// Editor visual de un workflow, al estilo de los editores de flujos por nodos:
// un disparador, un nodo por paso y una salida con nombre por cada camino
// (Éxito, Fallo, Sí/No, casos). Se programa arrastrando pasos desde la paleta
// y uniendo salidas con pasos; soltar una conexión en el vacío crea el paso.
// Las posiciones se guardan en `workflow.layout` (solo presentación).
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Handle, MarkerType, Position, VueFlow, useVueFlow, type Connection, type Edge, type Node, type NodeChange } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/controls/dist/style.css'
import type { Step, StepType, Workflow } from '@/infrastructure/api/types.ts'
import { stepColors, stepGroups, stepIcons, stepSummary, newStep } from '../steps.ts'
import { appendStep, connect, isTerminal, links, moveToStart, ports, reachable, removeStep, type Link } from '../graph.ts'

export interface StepVisit {
  count: number
  status: 'ok' | 'error' | 'handled' | 'running'
}

/** Recorrido de una ejecución sobre el diagrama. */
export interface Trace {
  visits: Record<string, StepVisit>
  /** Pares `origen>destino` recorridos (por id de paso). */
  edges: Set<string>
  first: string | null
}

const props = defineProps<{
  /** Índices de pasos con errores de validación. */
  errors: Set<number>
  /** Cómo se inicia: escenarios que lo usan y programación. */
  usedBy: number
  trace?: Trace | null
}>()
const wf = defineModel<Workflow>({ required: true })
/** Índice del paso seleccionado, `'trigger'` o `null`. */
const selected = defineModel<number | 'trigger' | null>('selected', { default: null })
const { t } = useI18n()
const flowId = `wf-${Math.random().toString(36).slice(2)}`
const { fitView, screenToFlowCoordinate, updateNodeInternals } = useVueFlow(flowId)

const START = '__inicio'
const NODE_W = 250
const nodeId = (i: number) => `s${i}`
const indexOf = (nid: string) => (nid.startsWith('s') ? Number(nid.slice(1)) : -1)
const steps = computed(() => wf.value.steps)
const allLinks = computed(() => links(steps.value))
const live = computed(() => reachable(steps.value))

const nodeWidth = (s: Step) => Math.max(NODE_W, ports(s).length * 84)

/** Distribución automática por niveles (BFS desde el inicio). */
function autoLayout(): Record<number, { x: number; y: number }> {
  const level = new Map<number, number>()
  if (steps.value.length) {
    level.set(0, 0)
    const queue = [0]
    while (queue.length) {
      const i = queue.shift()!
      for (const l of allLinks.value.filter((x) => x.from === i)) {
        if (!level.has(l.to)) {
          level.set(l.to, level.get(i)! + 1)
          queue.push(l.to)
        }
      }
    }
  }
  let extra = Math.max(-1, ...level.values()) + 1
  steps.value.forEach((_, i) => {
    if (!level.has(i)) level.set(i, extra++)
  })
  const rows = new Map<number, number[]>()
  for (const [i, lv] of level) rows.set(lv, [...(rows.get(lv) ?? []), i])
  const pos: Record<number, { x: number; y: number }> = {}
  for (const [lv, items] of rows) {
    items.sort((a, b) => a - b)
    items.forEach((i, k) => (pos[i] = { x: (k - (items.length - 1) / 2) * 320 - NODE_W / 2 + 80, y: 120 + lv * 190 }))
  }
  return pos
}

const nodes = computed<Node[]>(() => {
  const auto = autoLayout()
  const layout = wf.value.layout ?? {}
  const list: Node[] = [{ id: START, type: 'trigger', position: layout[START] ?? { x: 0, y: 0 }, data: {}, deletable: false }]
  steps.value.forEach((s, i) => {
    list.push({ id: nodeId(i), type: 'step', position: (s.id && layout[s.id]) || auto[i]!, data: { index: i }, deletable: false })
  })
  return list
})

const traced = (from: string | null, to: string | undefined) => !!props.trace && !!to && props.trace.edges.has(`${from ?? START}>${to}`)

const edges = computed<Edge[]>(() => {
  const list: Edge[] = []
  const first = steps.value[0]
  if (first) {
    const on = !!props.trace && props.trace.first === first.id
    list.push({
      id: 'e-start',
      source: START,
      sourceHandle: 'start',
      target: nodeId(0),
      deletable: false,
      class: on ? 'edge-traced' : '',
      animated: on,
      markerEnd: MarkerType.ArrowClosed,
    })
  }
  for (const l of allLinks.value) {
    const id = `e${l.from}-${l.kind}`
    const on = traced(steps.value[l.from]!.id, steps.value[l.to]?.id)
    list.push({
      id,
      source: nodeId(l.from),
      sourceHandle: l.kind,
      target: nodeId(l.to),
      animated: on,
      class: [`edge-${l.tone}`, selectedEdge.value === id ? 'edge-selected' : '', on ? 'edge-traced' : ''].join(' '),
      markerEnd: MarkerType.ArrowClosed,
      data: l,
    })
  }
  return list
})

// Vue Flow mide las salidas de cada nodo; hay que avisarle cuando cambian.
const portsSignature = computed(() => steps.value.map((s) => `${s.type}:${ports(s).map((p) => p.kind).join(',')}`).join('|'))
watch(portsSignature, () => nextTick(() => updateNodeInternals()))

// -- cambios conservando la selección -------------------------------------------

function mutate(fn: () => void) {
  const sel = typeof selected.value === 'number' ? steps.value[selected.value]?.id : null
  fn()
  if (sel !== null && sel !== undefined) {
    const j = steps.value.findIndex((s) => s.id === sel)
    selected.value = j >= 0 ? j : null
  }
}

function place(id: string, pos: { x: number; y: number }) {
  wf.value.layout ??= {}
  wf.value.layout[id] = { x: Math.round(pos.x), y: Math.round(pos.y) }
}

/** Crea un paso en una posición y, opcionalmente, lo conecta desde una salida. */
function addAt(type: StepType, pos: { x: number; y: number }, from?: { index: number | 'start'; kind: string }) {
  const step = newStep(type, steps.value.map((s) => s.id))
  mutate(() => {
    appendStep(wf.value.steps, step)
    const idx = wf.value.steps.length - 1
    if (from?.index === 'start') moveToStart(wf.value.steps, idx)
    else if (from) connect(wf.value.steps, from.index, from.kind, idx)
  })
  // Si el flujo usaba posiciones automáticas, se fijan para que no se reacomode.
  for (const n of nodes.value) {
    const key = n.id === START ? START : steps.value[indexOf(n.id)]?.id
    if (key && key !== step.id && !wf.value.layout?.[key]) place(key, n.position)
  }
  place(step.id, { x: pos.x - nodeWidth(step) / 2, y: pos.y })
  selected.value = steps.value.findIndex((s) => s.id === step.id)
}

// -- conexiones ------------------------------------------------------------------

let pending: { index: number | 'start'; kind: string } | null = null
let connected = false

function onConnectStart(p: { nodeId?: string; handleId?: string | null }) {
  connected = false
  if (!p.nodeId) return (pending = null)
  pending = p.nodeId === START ? { index: 'start', kind: 'start' } : { index: indexOf(p.nodeId), kind: p.handleId ?? 'next' }
}

function link(from: { index: number | 'start'; kind: string }, to: number) {
  mutate(() => {
    if (from.index === 'start') moveToStart(wf.value.steps, to)
    else if (from.index !== to) connect(wf.value.steps, from.index, from.kind, to)
  })
}

function onConnect(c: Connection) {
  connected = true
  const to = indexOf(c.target)
  if (to < 0) return
  if (c.source === START) return link({ index: 'start', kind: 'start' }, to)
  const from = indexOf(c.source)
  if (from < 0 || isTerminal(steps.value[from]!)) return
  link({ index: from, kind: c.sourceHandle ?? 'next' }, to)
}

const quickAdd = ref<{ x: number; y: number; flow: { x: number; y: number }; from: { index: number | 'start'; kind: string } } | null>(null)
const wrapper = ref<HTMLElement | null>(null)

/** Soltar sobre un nodo lo conecta; soltar en el vacío ofrece crear un paso. */
function onConnectEnd(ev?: MouseEvent | TouchEvent) {
  const from = pending
  pending = null
  if (connected || !from || !ev || !wrapper.value) return
  const point = 'changedTouches' in ev ? ev.changedTouches[0]! : ev
  const el = document.elementFromPoint(point.clientX, point.clientY)
  const nodeEl = el?.closest('.vue-flow__node') as HTMLElement | null
  const target = nodeEl?.dataset.id
  if (target) {
    const to = indexOf(target)
    if (to >= 0) link(from, to)
    return
  }
  const box = wrapper.value.getBoundingClientRect()
  // El menú se mantiene dentro del lienzo (mide 260 × 360 como máximo).
  quickAdd.value = {
    x: Math.max(8, Math.min(point.clientX - box.left, box.width - 268)),
    y: Math.max(8, Math.min(point.clientY - box.top, box.height - 368)),
    flow: screenToFlowCoordinate({ x: point.clientX, y: point.clientY }),
    from,
  }
}

function chooseQuick(type: StepType) {
  const q = quickAdd.value
  quickAdd.value = null
  if (q) addAt(type, q.flow, q.from)
}

// -- paleta ------------------------------------------------------------------------

const DRAG_MIME = 'application/x-fakes-step'
const palette = ref(localStorage.getItem('fakes.palette') !== 'closed')
watch(palette, (v) => localStorage.setItem('fakes.palette', v ? 'open' : 'closed'))

function onPaletteDrag(e: DragEvent, type: StepType) {
  e.dataTransfer?.setData(DRAG_MIME, type)
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy'
}

function onDragOver(e: DragEvent) {
  if (e.dataTransfer?.types.includes(DRAG_MIME)) {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'copy'
  }
}

function onDrop(e: DragEvent) {
  const type = e.dataTransfer?.getData(DRAG_MIME) as StepType | undefined
  if (!type) return
  e.preventDefault()
  addAt(type, screenToFlowCoordinate({ x: e.clientX, y: e.clientY }))
}

/** Clic en la paleta: el paso aparece en el centro de la vista. */
function addCentered(type: StepType) {
  const box = wrapper.value?.getBoundingClientRect()
  const center = box ? screenToFlowCoordinate({ x: box.left + box.width / 2, y: box.top + box.height / 2 }) : { x: 0, y: 0 }
  addAt(type, center)
}

// -- selección y borrado ---------------------------------------------------------

const selectedEdge = ref<string | null>(null)
const selectedLink = computed(() => edges.value.find((e) => e.id === selectedEdge.value)?.data as Link | undefined)

function disconnectSelected() {
  const l = selectedLink.value
  if (l) mutate(() => connect(wf.value.steps, l.from, l.kind, null))
  selectedEdge.value = null
}

function deleteSelectedStep() {
  if (typeof selected.value !== 'number') return
  const i = selected.value
  const id = steps.value[i]?.id
  selected.value = null
  removeStep(wf.value.steps, i)
  if (id && wf.value.layout) delete wf.value.layout[id]
}

function onKey(e: KeyboardEvent) {
  if (e.key !== 'Delete' && e.key !== 'Backspace') return
  const tag = (e.target as HTMLElement | null)?.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || (e.target as HTMLElement | null)?.isContentEditable) return
  if (selectedLink.value) {
    e.preventDefault()
    disconnectSelected()
  } else if (typeof selected.value === 'number') {
    e.preventDefault()
    deleteSelectedStep()
  }
}

/** Movimiento de nodos: las posiciones se guardan en `layout`. */
function onNodesChange(changes: NodeChange[]) {
  for (const ch of changes) {
    if (ch.type !== 'position' || !ch.position) continue
    const key = ch.id === START ? START : steps.value[indexOf(ch.id)]?.id
    if (key) place(key, ch.position)
  }
}

function onNodeClick(ev: { node: Node }) {
  const i = indexOf(ev.node.id)
  selected.value = ev.node.id === START ? 'trigger' : i >= 0 ? i : null
  selectedEdge.value = null
  quickAdd.value = null
  wrapper.value?.focus({ preventScroll: true })
}

function onPaneClick() {
  selected.value = null
  selectedEdge.value = null
  quickAdd.value = null
}

const stepLabel = (i: number) => steps.value[i]?.name || steps.value[i]?.id || `paso ${i + 1}`

function arrange() {
  wf.value.layout = {}
  setTimeout(() => fitView({ padding: 0.2 }), 30)
}

function focusStep(id: string) {
  const i = steps.value.findIndex((s) => s.id === id)
  if (i >= 0) fitView({ nodes: [nodeId(i)], padding: 1.2, duration: 300, maxZoom: 1.1 })
}

const scheduleText = computed(() => {
  const ms = wf.value.schedule?.everyMs
  if (!ms) return null
  return ms >= 60000 ? `cada ${+(ms / 60000).toFixed(1)} min` : `cada ${+(ms / 1000).toFixed(1)} s`
})

const visitOf = (i: number) => {
  const id = steps.value[i]?.id
  return id ? props.trace?.visits[id] : undefined
}

defineExpose({ arrange, focusStep })
</script>

<template>
  <div ref="wrapper" class="wf-diagram" :class="{ 'wf-diagram--trace': !!trace }" tabindex="0" @keydown="onKey" @dragover="onDragOver" @drop="onDrop">
    <VueFlow
      :id="flowId"
      :nodes="nodes"
      :edges="edges"
      :default-edge-options="{ type: 'smoothstep' }"
      :nodes-connectable="true"
      :apply-changes="false"
      :delete-key-code="null"
      :connection-radius="28"
      fit-view-on-init
      :min-zoom="0.25"
      :max-zoom="1.6"
      @connect="onConnect"
      @connect-start="onConnectStart"
      @connect-end="onConnectEnd"
      @nodes-change="onNodesChange"
      @node-click="onNodeClick"
      @edge-click="(ev) => ((selectedEdge = ev.edge.id === 'e-start' ? null : ev.edge.id), (quickAdd = null))"
      @pane-click="onPaneClick"
    >
      <template #node-trigger>
        <div class="wf-trigger" :class="{ 'wf-trigger--selected': selected === 'trigger' }">
          <div class="d-flex align-center gap-2 mb-1">
            <VIcon icon="mdi-lightning-bolt" size="18" />
            <span class="font-weight-bold">Disparador</span>
          </div>
          <div class="d-flex flex-wrap gap-1">
            <span class="wf-chip">Manual / API / MCP</span>
            <span class="wf-chip">{{ usedBy ? `${usedBy} escenario${usedBy === 1 ? '' : 's'}` : 'Sin escenarios' }}</span>
            <span v-if="scheduleText" class="wf-chip">{{ scheduleText }}</span>
            <span v-if="!wf.enabled" class="wf-chip">Deshabilitado</span>
          </div>
          <div class="wf-ports">
            <div class="wf-port wf-port--success">
              Inicio
              <Handle id="start" type="source" :position="Position.Bottom" />
            </div>
          </div>
        </div>
      </template>

      <template #node-step="{ data }">
        <div
          v-if="steps[data.index]"
          class="wf-node"
          :class="{
            'wf-node--selected': selected === data.index,
            'wf-node--error': errors.has(data.index),
            'wf-node--orphan': !live.has(data.index),
            'wf-node--visited': !!visitOf(data.index),
          }"
          :style="{ '--accent': `rgb(var(--v-theme-${stepColors[steps[data.index]!.type]}))`, width: `${nodeWidth(steps[data.index]!)}px` }"
        >
          <Handle type="target" :position="Position.Top" />
          <div class="wf-node__head">
            <VIcon :icon="stepIcons[steps[data.index]!.type]" size="16" />
            <span class="text-truncate">{{ t(`stepType.${steps[data.index]!.type}`) }}</span>
            <VSpacer />
            <VIcon v-if="errors.has(data.index)" icon="mdi-alert-circle-outline" size="16" title="Tiene errores" />
            <span
              v-if="visitOf(data.index)"
              class="wf-visit"
              :class="`wf-visit--${visitOf(data.index)!.status}`"
              :title="`Se ejecutó ${visitOf(data.index)!.count} vez/veces`"
            >
              {{ visitOf(data.index)!.status === 'running' ? '●' : `×${visitOf(data.index)!.count}` }}
            </span>
          </div>
          <div class="wf-node__body">
            <div class="font-weight-medium text-truncate">{{ steps[data.index]!.name || steps[data.index]!.id }}</div>
            <code v-if="stepSummary(steps[data.index]!)" class="wf-node__summary">{{ stepSummary(steps[data.index]!) }}</code>
            <div v-if="!live.has(data.index)" class="wf-node__orphan">No se alcanza desde el disparador</div>
          </div>
          <div v-if="ports(steps[data.index]!).length" class="wf-ports">
            <div v-for="p in ports(steps[data.index]!)" :key="p.kind" class="wf-port" :class="`wf-port--${p.tone}`" :title="`Salida «${p.label}»: arrástrala hasta un paso`">
              <span class="text-truncate">{{ p.label }}</span>
              <Handle :id="p.kind" type="source" :position="Position.Bottom" />
            </div>
          </div>
          <div v-else class="wf-ports wf-ports--end">{{ steps[data.index]!.type === 'fail' ? 'Termina con error' : 'Termina' }}</div>
        </div>
      </template>

      <Background :gap="18" />
      <Controls :show-interactive="false" position="bottom-right" />
    </VueFlow>

    <!-- Paleta -->
    <div class="wf-palette" :class="{ 'wf-palette--closed': !palette }">
      <div class="d-flex align-center px-2 pt-2 pb-1">
        <VIcon icon="mdi-shape-plus-outline" size="18" class="me-2" />
        <span v-if="palette" class="font-weight-bold text-body-medium">Pasos</span>
        <VSpacer />
        <VBtn
          :icon="palette ? 'mdi-chevron-left' : 'mdi-chevron-right'"
          size="x-small"
          variant="text"
          :aria-label="palette ? 'Ocultar paleta' : 'Mostrar paleta'"
          @click="palette = !palette"
        />
      </div>
      <template v-if="palette">
        <div class="text-body-small text-medium-emphasis px-3 pb-2">Arrastra un paso al lienzo o haz clic para agregarlo.</div>
        <div v-for="g in stepGroups" :key="g.title" class="px-2 pb-2">
          <div class="wf-palette__group">{{ g.title }}</div>
          <div
            v-for="st in g.types"
            :key="st"
            class="wf-palette__item"
            draggable="true"
            role="button"
            tabindex="0"
            :title="t(`stepHint.${st}`)"
            :style="{ '--accent': `rgb(var(--v-theme-${stepColors[st]}))` }"
            @dragstart="onPaletteDrag($event, st)"
            @click="addCentered(st)"
            @keydown.enter="addCentered(st)"
          >
            <VIcon :icon="stepIcons[st]" size="16" />
            <span class="text-truncate">{{ t(`stepType.${st}`) }}</span>
          </div>
        </div>
      </template>
    </div>

    <!-- Crear paso al soltar una conexión en el vacío -->
    <VCard v-if="quickAdd" class="wf-quick" :style="{ left: `${quickAdd.x}px`, top: `${quickAdd.y}px` }" elevation="8">
      <div class="text-body-small text-medium-emphasis px-3 pt-2">Crear y conectar</div>
      <VList density="compact" class="py-1">
        <VListItem v-for="st in stepGroups.flatMap((g) => g.types)" :key="st" :prepend-icon="stepIcons[st]" :title="t(`stepType.${st}`)" @click="chooseQuick(st)" />
      </VList>
    </VCard>

    <div v-if="selectedLink" class="wf-edge-bar text-body-small">
      {{ stepLabel(selectedLink.from) }}
      <b>({{ selectedLink.label }})</b>
      → {{ stepLabel(selectedLink.to) }}
      <VBtn size="small" color="error" variant="tonal" prepend-icon="mdi-link-variant-off" @click="disconnectSelected">Desconectar</VBtn>
    </div>

    <div v-if="!steps.length" class="wf-empty text-body-medium text-medium-emphasis">
      <VIcon icon="mdi-gesture-tap-hold" size="32" class="mb-2" />
      <div>Arrastra el primer paso desde la paleta,</div>
      <div>o arrastra la salida «Inicio» del disparador a un espacio vacío.</div>
    </div>
  </div>
</template>

<style scoped>
.wf-diagram {
  position: relative;
  height: 100%;
  min-height: 560px;
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: 12px;
  overflow: hidden;
  background: rgb(var(--v-theme-surface));
  outline: none;
}
.wf-trigger {
  width: 260px;
  padding: 10px 12px 0;
  border-radius: 14px;
  background: rgb(var(--v-theme-primary));
  color: rgb(var(--v-theme-on-primary));
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.15);
}
.wf-trigger--selected {
  box-shadow: 0 0 0 3px rgba(var(--v-theme-primary), 0.35);
}
.wf-chip {
  font-size: 11px;
  padding: 1px 8px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.2);
}
.wf-node {
  border-radius: 12px;
  background: rgb(var(--v-theme-surface));
  border: 1px solid rgba(var(--v-border-color), 0.25);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.08);
  color: rgb(var(--v-theme-on-surface));
  overflow: visible;
}
.wf-node__head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-radius: 11px 11px 0 0;
  background: var(--accent);
  color: #fff;
  font-size: 12px;
  font-weight: 600;
}
.wf-node__body {
  padding: 8px 12px 6px;
}
.wf-node__summary {
  display: block;
  margin-top: 2px;
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  opacity: 0.75;
}
.wf-node__orphan {
  margin-top: 4px;
  font-size: 11px;
  color: rgb(var(--v-theme-warning));
}
.wf-node--selected {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
}
.wf-node--error {
  border-color: rgb(var(--v-theme-error));
}
.wf-node--orphan {
  border-style: dashed;
}
.wf-diagram--trace .wf-node:not(.wf-node--visited) {
  opacity: 0.45;
}
.wf-ports {
  display: flex;
  gap: 6px;
  padding: 6px 8px 10px;
}
.wf-ports--end {
  justify-content: center;
  font-size: 11px;
  opacity: 0.6;
}
.wf-port {
  position: relative;
  flex: 1 1 0;
  min-width: 0;
  text-align: center;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 6px 6px;
  border-radius: 8px;
  background: rgba(var(--v-theme-on-surface), 0.06);
  display: flex;
  justify-content: center;
}
.wf-trigger .wf-port {
  background: rgba(255, 255, 255, 0.2);
  margin: 0 40px;
}
.wf-port--success {
  color: rgb(var(--v-theme-success));
}
.wf-port--error {
  color: rgb(var(--v-theme-error));
}
.wf-trigger .wf-port--success {
  color: inherit;
}
.wf-port :deep(.vue-flow__handle) {
  width: 12px;
  height: 12px;
  bottom: -7px;
  background: currentColor;
  border: 2px solid rgb(var(--v-theme-surface));
}
.wf-node :deep(.vue-flow__handle-top) {
  width: 12px;
  height: 12px;
  top: -7px;
  background: rgb(var(--v-theme-surface));
  border: 2px solid rgba(var(--v-theme-on-surface), 0.4);
}
.wf-visit {
  font-size: 11px;
  padding: 0 6px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.95);
  color: rgb(var(--v-theme-success));
}
.wf-visit--error {
  color: rgb(var(--v-theme-error));
}
.wf-visit--handled {
  color: rgb(var(--v-theme-warning));
}
.wf-visit--running {
  color: rgb(var(--v-theme-info));
  animation: wf-pulse 1s infinite;
}
@keyframes wf-pulse {
  50% {
    opacity: 0.3;
  }
}
.wf-palette {
  position: absolute;
  top: 10px;
  left: 10px;
  bottom: 10px;
  width: 190px;
  overflow: auto;
  border-radius: 12px;
  background: rgba(var(--v-theme-surface), 0.96);
  border: 1px solid rgba(var(--v-border-color), 0.15);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.08);
  z-index: 4;
}
.wf-palette--closed {
  bottom: auto;
  width: auto;
}
.wf-palette__group {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  opacity: 0.6;
  padding: 4px 6px;
}
.wf-palette__item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  margin-bottom: 4px;
  border-radius: 8px;
  border: 1px solid rgba(var(--v-border-color), 0.15);
  border-left: 4px solid var(--accent);
  cursor: grab;
  font-size: 13px;
  user-select: none;
}
.wf-palette__item:hover,
.wf-palette__item:focus-visible {
  background: rgba(var(--v-theme-on-surface), 0.05);
}
.wf-palette__item .v-icon {
  color: var(--accent);
}
.wf-quick {
  position: absolute;
  z-index: 6;
  width: 260px;
  max-height: 360px;
  overflow: auto;
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
.wf-empty {
  position: absolute;
  top: 55%;
  left: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
  pointer-events: none;
}
:deep(.edge-selected .vue-flow__edge-path) {
  stroke-width: 3;
  stroke: rgb(var(--v-theme-primary));
}
:deep(.edge-error .vue-flow__edge-path) {
  stroke: rgb(var(--v-theme-error));
  stroke-dasharray: 6 4;
}
:deep(.edge-traced .vue-flow__edge-path) {
  stroke: rgb(var(--v-theme-info));
  stroke-width: 3;
}
:deep(.vue-flow__controls-button) {
  background: rgb(var(--v-theme-surface));
  color: rgb(var(--v-theme-on-surface));
  fill: currentColor;
  border-bottom-color: rgba(var(--v-border-color), 0.15);
}
</style>
