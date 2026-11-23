<script setup lang="ts">
import { computed, provide, reactive, ref, watch } from 'vue'
import { onBeforeRouteLeave, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowDown, ArrowUp, ChevronDown, ChevronLeft, ChevronRight, Copy, Plus, Trash2 } from 'lucide-vue-next'
import { api, ApiError } from '../lib/api'
import { store, toast, toastError } from '../lib/store'
import CodeEditor from '../components/CodeEditor.vue'
import CopyButton from '../components/CopyButton.vue'
import EmptyState from '../components/EmptyState.vue'
import IssuesList from '../components/IssuesList.vue'
import MethodBadge from '../components/MethodBadge.vue'
import ParamTable from '../components/ParamTable.vue'
import ResponseEditor from '../components/ResponseEditor.vue'
import RuleEditor from '../components/RuleEditor.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
import TryRequest from '../components/TryRequest.vue'
import { HTTP_METHODS, type BodyKind, type Endpoint, type Issue, type Scenario } from '../lib/types'

const props = defineProps<{ sid: string; eid: string }>()
const router = useRouter()
const { t } = useI18n()

const ep = ref<Endpoint | null>(null)
const url = ref('')
const original = ref('')
const notFound = ref(false)
const issues = ref<Issue[]>([])
const tab = ref<'scenarios' | 'request' | 'general' | 'try'>('scenarios')
const open = reactive(new Set<number>([0]))
const saving = ref(false)

const invalidJson = reactive(new Set<symbol>())
provide('invalidJson', invalidJson)

const serviceName = computed(() => store.services.find((s) => s.id === props.sid)?.name ?? props.sid)
const dirty = computed(() => !!ep.value && JSON.stringify(ep.value) !== original.value)

function normalize(e: Endpoint): Endpoint {
  e.request ??= {}
  e.request.headers ??= []
  e.request.query ??= []
  e.request.pathParams ??= []
  e.activeScenario ??= null
  for (const s of e.scenarios) {
    s.rules ??= []
    s.ruleMode ??= 'all'
    s.enabled ??= true
    s.response.headers ??= {}
    s.response.body ??= { type: 'none' }
  }
  return e
}

async function load() {
  try {
    const v = await api.endpoint(props.sid, props.eid)
    ep.value = normalize(v.endpoint)
    url.value = v.url
    original.value = JSON.stringify(ep.value)
    schemaText.value = ep.value.request?.body?.schema ? JSON.stringify(ep.value.request.body.schema, null, 2) : ''
    exampleText.value =
      ep.value.request?.body?.example !== undefined ? JSON.stringify(ep.value.request.body.example, null, 2) : ''
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

onBeforeRouteLeave(() => {
  if (dirty.value && !confirm('Tienes cambios sin guardar. ¿Salir de todos modos?')) return false
})

// -- escenarios ------------------------------------------------------------

function addScenario() {
  if (!ep.value) return
  ep.value.scenarios.push({
    id: '',
    name: `Escenario ${ep.value.scenarios.length + 1}`,
    enabled: true,
    ruleMode: 'all',
    rules: [],
    response: { status: 400, headers: {}, body: { type: 'json', content: { error: 'bad_request' } } },
  })
  open.add(ep.value.scenarios.length - 1)
}

function duplicate(i: number) {
  if (!ep.value) return
  const copy: Scenario = JSON.parse(JSON.stringify(ep.value.scenarios[i]))
  copy.id = ''
  copy.name = `${copy.name} (copia)`
  ep.value.scenarios.splice(i + 1, 0, copy)
  open.add(i + 1)
}

function move(i: number, d: -1 | 1) {
  const list = ep.value?.scenarios
  if (!list || i + d < 0 || i + d >= list.length) return
  ;[list[i], list[i + d]] = [list[i + d], list[i]]
  const a = open.has(i)
  const b = open.has(i + d)
  a ? open.add(i + d) : open.delete(i + d)
  b ? open.add(i) : open.delete(i)
}

function removeScenario(i: number) {
  if (!ep.value) return
  const s = ep.value.scenarios[i]
  if (!confirm(`¿Eliminar el escenario “${s.name}”?`)) return
  if (ep.value.activeScenario === s.id) ep.value.activeScenario = null
  ep.value.scenarios.splice(i, 1)
  open.delete(i)
}

function toggleOpen(i: number) {
  open.has(i) ? open.delete(i) : open.add(i)
}

function scenarioIssues(i: number) {
  return issues.value.filter((x) => x.field.startsWith(`scenarios[${i}]`))
}

// -- body esperado ----------------------------------------------------------

const schemaText = ref('')
const exampleText = ref('')
const schemaError = ref('')
const kinds: BodyKind[] = ['any', 'json', 'xml', 'text', 'form', 'multipart', 'binary']
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

// -- respuesta de validación ---------------------------------------------

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
  if (!ep.value) return
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
  if (!ep.value || !confirm(`¿Eliminar el endpoint ${ep.value.method} ${ep.value.path}?`)) return
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
    if (dirty.value) save()
  }
}
</script>

