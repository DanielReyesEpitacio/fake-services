<script setup lang="ts">
import { computed, provide, reactive, ref, watch } from 'vue'
import { onBeforeRouteLeave, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { store, toast, toastError } from '@/app/shared/state/live.ts'
import { confirmAction } from '@/app/shared/state/confirm.ts'
import CodeEditor from '@/app/shared/components/CodeEditor.vue'
import CopyButton from '@/app/shared/components/CopyButton.vue'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import MethodChip from '@/app/shared/components/MethodChip.vue'
import ParamTable from '../components/ParamTable.vue'
import ResponseEditor from '../components/ResponseEditor.vue'
import RuleEditor from '../components/RuleEditor.vue'
import TryRequest from '../components/TryRequest.vue'
import WorkflowLinksEditor from '../components/WorkflowLinksEditor.vue'
import ScenarioAdvanced from '@/app/modules/endpoints/components/ScenarioAdvanced.vue'
import { HTTP_METHODS, type BodyKind, type Endpoint, type Issue, type Scenario } from '@/infrastructure/api/types.ts'

const props = defineProps<{ sid: string; eid: string }>()
const router = useRouter()
const { t } = useI18n()

const ep = ref<Endpoint | null>(null)
const url = ref('')
const original = ref('')
const notFound = ref(false)
const issues = ref<Issue[]>([])
const tab = ref<'scenarios' | 'request' | 'general' | 'try'>('scenarios')
const openPanels = ref<number[]>([0])
const saving = ref(false)

const workflows = ref<{ id: string; name: string }[]>([])
api
  .workflows(props.sid)
  .then((list) => (workflows.value = list.map((w) => ({ id: w.id, name: w.name }))))
  .catch(() => {})

const invalidJson = reactive(new Set<symbol>())
provide('invalidJson', invalidJson)

const serviceName = computed(() => store.services.find((s) => s.id === props.sid)?.name ?? props.sid)
const dirty = computed(() => !!ep.value && JSON.stringify(ep.value) !== original.value)
const methodItems = HTTP_METHODS.map((m) => ({ value: m, title: m === '*' ? 'ANY' : m }))
const kindItems = computed(() =>
  (['any', 'json', 'xml', 'text', 'form', 'multipart', 'binary'] as BodyKind[]).map((value) => ({
    value,
    title: t(`bodyKind.${value}`),
  })),
)
const scenarioItems = computed(() => [
  { title: 'Ninguno (según reglas)', value: null },
  ...(ep.value?.scenarios.filter((s) => s.id).map((s) => ({ title: s.name, value: s.id })) ?? []),
])

function normalize(e: Endpoint): Endpoint {
  e.request ??= {}
  e.request.headers ??= []
  e.request.query ??= []
  e.request.pathParams ??= []
  e.activeScenario ??= null
  e.description ??= ''
  e.name ??= ''
  for (const s of e.scenarios) {
    s.rules ??= []
    s.ruleMode ??= 'all'
    s.enabled ??= true
    s.description ??= ''
    s.response.headers ??= {}
    s.response.body ??= { type: 'none' }
    s.workflows ??= []
  }
  return e
}

async function load() {
  try {
    const v = await api.endpoint(props.sid, props.eid)
    ep.value = normalize(v.endpoint)
    url.value = v.url
    original.value = JSON.stringify(ep.value)
    const body = ep.value.request?.body
    schemaText.value = body?.schema ? JSON.stringify(body.schema, null, 2) : ''
    exampleText.value = body?.example !== undefined ? JSON.stringify(body.example, null, 2) : ''
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) notFound.value = true
    else toastError(e)
  }
}
load()

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
    text: 'Si sales ahora se perderán los cambios de este endpoint.',
    confirmText: 'Salir sin guardar',
    danger: true,
  })
})

// -- escenarios ------------------------------------------------------------

