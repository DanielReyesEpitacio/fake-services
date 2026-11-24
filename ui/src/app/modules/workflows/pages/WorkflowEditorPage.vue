<script setup lang="ts">
import { computed, onBeforeUnmount, provide, reactive, ref, watch } from 'vue'
import { onBeforeRouteLeave, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { api, ApiError, saveJson } from '@/infrastructure/api/client.ts'
import { formatTime, onExecution, store, toast, toastError } from '@/app/shared/state/live.ts'
import { confirmAction } from '@/app/shared/state/confirm.ts'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import JsonEditor from '@/app/shared/components/JsonEditor.vue'
import StepEditor from '../components/StepEditor.vue'
import WorkflowDiagram, { type Trace } from '../components/WorkflowDiagram.vue'
import WorkflowSettings from '../components/WorkflowSettings.vue'
import { newStep, stepIcons, stepSummary, stepTypes, uniqueStepId } from '../steps.ts'
import { appendStep, removeStep as removeGraphStep, renameRefs } from '../graph.ts'
import type { ExecutionSummary, Issue, Step, StepType, Workflow, WorkflowUsage } from '@/infrastructure/api/types.ts'

const props = defineProps<{ sid: string; wid: string }>()
const router = useRouter()
const { t } = useI18n()
const diagram = ref<InstanceType<typeof WorkflowDiagram> | null>(null)

const wf = ref<Workflow | null>(null)
const usedBy = ref<WorkflowUsage[]>([])
const siblings = ref<{ id: string; name: string }[]>([])
const executions = ref<ExecutionSummary[]>([])
const original = ref('')
const issues = ref<Issue[]>([])
const notFound = ref(false)
const saving = ref(false)
const openPanels = ref<number[]>([])
const view = ref<'list' | 'diagram'>(localStorage.getItem('fakes.workflowView') === 'list' ? 'list' : 'diagram')
watch(view, (v) => localStorage.setItem('fakes.workflowView', v))
/** Paso seleccionado en el diagrama, o el disparador. */
const selectedStep = ref<number | 'trigger' | null>(null)
const selectedIndex = computed(() => (typeof selectedStep.value === 'number' && wf.value?.steps[selectedStep.value] ? selectedStep.value : null))

const invalidJson = reactive(new Set<symbol>())
provide('invalidJson', invalidJson)

const dirty = computed(() => !!wf.value && JSON.stringify(wf.value) !== original.value)
const serviceName = computed(() => store.services.find((s) => s.id === props.sid)?.name ?? props.sid)
const stepIds = computed(() => wf.value?.steps.map((s) => s.id) ?? [])

function normalize(w: Workflow): Workflow {
  w.description ??= ''
  w.variables ??= {}
  w.maxSteps ??= 500
  w.schedule ??= null
  w.layout ??= {}
  for (const s of w.steps) {
    s.name ??= ''
    if (s.type === 'http') {
      s.headers ??= {}
      s.query ??= {}
      s.body ??= { type: 'none' }
      s.continueOnError ??= false
    }
    if (s.type === 'runWorkflow') s.wait ??= false
  }
  return w
}

async function load() {
  try {
    const [v, list] = await Promise.all([api.workflow(props.sid, props.wid), api.workflows(props.sid)])
    wf.value = normalize(v.workflow)
    usedBy.value = v.usedBy
    siblings.value = list.map((w) => ({ id: w.id, name: w.name }))
    original.value = JSON.stringify(wf.value)
    loadExecutions()
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) notFound.value = true
    else toastError(e)
  }
}
load()

async function loadExecutions() {
  try {
    executions.value = await api.executions({ serviceId: props.sid, workflowId: props.wid, limit: 10 })
  } catch {
    /* la lista es informativa */
  }
}

const stop = onExecution((e) => {
  if (e.serviceId === props.sid && e.workflowId === props.wid) loadExecutions()
  if (e.id === traceId.value) loadTrace()
})
onBeforeUnmount(stop)

watch(
  () => store.configVersion,
  () => {
    if (!dirty.value && !saving.value) load()
  },
)

onBeforeRouteLeave(async () => {
  if (!dirty.value) return true
  return confirmAction({
    title: 'Cambios sin guardar',
    text: 'Si sales ahora se perderán los cambios de este workflow.',
    confirmText: 'Salir sin guardar',
    danger: true,
  })
})

// -- pasos ------------------------------------------------------------------

const stepIdsSet = () => wf.value?.steps.map((s) => s.id) ?? []

function addStep(type: StepType) {
  if (!wf.value) return
  wf.value.steps.push(newStep(type, stepIdsSet()))
  openPanels.value = [...openPanels.value, wf.value.steps.length - 1]
  selectedStep.value = wf.value.steps.length - 1
}