<template>
  <div class="min-h-full" tabindex="-1" @keydown="onKey">
    <EmptyState v-if="notFound" title="Este endpoint no existe" text="Puede que se haya eliminado o renombrado.">
      <RouterLink :to="`/servicios/${sid}`" class="btn">Volver al servicio</RouterLink>
    </EmptyState>

    <template v-else-if="ep">
      <!-- Barra superior -->
      <div class="sticky top-0 z-10 border-b border-zinc-200 bg-white/90 px-8 py-3 backdrop-blur">
        <RouterLink :to="`/servicios/${sid}`" class="inline-flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-800">
          <ChevronLeft class="size-3.5" /> {{ serviceName }}
        </RouterLink>
        <div class="mt-1 flex items-center gap-3">
          <MethodBadge :method="ep.method" />
          <code class="truncate text-base font-medium">{{ ep.path }}</code>
          <span v-if="ep.name" class="truncate text-sm text-zinc-500">· {{ ep.name }}</span>
          <div class="ml-auto flex items-center gap-2">
            <span v-if="dirty" class="text-xs text-brand-700">Cambios sin guardar</span>
            <button class="btn" :disabled="!dirty" @click="discard">Descartar</button>
            <button class="btn btn-primary" :disabled="!dirty || saving" title="Ctrl+S" @click="save">
              {{ saving ? t('common.saving') : t('common.save') }}
            </button>
          </div>
        </div>
        <div class="mt-1 flex items-center gap-1.5 text-xs text-zinc-500">
          <code>{{ url }}</code>
          <CopyButton :text="url" class="hover:text-zinc-800" />
        </div>
      </div>

      <div class="space-y-5 px-8 py-5">
        <IssuesList :issues="issues" />

        <nav class="flex border-b border-zinc-200">
          <button class="tab" :class="{ 'tab-active': tab === 'scenarios' }" @click="tab = 'scenarios'">
            Escenarios ({{ ep.scenarios.length }})
          </button>
          <button class="tab" :class="{ 'tab-active': tab === 'request' }" @click="tab = 'request'">Solicitud esperada</button>
          <button class="tab" :class="{ 'tab-active': tab === 'general' }" @click="tab = 'general'">General</button>
          <button class="tab" :class="{ 'tab-active': tab === 'try' }" @click="tab = 'try'">Probar</button>
        </nav>

        <!-- Escenarios -->
        <section v-if="tab === 'scenarios'" class="space-y-3">
          <div class="flex items-center justify-between gap-4 rounded-lg bg-zinc-100 px-3 py-2 text-sm">
            <span class="text-zinc-600">
              Se evalúan en orden y responde el primero cuyas reglas se cumplan.
            </span>
            <label class="flex items-center gap-2 whitespace-nowrap">
              <span class="text-zinc-600">Forzar escenario:</span>
              <select v-model="ep.activeScenario" class="input w-52 py-1">
                <option :value="null">Ninguno (según reglas)</option>
                <option v-for="s in ep.scenarios.filter((s) => s.id)" :key="s.id" :value="s.id">{{ s.name }}</option>
              </select>
            </label>
          </div>

          <EmptyState v-if="!ep.scenarios.length" title="Sin escenarios" text="Sin escenarios el endpoint responde 501.">
            <button class="btn btn-primary" @click="addScenario"><Plus class="size-4" /> Agregar escenario</button>
          </EmptyState>

          <div
            v-for="(s, i) in ep.scenarios"
            :key="i"
            class="card"
            :class="{
              'ring-2 ring-amber-300': ep.activeScenario && ep.activeScenario === s.id,
              'opacity-60': s.enabled === false,
              'ring-2 ring-rose-200': scenarioIssues(i).length,
            }"
          >
            <div class="flex items-center gap-2 px-3 py-2">
              <button class="btn btn-ghost btn-sm" :aria-label="open.has(i) ? 'Contraer' : 'Expandir'" @click="toggleOpen(i)">
                <ChevronDown v-if="open.has(i)" class="size-4" />
                <ChevronRight v-else class="size-4" />
              </button>
              <span class="w-6 text-center text-xs text-zinc-400">{{ i + 1 }}</span>
              <input v-model="s.name" class="input max-w-xs border-transparent font-medium hover:border-zinc-300" />
              <span class="badge font-mono" :class="s.response.status < 400 ? 'bg-lime-50 text-lime-700' : 'bg-rose-50 text-rose-700'">
                {{ s.response.status }}
              </span>
              <span class="text-xs text-zinc-500">
                {{ s.rules?.length ? `${s.rules.length} regla${s.rules.length === 1 ? '' : 's'}` : 'por defecto' }}
                <template v-if="s.response.delayMs"> · {{ s.response.delayMs }} ms</template>
              </span>
              <code v-if="s.id" class="text-[11px] text-zinc-400">{{ s.id }}</code>
              <div class="ml-auto flex items-center gap-1">
                <ToggleSwitch :model-value="s.enabled !== false" label="Habilitado" @update:model-value="s.enabled = $event" />
                <button class="btn btn-ghost btn-sm" title="Subir" :disabled="i === 0" @click="move(i, -1)"><ArrowUp class="size-3.5" /></button>
                <button class="btn btn-ghost btn-sm" title="Bajar" :disabled="i === ep.scenarios.length - 1" @click="move(i, 1)">
                  <ArrowDown class="size-3.5" />
                </button>
                <button class="btn btn-ghost btn-sm" title="Duplicar" @click="duplicate(i)"><Copy class="size-3.5" /></button>
                <button class="btn btn-ghost btn-sm text-zinc-400 hover:text-rose-600" title="Eliminar" @click="removeScenario(i)">
                  <Trash2 class="size-3.5" />
                </button>
              </div>
            </div>
            <div v-if="open.has(i)" class="grid grid-cols-1 gap-5 border-t border-zinc-100 p-4 xl:grid-cols-2">
              <div class="space-y-3">
                <div>
                  <label class="label">Descripción</label>
                  <input v-model="s.description" class="input" placeholder="Cuándo usar este escenario" />
                </div>
                <div>
                  <label class="label">Condiciones</label>
                  <RuleEditor v-model="s.rules!" v-model:mode="s.ruleMode" />
                </div>
                <IssuesList :issues="scenarioIssues(i)" />
              </div>
              <div>
                <ResponseEditor v-model="s.response" />
              </div>
            </div>
          </div>

          <button class="btn" @click="addScenario"><Plus class="size-4" /> Agregar escenario</button>
        </section>

        <!-- Solicitud esperada -->
        <section v-else-if="tab === 'request'" class="max-w-5xl space-y-5">
          <p class="text-sm text-zinc-600">
            Si la solicitud no cumple estas reglas, la simulación responde con error de validación (422 por defecto).
          </p>
          <div class="card space-y-2 p-4">
            <h2 class="card-title">Headers</h2>
            <ParamTable v-model="ep.request!.headers!" name-placeholder="Authorization" add-label="Agregar header" />
          </div>
          <div class="card space-y-2 p-4">
            <h2 class="card-title">Parámetros de query</h2>
            <ParamTable v-model="ep.request!.query!" name-placeholder="page" add-label="Agregar parámetro" />
          </div>
          <div class="card space-y-2 p-4">
            <h2 class="card-title">Parámetros de ruta</h2>
            <ParamTable v-model="ep.request!.pathParams!" name-placeholder="id" add-label="Agregar parámetro" />
          </div>
          <div class="card space-y-3 p-4">
            <div class="flex items-center justify-between">
              <h2 class="card-title">Body</h2>
              <label class="flex items-center gap-2 text-sm">
                <ToggleSwitch v-model="hasBody" label="Validar body" /> Validar body
              </label>
            </div>
            <template v-if="ep.request?.body">
              <div class="flex gap-4">
                <div>
                  <label class="label">Tipo</label>
                  <select v-model="ep.request.body.kind" class="input w-56">
                    <option v-for="k in kinds" :key="k" :value="k">{{ t(`bodyKind.${k}`) }}</option>
                  </select>
                </div>
                <label class="mt-5 flex items-center gap-2 text-sm">
                  <input v-model="ep.request.body.required" type="checkbox" class="size-4 accent-brand-600" /> Obligatorio
                </label>
              </div>
              <div class="grid grid-cols-2 gap-4">
                <div>
                  <label class="label">JSON Schema (opcional)</label>
                  <CodeEditor v-model="schemaText" language="json" min-height="160px" />
                </div>
                <div>
                  <label class="label">Ejemplo (opcional)</label>
                  <CodeEditor v-model="exampleText" language="json" min-height="160px" />
                  <p class="hint">Se usa para precargar la pestaña Probar.</p>
                </div>
              </div>
              <p v-if="schemaError" class="text-xs text-rose-600">JSON inválido: {{ schemaError }}</p>
            </template>
          </div>
          <div class="card space-y-3 p-4">
            <div class="flex items-center justify-between">
              <h2 class="card-title">Respuesta ante datos inválidos</h2>
              <label class="flex items-center gap-2 text-sm">
                <ToggleSwitch v-model="customValidation" label="Personalizar" /> Personalizar
              </label>
            </div>
            <p v-if="!ep.onValidationError" class="text-xs text-zinc-500">
              Por defecto: HTTP 422 con la lista de errores. Personalízala para imitar el formato del proveedor.
            </p>
            <template v-else>
              <p class="text-xs text-zinc-500">Usa <code v-pre>{{ validation.errors }}</code> para incluir los errores.</p>
              <ResponseEditor v-model="ep.onValidationError" />
            </template>
          </div>
        </section>

        <!-- General -->
        <section v-else-if="tab === 'general'" class="max-w-3xl space-y-4">
          <div class="card space-y-3 p-4">
            <div class="flex gap-2">
              <div>
                <label class="label">Método</label>
                <select v-model="ep.method" class="input-mono w-28">
                  <option v-for="m in HTTP_METHODS" :key="m" :value="m">{{ m === '*' ? 'ANY' : m }}</option>
                </select>
              </div>
              <div class="flex-1">
                <label class="label">Ruta</label>
                <input v-model="ep.path" class="input-mono" />
              </div>
            </div>
            <p class="hint !mt-0">
              Parámetros: <code v-pre>{id}</code>. Comodín final: <code v-pre>{*resto}</code>. Las rutas literales tienen prioridad.
            </p>
            <div>
              <label class="label">Nombre</label>
              <input v-model="ep.name" class="input" />
            </div>
            <div>
              <label class="label">Descripción</label>
              <textarea v-model="ep.description" class="input h-20" />
            </div>
            <div class="flex items-center gap-2">
              <ToggleSwitch v-model="ep.enabled" label="Habilitado" />
              <span class="text-sm">Endpoint habilitado</span>
            </div>
            <div>
              <label class="label">Identificador</label>
              <input :value="ep.id" class="input-mono" disabled />
            </div>
          </div>
          <button class="btn btn-danger" @click="remove"><Trash2 class="size-3.5" /> Eliminar endpoint</button>
        </section>

        <!-- Probar -->
        <section v-else class="max-w-4xl">
          <p v-if="dirty" class="mb-3 rounded-lg bg-amber-50 px-3 py-2 text-sm text-amber-900">
            Guarda los cambios para probarlos: la simulación usa la configuración guardada.
          </p>
          <TryRequest :endpoint="ep" :url="url" />
        </section>
      </div>
    </template>
  </div>
</template>