function addScenario() {
  if (!ep.value) return
  ep.value.scenarios.push({
    id: '',
    name: `Escenario ${ep.value.scenarios.length + 1}`,
    description: '',
    enabled: true,
    ruleMode: 'all',
    rules: [],
    response: { status: 400, headers: {}, body: { type: 'json', content: { error: 'bad_request' } } },
    workflows: [],
  })
  openPanels.value = [...openPanels.value, ep.value.scenarios.length - 1]
}

function duplicate(i: number) {
  if (!ep.value) return
  const copy: Scenario = JSON.parse(JSON.stringify(ep.value.scenarios[i]))
  copy.id = ''
  copy.name = `${copy.name} (copia)`
  ep.value.scenarios.splice(i + 1, 0, copy)
}

function move(i: number, d: -1 | 1) {
  const list = ep.value?.scenarios
  if (!list || i + d < 0 || i + d >= list.length) return
  ;[list[i], list[i + d]] = [list[i + d], list[i]]
}

async function removeScenario(i: number) {
  if (!ep.value) return
  const s = ep.value.scenarios[i]
  const ok = await confirmAction({
    title: 'Eliminar escenario',
    text: `¿Eliminar “${s.name}”? El cambio se aplica al guardar.`,
    confirmText: 'Eliminar',
    danger: true,
  })
  if (!ok) return
  if (ep.value.activeScenario === s.id) ep.value.activeScenario = null
  ep.value.scenarios.splice(i, 1)
}

const scenarioIssues = (i: number) => issues.value.filter((x) => x.field.startsWith(`scenarios[${i}]`))

// -- body esperado ----------------------------------------------------------

const schemaText = ref('')
const exampleText = ref('')
const schemaError = ref('')
const bodySymbol = Symbol('body-schema')

const hasBody = computed({
  get: () => !!ep.value?.request?.body,
  set: (v) => {
    if (!ep.value?.request) return
    ep.value.request.body = v ? { kind: 'json', required: true } : null
    if (!v) invalidJson.delete(bodySymbol)
  },
})

watch([schemaText, exampleText], ([schema, example]) => {
  const body = ep.value?.request?.body
  if (!body) return
  try {
    const s = schema.trim() ? JSON.parse(schema) : undefined
    const ex = example.trim() ? JSON.parse(example) : undefined
    schemaError.value = ''
    invalidJson.delete(bodySymbol)
    if (JSON.stringify(s) !== JSON.stringify(body.schema)) body.schema = s
    if (JSON.stringify(ex) !== JSON.stringify(body.example)) body.example = ex
  } catch (e) {
    schemaError.value = (e as Error).message
    invalidJson.add(bodySymbol)
  }
})

const customValidation = computed({
  get: () => !!ep.value?.onValidationError,
  set: (v) => {
    if (!ep.value) return
    ep.value.onValidationError = v
      ? {
          status: 422,
          headers: {},
          body: { type: 'json', content: { error: 'invalid_request', errors: '{{ validation.errors }}' } },
        }
      : null
  },
})

// -- guardar ---------------------------------------------------------------