function changeType(i: number, type: StepType) {
  if (!wf.value) return
  const old = wf.value.steps[i]
  const fresh = newStep(type, stepIdsSet())
  const keepNext = !['condition', 'switch', 'end', 'fail'].includes(type)
  const keepError = type !== 'end' && type !== 'fail'
  wf.value.steps[i] = {
    ...fresh,
    id: old.id,
    name: old.name,
    ...(keepNext && old.next ? { next: old.next } : {}),
    ...(keepError && old.onError ? { onError: old.onError } : {}),
  } as Step
}

function move(i: number, d: -1 | 1) {
  const list = wf.value?.steps
  if (!list || i + d < 0 || i + d >= list.length) return
  ;[list[i], list[i + d]] = [list[i + d], list[i]]
}

function duplicate(i: number) {
  if (!wf.value) return
  const copy: Step = JSON.parse(JSON.stringify(wf.value.steps[i]))
  copy.id = uniqueStepId(copy.id, stepIdsSet())
  wf.value.steps.splice(i + 1, 0, copy)
}

function removeStep(i: number) {
  wf.value?.steps.splice(i, 1)
  selectedStep.value = null
}

// En el diagrama, duplicar y eliminar conservan las conexiones de los demás pasos.
function duplicateInDiagram(i: number) {
  const w = wf.value
  if (!w) return
  const copy: Step = JSON.parse(JSON.stringify(w.steps[i]))
  copy.id = uniqueStepId(copy.id, stepIdsSet())
  const pos = w.layout?.[w.steps[i]!.id]
  appendStep(w.steps, copy)
  if (pos) (w.layout ??= {})[copy.id] = { x: pos.x + 40, y: pos.y + 60 }
  selectedStep.value = w.steps.length - 1
}

function removeInDiagram(i: number) {
  const w = wf.value
  if (!w) return
  const id = w.steps[i]?.id
  selectedStep.value = null
  removeGraphStep(w.steps, i)
  if (id && w.layout) delete w.layout[id]
}

// Renombrar un paso actualiza los saltos que apuntan a él y su posición.
watch(
  () => wf.value?.steps.map((s) => s.id) ?? [],
  (now, before) => {
    const w = wf.value
    if (!w || now.length !== before.length) return
    now.forEach((id, i) => {
      const old = before[i]
      if (!old || old === id || !id) return
      const others = now.filter((_, j) => j !== i)
      if (others.includes(id) || others.includes(old)) return
      renameRefs(w.steps, old, id)
      if (w.layout?.[old]) {
        w.layout[id] = w.layout[old]
        delete w.layout[old]
      }
    })
  },
)

const stepIssues = (i: number) => issues.value.filter((x) => x.field.startsWith(`steps[${i}]`))
const errorSteps = computed(() => {
  const set = new Set<number>()
  for (const x of issues.value) {
    const m = /^steps\[(\d+)\]/.exec(x.field)
    if (m) set.add(Number(m[1]))
  }
  return set
})

// -- recorrido de una ejecución ----------------------------------------------

const traceId = ref<string | null>(null)
const trace = ref<Trace | null>(null)

async function loadTrace() {
  const id = traceId.value
  if (!id) {
    trace.value = null
    return
  }
  try {
    const d = await api.execution(id)
    if (traceId.value !== id) return
    const visits: Trace['visits'] = {}
    const edges = new Set<string>()
    let prev: string | null = null
    let first: string | null = null
    for (const e of d.timeline) {
      const sid = e.stepId
      if (!sid) continue
      if (sid !== prev) {
        if (prev) edges.add(`${prev}>${sid}`)
        else first = sid
        visits[sid] ??= { count: 0, status: 'ok' }
        visits[sid].count++
        prev = sid
      }
      if (e.kind === 'step_failed') visits[sid]!.status = 'error'
      if (e.kind === 'error_handled') visits[sid]!.status = 'handled'
    }
    if (first) edges.add(`__inicio>${first}`)
    const ex = d.execution
    if (!['completed', 'failed', 'cancelled'].includes(ex.status) && ex.currentStep && visits[ex.currentStep]) {
      visits[ex.currentStep].status = 'running'
    }
    trace.value = { visits, edges, first }
  } catch (e) {
    toastError(e)
    traceId.value = null
  }
}
watch(traceId, loadTrace)

const traceItems = computed(() =>
  executions.value.map((e) => ({
    value: e.id,
    title: `${t(`executionStatus.${e.status}`)} · ${formatTime(e.createdMs)}`,
  })),
)

