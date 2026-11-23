<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, Plus, Trash2 } from 'lucide-vue-next'
import { api, ApiError } from '../lib/api'
import { refresh, store, toast, toastError } from '../lib/store'
import CopyButton from '../components/CopyButton.vue'
import EmptyState from '../components/EmptyState.vue'
import IssuesList from '../components/IssuesList.vue'
import KeyValueEditor from '../components/KeyValueEditor.vue'
import MethodBadge from '../components/MethodBadge.vue'
import ModalDialog from '../components/ModalDialog.vue'
import RunToggle from '../components/RunToggle.vue'
import RunningDot from '../components/RunningDot.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
import { HTTP_METHODS, type AuthConfig, type EndpointSummary, type HttpMethod, type Issue, type ServiceView } from '../lib/types'

const props = defineProps<{ sid: string }>()
const router = useRouter()
const { t } = useI18n()

const view = ref<ServiceView | null>(null)
const endpoints = ref<EndpointSummary[]>([])
const tab = ref<'endpoints' | 'config'>('endpoints')
const notFound = ref(false)

async function load() {
  try {
    const [v, eps] = await Promise.all([api.service(props.sid), api.endpoints(props.sid)])
    view.value = v
    endpoints.value = eps
    resetForm()
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) notFound.value = true
    else toastError(e)
  }
}
load()

// Recargar cuando cambie la configuración o el estado desde otro lado (agente, otra pestaña).
watch(
  () => [store.configVersion, store.services.find((s) => s.id === props.sid)?.running],
  () => {
    if (!dirty.value) load()
  },
)

// -- endpoints -------------------------------------------------------------

const showNew = ref(false)
const newEp = ref({ method: 'GET' as HttpMethod, path: '/', name: '' })
const newIssues = ref<Issue[]>([])

async function createEndpoint() {
  newIssues.value = []
  try {
    const v = await api.createEndpoint(props.sid, {
      method: newEp.value.method,
      path: newEp.value.path,
      name: newEp.value.name,
      enabled: true,
      scenarios: [
        {
          id: 'ok',
          name: 'Respuesta exitosa',
          response: {
            status: newEp.value.method === 'POST' ? 201 : 200,
            body: { type: 'json', content: { ok: true } },
          },
        },
      ],
    })
    toast('Endpoint creado')
    router.push(`/servicios/${props.sid}/endpoints/${v.endpoint.id}`)
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) newIssues.value = e.issues
    else toastError(e)
  }
}

async function toggleEndpoint(ep: EndpointSummary) {
  try {
    await api.setEndpointEnabled(props.sid, ep.id, !ep.enabled)
    ep.enabled = !ep.enabled
  } catch (e) {
    toastError(e)
  }
}

// -- configuración ---------------------------------------------------------

type AuthType = AuthConfig['type']
const form = ref({
  name: '',
  description: '',
  prefix: '',
  port: '' as string | number,
  upstreamUrl: '',
  autoStart: true,
  variables: {} as Record<string, string>,
  authType: 'none' as AuthType,
  authToken: '',
  authHeader: 'X-Api-Key',
  authValue: '',
  authUser: '',
  authPass: '',
})
const issues = ref<Issue[]>([])
const original = ref('')
const dirty = computed(() => original.value !== '' && JSON.stringify(form.value) !== original.value)

function resetForm() {
  const s = view.value?.service
  if (!s) return
  const a = s.auth ?? { type: 'none' }
  form.value = {
    name: s.name,
    description: s.description ?? '',
    prefix: s.prefix,
    port: s.port ?? '',
    upstreamUrl: s.upstreamUrl ?? '',
    autoStart: s.autoStart ?? true,
    variables: { ...(s.variables ?? {}) },
    authType: a.type,
    authToken: a.type === 'bearer' ? (a.token ?? '') : '',
    authHeader: a.type === 'apiKey' ? a.header : 'X-Api-Key',
    authValue: a.type === 'apiKey' ? (a.value ?? '') : '',
    authUser: a.type === 'basic' ? (a.username ?? '') : '',
    authPass: a.type === 'basic' ? (a.password ?? '') : '',
  }
  original.value = JSON.stringify(form.value)
  issues.value = []
}

function buildAuth(): AuthConfig {
  const f = form.value
  switch (f.authType) {
    case 'bearer':
      return { type: 'bearer', token: f.authToken || null }
    case 'apiKey':
      return { type: 'apiKey', header: f.authHeader, value: f.authValue || null }
    case 'basic':
      return { type: 'basic', username: f.authUser || null, password: f.authPass || null }
    default:
      return { type: 'none' }
  }
}