async function save() {
  if (!ep.value || !dirty.value) return
  if (invalidJson.size) {
    toast('Hay JSON inválido en el editor; corrígelo antes de guardar.', 'error')
    return
  }
  saving.value = true
  issues.value = []
  try {
    const v = await api.updateEndpoint(props.sid, props.eid, ep.value)
    ep.value = normalize(v.endpoint)
    url.value = v.url
    original.value = JSON.stringify(ep.value)
    toast('Endpoint guardado')
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
  if (!original.value) return
  ep.value = JSON.parse(original.value)
  issues.value = []
}

async function remove() {
  if (!ep.value) return
  const ok = await confirmAction({
    title: 'Eliminar endpoint',
    text: `Se eliminará ${ep.value.method} ${ep.value.path}. Esta acción no se puede deshacer.`,
    confirmText: 'Eliminar',
    danger: true,
  })
  if (!ok) return
  try {
    await api.deleteEndpoint(props.sid, props.eid)
    original.value = JSON.stringify(ep.value)
    toast('Endpoint eliminado')
    router.push(`/servicios/${props.sid}`)
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
    <EmptyState v-if="notFound" icon="mdi-help-circle-outline" title="Este endpoint no existe" text="Puede que se haya eliminado o renombrado.">
      <VBtn :to="`/servicios/${sid}`">Volver al servicio</VBtn>
    </EmptyState>

    <template v-else-if="ep">
      <VCard class="mb-4 editor-bar">
        <VCardText class="py-3">
          <VBtn variant="text" size="small" prepend-icon="mdi-chevron-left" :to="`/servicios/${sid}`" class="px-1 mb-1">
            {{ serviceName }}
          </VBtn>
          <div class="d-flex flex-wrap align-center gap-3">
            <MethodChip :method="ep.method" />
            <code class="text-title-medium">{{ ep.path }}</code>
            <span v-if="ep.name" class="text-medium-emphasis">· {{ ep.name }}</span>
            <VSpacer />
            <VChip v-if="dirty" color="primary" variant="tonal" size="small">Cambios sin guardar</VChip>
            <VBtn variant="text" :disabled="!dirty" @click="discard">Descartar</VBtn>
            <VBtn color="primary" :disabled="!dirty" :loading="saving" title="Ctrl+S" @click="save">{{ t('common.save') }}</VBtn>
          </div>
          <div class="d-flex align-center gap-1 mt-1 text-body-small text-medium-emphasis">
            <code>{{ url }}</code><CopyButton :text="url" />
          </div>
        </VCardText>
      </VCard>

      <IssuesList :issues="issues" />

      <VTabs v-model="tab" class="mb-4">
        <VTab value="scenarios">Escenarios ({{ ep.scenarios.length }})</VTab>
        <VTab value="request">Solicitud esperada</VTab>
        <VTab value="general">General</VTab>
        <VTab value="try">Probar</VTab>
      </VTabs>

      <VWindow v-model="tab">
        <!-- Escenarios -->
        <VWindowItem value="scenarios">
          <VCard class="mb-4">
            <VCardText class="d-flex flex-wrap align-center gap-4">
              <span class="text-body-medium text-medium-emphasis flex-grow-1">
                Se evalúan en orden y responde el primero cuyas reglas se cumplan.
              </span>
              <FSelect v-model="ep.activeScenario" :items="scenarioItems" label="Forzar escenario" hide-details style="max-width: 300px" />
            </VCardText>
          </VCard>

          <EmptyState v-if="!ep.scenarios.length" icon="mdi-call-split" title="Sin escenarios" text="Sin escenarios el endpoint responde 501.">
            <VBtn color="primary" prepend-icon="mdi-plus" @click="addScenario">Agregar escenario</VBtn>
          </EmptyState>

          <VExpansionPanels v-model="openPanels" multiple class="mb-4">
            <VExpansionPanel
              v-for="(s, i) in ep.scenarios"
              :key="i"
              :value="i"
              :class="{ 'scenario-forced': ep.activeScenario && ep.activeScenario === s.id, 'scenario-off': s.enabled === false }"
            >
              <VExpansionPanelTitle>
                <div class="d-flex align-center gap-3 w-100 me-2">
                  <span class="text-medium-emphasis text-body-small" style="width: 18px">{{ i + 1 }}</span>
                  <span class="font-weight-medium">{{ s.name }}</span>
                  <VChip size="small" label :color="s.response.status < 400 ? 'success' : 'error'" variant="tonal" class="mono">
                    {{ s.response.status }}
                  </VChip>
                  <span class="text-body-small text-medium-emphasis">
                    {{ s.rules?.length ? `${s.rules.length} regla${s.rules.length === 1 ? '' : 's'}` : 'por defecto' }}
                    <template v-if="s.response.delayMs"> · {{ s.response.delayMs }} ms</template>
                  </span>
                  <VChip v-if="s.workflows?.length" size="x-small" variant="tonal" prepend-icon="mdi-sitemap-outline">
                    {{ s.workflows.length }} workflow{{ s.workflows.length === 1 ? '' : 's' }}
                  </VChip>
                  <VChip v-if="ep.activeScenario && ep.activeScenario === s.id" size="x-small" color="warning">forzado</VChip>
                  <VChip v-if="s.probability != null" size="x-small" variant="tonal">{{ Math.round(s.probability * 100) }}%</VChip>
                  <VChip v-if="s.resource" size="x-small" variant="tonal" prepend-icon="mdi-database-outline">{{ s.resource.operation }} {{ s.resource.collection }}</VChip>
                  <VIcon v-if="scenarioIssues(i).length" icon="mdi-alert-circle-outline" color="error" size="small" />
                  <VSpacer />
                  <div class="d-flex align-center" @click.stop>
                    <VSwitch :model-value="s.enabled !== false" hide-details density="compact" aria-label="Habilitado" @update:model-value="s.enabled = !!$event" />
                    <VBtn icon="mdi-arrow-up" variant="text" size="small" title="Subir" :disabled="i === 0" @click="move(i, -1)" />
                    <VBtn icon="mdi-arrow-down" variant="text" size="small" title="Bajar" :disabled="i === ep.scenarios.length - 1" @click="move(i, 1)" />
                    <VBtn icon="mdi-content-duplicate" variant="text" size="small" title="Duplicar" @click="duplicate(i)" />
                    <VBtn icon="mdi-delete-outline" variant="text" size="small" title="Eliminar" @click="removeScenario(i)" />
                  </div>
                </div>
              </VExpansionPanelTitle>
              <VExpansionPanelText>
                <VRow>
                  <VCol cols="12" xl="6" class="d-flex flex-column gap-3">
                    <FTextField v-model="s.name" label="Nombre" />
                    <FTextField v-model="s.description" label="Descripción" placeholder="Cuándo usar este escenario" />
                    <div>
                      <div class="text-label-medium text-medium-emphasis mb-2">Condiciones</div>
                      <RuleEditor v-model="s.rules!" v-model:mode="s.ruleMode" />
                    </div>
                    <code v-if="s.id" class="text-body-small text-disabled">id: {{ s.id }}</code>
                    <IssuesList :issues="scenarioIssues(i)" />
                  </VCol>
                  <VCol cols="12" xl="6">
                    <ResponseEditor v-model="s.response" />
                  </VCol>
                  <VCol cols="12">
                    <div class="text-label-medium text-medium-emphasis mb-2">Avanzado</div>
                    <ScenarioAdvanced v-model="ep.scenarios[i]!" />
                  </VCol>
                  <VCol cols="12">
                    <div class="text-label-medium text-medium-emphasis mb-2">Workflows que inicia</div>
                    <WorkflowLinksEditor v-model="s.workflows!" :sid="sid" :workflows="workflows" />
                  </VCol>
                </VRow>
              </VExpansionPanelText>
            </VExpansionPanel>
          </VExpansionPanels>

          <VBtn prepend-icon="mdi-plus" variant="tonal" @click="addScenario">Agregar escenario</VBtn>
        </VWindowItem>

        <!-- Solicitud esperada -->
        <VWindowItem value="request">
          <div class="d-flex flex-column gap-4" style="max-width: 1100px">
            <div class="text-body-medium text-medium-emphasis">
              Si la solicitud no cumple estas reglas, la simulación responde con error de validación (422 por defecto).
            </div>
            <VCard title="Headers">
              <VCardText><ParamTable v-model="ep.request!.headers!" name-placeholder="Authorization" add-label="Agregar header" /></VCardText>
            </VCard>
            <VCard title="Parámetros de query">
              <VCardText><ParamTable v-model="ep.request!.query!" name-placeholder="page" add-label="Agregar parámetro" /></VCardText>
            </VCard>
            <VCard title="Parámetros de ruta">
              <VCardText><ParamTable v-model="ep.request!.pathParams!" name-placeholder="id" add-label="Agregar parámetro" /></VCardText>
            </VCard>
            <VCard title="Body">
              <template #append><VSwitch v-model="hasBody" label="Validar body" hide-details color="primary" /></template>
              <VCardText v-if="ep.request?.body" class="d-flex flex-column gap-3">
                <div class="d-flex align-center gap-4">
                  <FSelect v-model="ep.request.body.kind" :items="kindItems" label="Tipo" hide-details style="max-width: 280px" />
                  <VCheckbox v-model="ep.request.body.required" label="Obligatorio" hide-details />
                </div>
                <VRow>
                  <VCol cols="12" md="6"><CodeEditor v-model="schemaText" language="json" min-height="160px" label="JSON Schema (opcional)" /></VCol>
                  <VCol cols="12" md="6">
                    <CodeEditor v-model="exampleText" language="json" min-height="160px" label="Ejemplo (opcional)" />
                    <div class="text-body-small text-medium-emphasis mt-1">Se usa para precargar la pestaña Probar.</div>
                  </VCol>
                </VRow>
                <div v-if="schemaError" class="text-body-small text-error">JSON inválido: {{ schemaError }}</div>
              </VCardText>
            </VCard>
            <VCard title="Respuesta ante datos inválidos">
              <template #append><VSwitch v-model="customValidation" label="Personalizar" hide-details color="primary" /></template>
              <VCardText>
                <div v-if="!ep.onValidationError" class="text-body-small text-medium-emphasis">
                  Por defecto: HTTP 422 con la lista de errores. Personalízala para imitar el formato del proveedor.
                </div>
                <template v-else>
                  <div class="text-body-small text-medium-emphasis mb-3">
                    Usa <code class="inline" v-pre>{{ validation.errors }}</code> para incluir los errores.
                  </div>
                  <ResponseEditor v-model="ep.onValidationError" />
                </template>
              </VCardText>
            </VCard>
          </div>
        </VWindowItem>

        <!-- General -->
        <VWindowItem value="general">
          <div class="d-flex flex-column gap-4" style="max-width: 860px">
            <VCard>
              <VCardText class="d-flex flex-column gap-3">
                <div class="d-flex gap-2">
                  <FSelect v-model="ep.method" :items="methodItems" label="Método" class="mono" style="max-width: 140px" />
                  <FTextField
                    v-model="ep.path"
                    label="Ruta"
                    class="mono"
                    hint="Parámetros: {id}. Comodín final: {*resto}. Las rutas literales tienen prioridad."
                    persistent-hint
                  />
                </div>
                <FTextField v-model="ep.name" label="Nombre" />
                <FTextarea v-model="ep.description" label="Descripción" rows="3" />
                <VSwitch v-model="ep.enabled" label="Endpoint habilitado" color="primary" hide-details />
                <FTextField :model-value="ep.id" label="Identificador" class="mono" disabled />
              </VCardText>
            </VCard>
            <div>
              <VBtn color="error" variant="tonal" prepend-icon="mdi-delete-outline" @click="remove">Eliminar endpoint</VBtn>
            </div>
          </div>
        </VWindowItem>

        <!-- Probar -->
        <VWindowItem value="try">
          <div style="max-width: 1000px">
            <VAlert v-if="dirty" type="warning" variant="tonal" class="mb-4">
              Guarda los cambios para probarlos: la simulación usa la configuración guardada.
            </VAlert>
            <VCard>
              <VCardText><TryRequest :endpoint="ep" :url="url" /></VCardText>
            </VCard>
          </div>
        </VWindowItem>
      </VWindow>
    </template>
  </div>
</template>

<style scoped>
.editor-bar {
  position: sticky;
  top: calc(var(--v-layout-top, 0px) + 8px);
  z-index: 5;
}
.scenario-forced {
  box-shadow: 0 0 0 2px rgb(var(--v-theme-warning)) !important;
}
.scenario-off {
  opacity: 0.6;
}
</style>