// -- guardar / ejecutar -----------------------------------------------------

function payload(): Workflow {
  const w: Workflow = JSON.parse(JSON.stringify(wf.value))
  if (!w.schedule) delete w.schedule
  // Solo posiciones de pasos que existen.
  const keep = new Set(['__inicio', ...w.steps.map((s) => s.id)])
  const layout = Object.fromEntries(Object.entries(w.layout ?? {}).filter(([k]) => keep.has(k)))
  if (Object.keys(layout).length) w.layout = layout
  else delete w.layout
  return w
}

async function save() {
  if (!wf.value || !dirty.value) return
  if (invalidJson.size) {
    toast('Hay JSON inválido en el editor; corrígelo antes de guardar.', 'error')
    return
  }
  saving.value = true
  issues.value = []
  try {
    const v = await api.updateWorkflow(props.sid, props.wid, payload())
    wf.value = normalize(v.workflow)
    usedBy.value = v.usedBy
    original.value = JSON.stringify(wf.value)
    toast('Workflow guardado')
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) {
      issues.value = e.issues
      toast(t('common.errorsFound'), 'error')
    } else toastError(e)
  } finally {
    saving.value = false
  }
}

function discard() {
  if (original.value) wf.value = JSON.parse(original.value)
  issues.value = []
}

async function remove() {
  const ok = await confirmAction({
    title: 'Eliminar workflow',
    text: `Se eliminará «${wf.value?.name}». Las ejecuciones pasadas se conservan.`,
    confirmText: 'Eliminar',
    danger: true,
  })
  if (!ok) return
  try {
    await api.deleteWorkflow(props.sid, props.wid)
    original.value = JSON.stringify(wf.value)
    toast('Workflow eliminado')
    router.push(`/servicios/${props.sid}?tab=workflows`)
  } catch (e) {
    toastError(e)
  }
}

const runDialog = ref(false)
const runInput = ref<Record<string, unknown> | undefined>({})
const runContext = ref<unknown>({ request: { body: { callback_url: 'http://127.0.0.1:4000/ejemplo/webhooks' } }, response: { body: { id: 'doc_123' } } })

async function run() {
  if (invalidJson.size) {
    toast('Corrige el JSON antes de ejecutar.', 'error')
    return
  }
  try {
    const e = await api.runWorkflow(props.sid, props.wid, runInput.value ?? {}, runContext.value)
    runDialog.value = false
    toast('Ejecución iniciada')
    if (view.value === 'diagram') {
      traceId.value = e.id
      loadExecutions()
    } else router.push(`/ejecuciones/${e.id}`)
  } catch (e) {
    toastError(e)
  }
}

function onKey(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault()
    save()
  }
}
</script>

