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
import WorkflowDiagram from '../components/WorkflowDiagram.vue'
import ExecutionStatusChip from '@/app/modules/executions/components/ExecutionStatusChip.vue'
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
const view = ref<'list' | 'diagram'>(localStorage.getItem('fakes.workflowView') === 'diagram' ? 'diagram' : 'list')
watch(view, (v) => localStorage.setItem('fakes.workflowView', v))
const selectedStep = ref<number | null>(null)

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

const stepTypes: StepType[] = ['wait', 'http', 'condition', 'setVariables', 'resource', 'emit', 'runWorkflow', 'fail', 'end']
const stepIcons: Record<StepType, string> = {
  wait: 'mdi-timer-sand',
  http: 'mdi-send',
  emit: 'mdi-broadcast',
  setVariables: 'mdi-variable',
  condition: 'mdi-call-split',
  runWorkflow: 'mdi-sitemap-outline',
  resource: 'mdi-database-outline',
  fail: 'mdi-alert-octagon-outline',
  end: 'mdi-flag-checkered',
}

function uniqueStepId(base: string): string {
  const ids = new Set(stepIds.value)
  if (!ids.has(base)) return base
  let n = 2
  while (ids.has(`${base}-${n}`)) n++
  return `${base}-${n}`
}

function newStep(type: StepType): Step {
  const id = uniqueStepId(
    { wait: 'espera', http: 'webhook', emit: 'evento', setVariables: 'variables', condition: 'condicion', runWorkflow: 'subflujo', resource: 'recurso', fail: 'error', end: 'fin' }[type],
  )
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

function addStep(type: StepType) {
  if (!wf.value) return
  wf.value.steps.push(newStep(type))
  openPanels.value = [...openPanels.value, wf.value.steps.length - 1]
  selectedStep.value = wf.value.steps.length - 1
}

function changeType(i: number, type: StepType) {
  if (!wf.value) return
  const old = wf.value.steps[i]
  const fresh = newStep(type)
  wf.value.steps[i] = { ...fresh, id: old.id, name: old.name, next: old.next } as Step
}

function move(i: number, d: -1 | 1) {
  const list = wf.value?.steps
  if (!list || i + d < 0 || i + d >= list.length) return
  ;[list[i], list[i + d]] = [list[i + d], list[i]]
}

function duplicate(i: number) {
  if (!wf.value) return
  const copy: Step = JSON.parse(JSON.stringify(wf.value.steps[i]))
  copy.id = uniqueStepId(copy.id)
  wf.value.steps.splice(i + 1, 0, copy)
}

function removeStep(i: number) {
  wf.value?.steps.splice(i, 1)
  selectedStep.value = null
}

function stepSummary(s: Step): string {
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
      return `${s.conditions.length} condición(es) → ${s.onTrue ?? 'siguiente'} / ${s.onFalse ?? 'siguiente'}`
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

const stepIssues = (i: number) => issues.value.filter((x) => x.field.startsWith(`steps[${i}]`))
const errorSteps = computed(() => {
  const set = new Set<number>()
  for (const x of issues.value) {
    const m = /^steps\[(\d+)\]/.exec(x.field)
    if (m) set.add(Number(m[1]))
  }
  return set
})

// -- programación -----------------------------------------------------------

const scheduled = computed({
  get: () => !!wf.value?.schedule,
  set: (v) => {
    if (wf.value) wf.value.schedule = v ? { everyMs: 60000 } : null
  },
})

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
    router.push(`/ejecuciones/${e.id}`)
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

      <VRow>
        <VCol cols="12" lg="8">
          <div class="d-flex align-center gap-3 mb-3">
            <div class="text-title-medium font-weight-bold">Pasos</div>
            <span class="text-body-small text-medium-emphasis">Se ejecutan en orden salvo que un paso indique un salto.</span>
            <VSpacer />
            <VBtn v-if="view === 'diagram'" size="small" variant="text" prepend-icon="mdi-auto-fix" @click="diagram?.arrange()">Ordenar</VBtn>
            <VBtnToggle v-model="view" mandatory color="primary" divided>
              <VBtn value="list" size="small" prepend-icon="mdi-format-list-bulleted">Lista</VBtn>
              <VBtn value="diagram" size="small" prepend-icon="mdi-graph-outline">Diagrama</VBtn>
            </VBtnToggle>
          </div>

          <template v-if="view === 'diagram'">
            <WorkflowDiagram
              ref="diagram"
              v-model="wf"
              v-model:selected="selectedStep"
              :icons="stepIcons"
              :summary="stepSummary"
              :errors="errorSteps"
              class="mb-4"
            />
            <VCard v-if="selectedStep !== null && wf.steps[selectedStep]" class="mb-4">
              <VCardItem>
                <template #prepend><VIcon :icon="stepIcons[wf.steps[selectedStep]!.type]" color="primary" /></template>
                <VCardTitle>{{ wf.steps[selectedStep]!.name || wf.steps[selectedStep]!.id }}</VCardTitle>
                <VCardSubtitle>Paso {{ selectedStep + 1 }} · {{ t(`stepType.${wf.steps[selectedStep]!.type}`) }}</VCardSubtitle>
                <template #append>
                  <VMenu>
                    <template #activator="{ props: menu }">
                      <VBtn v-bind="menu" icon="mdi-swap-horizontal" variant="text" size="small" title="Cambiar tipo" />
                    </template>
                    <VList density="compact">
                      <VListItem v-for="st in stepTypes" :key="st" :prepend-icon="stepIcons[st]" :title="t(`stepType.${st}`)" @click="changeType(selectedStep, st)" />
                    </VList>
                  </VMenu>
                  <VBtn icon="mdi-content-duplicate" variant="text" size="small" title="Duplicar" @click="duplicate(selectedStep)" />
                  <VBtn icon="mdi-delete-outline" variant="text" size="small" title="Eliminar" @click="removeStep(selectedStep)" />
                  <VBtn icon="mdi-close" variant="text" size="small" title="Cerrar" @click="selectedStep = null" />
                </template>
              </VCardItem>
              <VCardText>
                <IssuesList :issues="stepIssues(selectedStep)" />
                <StepEditor v-model="wf.steps[selectedStep]" :step-ids="stepIds" :workflows="siblings" :workflow-id="wf.id" />
              </VCardText>
            </VCard>
            <div v-else-if="wf.steps.length" class="text-body-small text-medium-emphasis mb-4">
              Haz clic en un paso para editarlo.
            </div>
          </template>

          <EmptyState v-if="view === 'list' && !wf.steps.length" icon="mdi-format-list-numbered" title="Sin pasos" text="Agrega el primer paso del workflow." />

          <VExpansionPanels v-if="view === 'list'" v-model="openPanels" multiple class="mb-4">
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

        <VCol cols="12" lg="4" class="d-flex flex-column gap-4">
          <VCard title="General">
            <VCardText class="d-flex flex-column gap-3">
              <FTextField v-model="wf.name" label="Nombre" />
              <FTextarea v-model="wf.description" label="Descripción" rows="2" />
              <FTextField :model-value="wf.id" label="Identificador" class="mono" disabled />
              <VSwitch v-model="wf.enabled" label="Habilitado" color="primary" hide-details />
              <FTextField v-model.number="wf.maxSteps" label="Pasos máximos por ejecución" type="number" min="1" hint="Evita bucles infinitos." persistent-hint />
            </VCardText>
          </VCard>

          <VCard title="Ejecución periódica">
            <VCardText class="d-flex flex-column gap-3">
              <VSwitch v-model="scheduled" label="Ejecutar cada cierto tiempo" color="primary" hide-details />
              <FTextField
                v-if="wf.schedule"
                v-model.number="wf.schedule.everyMs"
                label="Intervalo (ms)"
                type="number"
                min="1000"
                hint="Solo mientras el servicio está iniciado."
                persistent-hint
              />
            </VCardText>
          </VCard>

          <VCard title="Variables iniciales">
            <VCardText>
              <JsonEditor v-model="wf.variables" empty-as="object" hint="Disponibles como vars.* (se combinan con las del servicio)." />
            </VCardText>
          </VCard>

          <VCard title="Lo inician">
            <VCardText>
              <div v-if="!usedBy.length" class="text-body-small text-medium-emphasis">
                Ningún escenario lo inicia todavía. Vincúlalo desde un endpoint, en la sección «Workflows» de un escenario.
              </div>
              <VList v-else density="compact">
                <VListItem
                  v-for="u in usedBy"
                  :key="`${u.endpointId}/${u.scenarioId}`"
                  :to="`/servicios/${sid}/endpoints/${u.endpointId}`"
                  prepend-icon="mdi-routes"
                  :title="u.endpointId"
                  :subtitle="`Escenario: ${u.scenarioId}`"
                />
              </VList>
            </VCardText>
          </VCard>

          <VCard title="Ejecuciones recientes">
            <template #append><VBtn variant="text" size="small" :to="`/ejecuciones?workflow=${wid}&servicio=${sid}`">Ver todas</VBtn></template>
            <VList v-if="executions.length" density="compact">
              <VListItem v-for="e in executions" :key="e.id" :to="`/ejecuciones/${e.id}`">
                <template #prepend><ExecutionStatusChip :status="e.status" class="me-3" /></template>
                <VListItemTitle class="text-body-small">{{ t(`trigger.${e.trigger}`) }}</VListItemTitle>
                <template #append><span class="text-body-small text-medium-emphasis">{{ formatTime(e.createdMs) }}</span></template>
              </VListItem>
            </VList>
            <VCardText v-else class="text-body-small text-medium-emphasis">Aún no se ha ejecutado.</VCardText>
          </VCard>

          <div>
            <VBtn color="error" variant="tonal" prepend-icon="mdi-delete-outline" @click="remove">Eliminar workflow</VBtn>
          </div>
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
</style>