async function save() {
  if (!view.value) return
  issues.value = []
  const f = form.value
  try {
    view.value = await api.updateService(props.sid, {
      ...view.value.service,
      endpoints: [],
      name: f.name,
      description: f.description,
      prefix: f.prefix,
      port: f.port === '' ? null : Number(f.port),
      upstreamUrl: f.upstreamUrl || null,
      autoStart: f.autoStart,
      variables: f.variables,
      auth: buildAuth(),
    })
    resetForm()
    toast('Servicio guardado')
    refresh()
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  }
}

async function remove() {
  if (!view.value || !confirm(t('common.confirmDelete', { name: view.value.service.name }))) return
  try {
    await api.deleteService(props.sid)
    toast('Servicio eliminado')
    await refresh()
    router.push('/servicios')
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div class="space-y-5 p-8">
    <EmptyState v-if="notFound" title="Este servicio no existe" text="Puede que se haya eliminado o que cambiaras de espacio de trabajo.">
      <RouterLink to="/servicios" class="btn">Ver servicios</RouterLink>
    </EmptyState>

    <template v-else-if="view">
      <RouterLink to="/servicios" class="inline-flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-800">
        <ChevronLeft class="size-3.5" /> Servicios
      </RouterLink>
      <header class="flex items-start justify-between gap-4">
        <div class="min-w-0">
          <div class="flex items-center gap-2.5">
            <RunningDot :running="view.running" />
            <h1 class="page-title">{{ view.service.name }}</h1>
          </div>
          <p v-if="view.service.description" class="page-subtitle">{{ view.service.description }}</p>
          <div class="mt-2 flex flex-wrap items-center gap-3 text-sm">
            <span class="flex items-center gap-1.5 rounded-lg bg-zinc-100 px-2 py-1">
              <code class="text-xs">{{ view.url }}</code>
              <CopyButton :text="view.url" class="text-zinc-500 hover:text-zinc-900" />
            </span>
            <span v-if="view.portUrl" class="flex items-center gap-1.5 rounded-lg bg-zinc-100 px-2 py-1">
              <code class="text-xs">{{ view.portUrl }}</code>
              <CopyButton :text="view.portUrl" class="text-zinc-500 hover:text-zinc-900" />
            </span>
          </div>
        </div>
        <RunToggle :id="view.service.id" :running="view.running" />
      </header>

      <nav class="flex border-b border-zinc-200">
        <button class="tab" :class="{ 'tab-active': tab === 'endpoints' }" @click="tab = 'endpoints'">
          Endpoints <span class="text-zinc-400">({{ endpoints.length }})</span>
        </button>
        <button class="tab" :class="{ 'tab-active': tab === 'config' }" @click="tab = 'config'">
          Configuración <span v-if="dirty" class="text-brand-600">•</span>
        </button>
      </nav>

      <!-- Endpoints -->
      <section v-if="tab === 'endpoints'" class="card">
        <div class="card-header">
          <span class="text-xs text-zinc-500">Las rutas son relativas a <code>{{ view.service.prefix }}</code></span>
          <button class="btn btn-primary btn-sm" @click="showNew = true; newIssues = []"><Plus class="size-3.5" /> Nuevo endpoint</button>
        </div>
        <EmptyState v-if="!endpoints.length" title="Sin endpoints" text="Agrega las rutas que tu aplicación llama de este proveedor.">
          <button class="btn btn-primary" @click="showNew = true"><Plus class="size-4" /> Agregar endpoint</button>
        </EmptyState>
        <table v-else class="table">
          <tbody>
            <tr
              v-for="ep in endpoints"
              :key="ep.id"
              class="cursor-pointer hover:bg-zinc-50"
              :class="{ 'opacity-50': !ep.enabled }"
              @click="router.push(`/servicios/${sid}/endpoints/${ep.id}`)"
            >
              <td class="w-20"><MethodBadge :method="ep.method" /></td>
              <td>
                <code class="text-[13px]">{{ ep.path }}</code>
                <div v-if="ep.name" class="text-xs text-zinc-500">{{ ep.name }}</div>
              </td>
              <td class="text-xs text-zinc-500">
                {{ ep.scenarioCount }} escenario{{ ep.scenarioCount === 1 ? '' : 's' }}
                <span v-if="ep.activeScenario" class="badge ml-1 bg-amber-50 text-amber-700">forzado: {{ ep.activeScenario }}</span>
              </td>
              <td class="w-10" @click.stop>
                <CopyButton :text="ep.url" class="text-zinc-400 hover:text-zinc-800" />
              </td>
              <td class="w-14" @click.stop>
                <ToggleSwitch :model-value="ep.enabled" label="Habilitado" @update:model-value="toggleEndpoint(ep)" />
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <!-- Configuración -->
      <section v-else class="max-w-3xl space-y-5">
        <div class="card space-y-3 p-4">
          <h2 class="card-title">General</h2>
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="label">Nombre</label>
              <input v-model="form.name" class="input" />
            </div>
            <div>
              <label class="label">Identificador</label>
              <input :value="view.service.id" class="input-mono" disabled />
            </div>
          </div>
          <div>
            <label class="label">Descripción</label>
            <input v-model="form.description" class="input" />
          </div>
          <div class="grid grid-cols-3 gap-3">
            <div>
              <label class="label">Prefijo</label>
              <input v-model="form.prefix" class="input-mono" />
            </div>
            <div>
              <label class="label">Puerto propio</label>
              <input v-model="form.port" type="number" class="input-mono" placeholder="—" />
            </div>
            <div class="flex items-end gap-2 pb-1.5">
              <ToggleSwitch v-model="form.autoStart" label="Iniciar al abrir" />
              <span class="text-sm">Iniciar al abrir la app</span>
            </div>
          </div>
          <div>
            <label class="label">URL del proveedor real</label>
            <input v-model="form.upstreamUrl" class="input-mono" placeholder="https://…" />
          </div>
        </div>

        <div class="card space-y-3 p-4">
          <h2 class="card-title">Autenticación que exige la simulación</h2>
          <select v-model="form.authType" class="input max-w-xs">
            <option v-for="a in ['none', 'bearer', 'apiKey', 'basic'] as const" :key="a" :value="a">{{ t(`auth.${a}`) }}</option>
          </select>
          <div v-if="form.authType === 'bearer'">
            <label class="label">Token esperado</label>
            <input v-model="form.authToken" class="input-mono" placeholder="vacío = aceptar cualquier token" />
          </div>
          <div v-else-if="form.authType === 'apiKey'" class="grid grid-cols-2 gap-3">
            <div>
              <label class="label">Header</label>
              <input v-model="form.authHeader" class="input-mono" />
            </div>
            <div>
              <label class="label">Valor esperado</label>
              <input v-model="form.authValue" class="input-mono" placeholder="vacío = cualquier valor" />
            </div>
          </div>
          <div v-else-if="form.authType === 'basic'" class="grid grid-cols-2 gap-3">
            <div>
              <label class="label">Usuario</label>
              <input v-model="form.authUser" class="input-mono" placeholder="vacío = cualquiera" />
            </div>
            <div>
              <label class="label">Contraseña</label>
              <input v-model="form.authPass" class="input-mono" placeholder="vacío = cualquiera" />
            </div>
          </div>
          <p class="hint">Si la solicitud no cumple, la simulación responde 401 antes de evaluar escenarios.</p>
        </div>

        <div class="card space-y-3 p-4">
          <h2 class="card-title">Variables del servicio</h2>
          <p class="text-xs text-zinc-500">
            Disponibles en plantillas como <code v-pre>{{ vars.NOMBRE }}</code>. Tienen prioridad sobre las del espacio.
          </p>
          <KeyValueEditor v-model="form.variables" key-placeholder="API_VERSION" value-placeholder="v1" add-label="Agregar variable" />
        </div>

        <IssuesList :issues="issues" />
        <div class="flex items-center justify-between">
          <button class="btn btn-danger" @click="remove"><Trash2 class="size-3.5" /> Eliminar servicio</button>
          <div class="flex gap-2">
            <button class="btn" :disabled="!dirty" @click="resetForm">Descartar</button>
            <button class="btn btn-primary" :disabled="!dirty" @click="save">Guardar cambios</button>
          </div>
        </div>
      </section>

      <ModalDialog v-if="showNew" title="Nuevo endpoint" @close="showNew = false">
        <form class="space-y-3" @submit.prevent="createEndpoint">
          <div class="flex gap-2">
            <select v-model="newEp.method" class="input-mono w-28">
              <option v-for="m in HTTP_METHODS" :key="m" :value="m">{{ m === '*' ? 'ANY' : m }}</option>
            </select>
            <input v-model="newEp.path" class="input-mono flex-1" placeholder="/documents/{id}/sign" autofocus />
          </div>
          <p class="hint">Usa <code v-pre>{id}</code> para parámetros y <code v-pre>{*resto}</code> al final para capturar el resto de la ruta.</p>
          <div>
            <label class="label">Nombre (opcional)</label>
            <input v-model="newEp.name" class="input" placeholder="Solicitar firma" />
          </div>
          <IssuesList :issues="newIssues" />
        </form>
        <template #footer>
          <button class="btn" @click="showNew = false">Cancelar</button>
          <button class="btn btn-primary" @click="createEndpoint">Crear y editar</button>
        </template>
      </ModalDialog>
    </template>
  </div>
</template>