<template>
  <div tabindex="-1" style="outline: none" @keydown="onKey">
    <EmptyState v-if="notFound" icon="mdi-help-circle-outline" title="Este workflow no existe" text="Puede que se haya eliminado o renombrado.">
      <VBtn :to="`/servicios/${sid}`">Volver al servicio</VBtn>
    </EmptyState>

    <template v-else-if="wf">
      <VCard class="mb-4 editor-bar">
        <VCardText class="py-3">
          <VBtn variant="text" size="small" prepend-icon="mdi-chevron-left" :to="`/servicios/${sid}?tab=workflows`" class="px-1 mb-1">
            {{ serviceName }}
          </VBtn>
          <div class="d-flex flex-wrap align-center gap-3">
            <VIcon icon="mdi-sitemap-outline" color="primary" />
            <span class="text-title-large font-weight-bold">{{ wf.name }}</span>
            <VChip v-if="!wf.enabled" size="small" variant="tonal">Deshabilitado</VChip>
            <VSpacer />
            <VChip v-if="dirty" color="primary" variant="tonal" size="small">Cambios sin guardar</VChip>
            <VBtn variant="text" prepend-icon="mdi-download" title="Descargar este workflow como JSON" @click="saveJson(payload(), `workflow-${wf.id}.json`)">
              Exportar
            </VBtn>
            <VBtn variant="tonal" prepend-icon="mdi-play" :disabled="dirty" @click="runDialog = true">Ejecutar</VBtn>
            <VBtn variant="text" :disabled="!dirty" @click="discard">Descartar</VBtn>
            <VBtn color="primary" :disabled="!dirty" :loading="saving" title="Ctrl+S" @click="save">{{ t('common.save') }}</VBtn>
          </div>
        </VCardText>
      </VCard>

      <IssuesList :issues="issues.filter((i) => !i.field.startsWith('steps['))" />

      <div class="d-flex flex-wrap align-center gap-3 mb-3">
        <div class="text-title-medium font-weight-bold">Flujo</div>
        <span v-if="view === 'list'" class="text-body-small text-medium-emphasis">Se ejecutan en orden salvo que un paso indique un salto.</span>
        <VSpacer />
        <template v-if="view === 'diagram'">
          <FSelect
            v-model="traceId"
            :items="traceItems"
            clearable
            hide-details
            placeholder="Ver recorrido de una ejecución"
            no-data-text="Aún no se ha ejecutado"
            prepend-inner-icon="mdi-map-marker-path"
            style="min-width: 280px; max-width: 320px"
          />
          <VBtn size="small" variant="text" prepend-icon="mdi-auto-fix" @click="diagram?.arrange()">Ordenar</VBtn>
        </template>
        <VBtnToggle v-model="view" mandatory color="primary" divided>
          <VBtn value="diagram" size="small" prepend-icon="mdi-graph-outline">Diagrama</VBtn>
          <VBtn value="list" size="small" prepend-icon="mdi-format-list-bulleted">Lista</VBtn>
        </VBtnToggle>
      </div>

      <!-- Diagrama: lienzo + inspector -->
      <div v-if="view === 'diagram'" class="studio mb-4">
        <WorkflowDiagram
          ref="diagram"
          v-model="wf"
          v-model:selected="selectedStep"
          :errors="errorSteps"
          :used-by="usedBy.length"
          :trace="trace"
          class="studio__canvas"
        />
        <aside class="studio__inspector">
          <VCard v-if="selectedIndex !== null" class="studio__panel">
            <VCardItem>
              <template #prepend><VIcon :icon="stepIcons[wf.steps[selectedIndex]!.type]" color="primary" /></template>
              <VCardTitle>{{ wf.steps[selectedIndex]!.name || wf.steps[selectedIndex]!.id }}</VCardTitle>
              <VCardSubtitle>{{ t(`stepType.${wf.steps[selectedIndex]!.type}`) }}</VCardSubtitle>
              <template #append>
                <VMenu>
                  <template #activator="{ props: menu }">
                    <VBtn v-bind="menu" icon="mdi-swap-horizontal" variant="text" size="small" title="Cambiar tipo" />
                  </template>
                  <VList density="compact">
                    <VListItem v-for="st in stepTypes" :key="st" :prepend-icon="stepIcons[st]" :title="t(`stepType.${st}`)" @click="changeType(selectedIndex, st)" />
                  </VList>
                </VMenu>
                <VBtn icon="mdi-content-duplicate" variant="text" size="small" title="Duplicar" @click="duplicateInDiagram(selectedIndex)" />
                <VBtn icon="mdi-delete-outline" variant="text" size="small" title="Eliminar (Supr)" @click="removeInDiagram(selectedIndex)" />
                <VBtn icon="mdi-close" variant="text" size="small" title="Cerrar" @click="selectedStep = null" />
              </template>
            </VCardItem>
            <VCardText>
              <IssuesList :issues="stepIssues(selectedIndex)" />
              <div v-if="trace?.visits[wf.steps[selectedIndex]!.id]" class="text-body-small mb-3">
                <VIcon icon="mdi-map-marker-path" size="small" class="me-1" />
                En la ejecución seleccionada se ejecutó {{ trace.visits[wf.steps[selectedIndex]!.id]!.count }} vez/veces.
                <RouterLink :to="`/ejecuciones/${traceId}`">Ver detalle</RouterLink>
              </div>
              <StepEditor v-model="wf.steps[selectedIndex]" :step-ids="stepIds" :workflows="siblings" :workflow-id="wf.id" compact />
            </VCardText>
          </VCard>
          <template v-else>
            <div class="d-flex align-center gap-2 mb-3 px-1">
              <VIcon icon="mdi-lightning-bolt" color="primary" />
              <span class="text-title-medium font-weight-bold">Disparador y ajustes</span>
            </div>
            <div class="text-body-small text-medium-emphasis mb-3 px-1">
              Selecciona un paso para editarlo. Une la salida de un paso con otro para decidir a dónde sigue; suelta una salida en
              un espacio vacío para crear el siguiente paso.
            </div>
            <WorkflowSettings
              v-model="wf"
              :sid="sid"
              :wid="wid"
              :used-by="usedBy"
              :executions="executions"
              :trace-id="traceId"
              can-trace
              @trace="traceId = $event"
              @remove="remove"
            />
          </template>
        </aside>
      </div>

      <!-- Lista -->
      <VRow v-else>
        <VCol cols="12" lg="8">
          <EmptyState v-if="!wf.steps.length" icon="mdi-format-list-numbered" title="Sin pasos" text="Agrega el primer paso del workflow." />

          <VExpansionPanels v-model="openPanels" multiple class="mb-4">
            <VExpansionPanel v-for="(s, i) in wf.steps" :key="i" :value="i">
              <VExpansionPanelTitle>
                <div class="d-flex align-center gap-3 w-100 me-2 min-w-0">
                  <span class="text-medium-emphasis text-body-small" style="width: 18px">{{ i + 1 }}</span>
                  <VIcon :icon="stepIcons[s.type]" color="primary" size="20" />
                  <span class="font-weight-medium text-no-wrap">{{ s.name || s.id }}</span>
                  <VChip size="x-small" variant="tonal" class="flex-shrink-0">{{ t(`stepType.${s.type}`) }}</VChip>
                  <code class="text-body-small text-medium-emphasis text-truncate">{{ stepSummary(s) }}</code>
                  <VIcon v-if="stepIssues(i).length" icon="mdi-alert-circle-outline" color="error" size="small" />
                  <VSpacer />
                  <div class="d-flex align-center" @click.stop>
                    <VMenu>
                      <template #activator="{ props: menu }">
                        <VBtn v-bind="menu" icon="mdi-swap-horizontal" variant="text" size="small" title="Cambiar tipo" />
                      </template>
                      <VList density="compact">
                        <VListItem v-for="st in stepTypes" :key="st" :prepend-icon="stepIcons[st]" :title="t(`stepType.${st}`)" @click="changeType(i, st)" />
                      </VList>
                    </VMenu>
                    <VBtn icon="mdi-arrow-up" variant="text" size="small" title="Subir" :disabled="i === 0" @click="move(i, -1)" />
                    <VBtn icon="mdi-arrow-down" variant="text" size="small" title="Bajar" :disabled="i === wf.steps.length - 1" @click="move(i, 1)" />
                    <VBtn icon="mdi-content-duplicate" variant="text" size="small" title="Duplicar" @click="duplicate(i)" />
                    <VBtn icon="mdi-delete-outline" variant="text" size="small" title="Eliminar" @click="removeStep(i)" />
                  </div>
                </div>
              </VExpansionPanelTitle>
              <VExpansionPanelText>
                <IssuesList :issues="stepIssues(i)" />
                <StepEditor v-model="wf.steps[i]" :step-ids="stepIds" :workflows="siblings" :workflow-id="wf.id" />
              </VExpansionPanelText>
            </VExpansionPanel>
          </VExpansionPanels>

          <VMenu>
            <template #activator="{ props: menu }">
              <VBtn v-bind="menu" prepend-icon="mdi-plus" variant="tonal">Agregar paso</VBtn>
            </template>
            <VList min-width="320">
              <VListItem
                v-for="st in stepTypes"
                :key="st"
                :prepend-icon="stepIcons[st]"
                :title="t(`stepType.${st}`)"
                :subtitle="t(`stepHint.${st}`)"
                @click="addStep(st)"
              />
            </VList>
          </VMenu>
        </VCol>

        <VCol cols="12" lg="4">
          <WorkflowSettings v-model="wf" :sid="sid" :wid="wid" :used-by="usedBy" :executions="executions" @remove="remove" />
        </VCol>
      </VRow>

      <VDialog v-model="runDialog" max-width="680">
        <VCard title="Ejecutar manualmente">
          <VCardText class="d-flex flex-column gap-4">
            <JsonEditor v-model="runInput" empty-as="object" label="Entrada (input)" hint="Objeto JSON disponible como input.*" />
            <JsonEditor
              v-model="runContext"
              label="Contexto simulado (opcional)"
              min-height="120px"
              hint="Simula la solicitud y respuesta originales: request.*, response.*"
            />
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn variant="text" @click="runDialog = false">Cancelar</VBtn>
            <VBtn color="primary" prepend-icon="mdi-play" @click="run">Ejecutar</VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </div>
</template>

<style scoped>
.editor-bar {
  position: sticky;
  top: calc(var(--v-layout-top, 0px) + 8px);
  z-index: 5;
}
.studio {
  display: flex;
  gap: 16px;
  height: calc(100vh - var(--v-layout-top, 0px) - 210px);
  min-height: 600px;
}
.studio__canvas {
  flex: 1 1 auto;
  min-width: 0;
}
.studio__inspector {
  flex: 0 0 440px;
  overflow: auto;
  padding-right: 2px;
}
@media (max-width: 1100px) {
  .studio {
    flex-direction: column;
    height: auto;
  }
  .studio__canvas {
    height: 620px;
  }
  .studio__inspector {
    flex-basis: auto;
  }
}
</style>
