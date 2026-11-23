<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { refresh, store, toast, toastError } from '@/app/shared/state/live.ts'
import { confirmAction } from '@/app/shared/state/confirm.ts'
import CopyButton from '@/app/shared/components/CopyButton.vue'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import KeyValueEditor from '@/app/shared/components/KeyValueEditor.vue'
import MethodChip from '@/app/shared/components/MethodChip.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import RunToggle from '@/app/shared/components/RunToggle.vue'
import RunningDot from '@/app/shared/components/RunningDot.vue'
import { HTTP_METHODS, type AuthConfig, type EndpointSummary, type HttpMethod, type Issue, type ServiceView } from '@/infrastructure/api/types.ts'

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

// Recargar si la configuración cambia desde otro lado (agente, otra pestaña).
watch(
  () => [store.configVersion, store.services.find((s) => s.id === props.sid)?.running],
  () => {
    if (!dirty.value) load()
  },
)

// -- endpoints -------------------------------------------------------------

const methodItems = HTTP_METHODS.map((m) => ({ value: m, title: m === '*' ? 'ANY' : m }))
const showNew = ref(false)
const newEp = ref({ method: 'GET' as HttpMethod, path: '/', name: '' })
const newIssues = ref<Issue[]>([])

function openNew() {
  newEp.value = { method: 'GET', path: '/', name: '' }
  newIssues.value = []
  showNew.value = true
}

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
const authItems = (['none', 'bearer', 'apiKey', 'basic'] as AuthType[]).map((value) => ({
  value,
  title: t(`auth.${value}`),
}))
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
  if (!view.value) return
  const ok = await confirmAction({
    title: 'Eliminar servicio',
    text: `Se eliminará “${view.value.service.name}” con todos sus endpoints. Esta acción no se puede deshacer.`,
    confirmText: 'Eliminar',
    danger: true,
  })
  if (!ok) return
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
  <div>
    <EmptyState
      v-if="notFound"
      icon="mdi-help-circle-outline"
      title="Este servicio no existe"
      text="Puede que se haya eliminado o que cambiaras de espacio de trabajo."
    >
      <VBtn to="/servicios">Ver servicios</VBtn>
    </EmptyState>

    <template v-else-if="view">
      <PageHeader :title="view.service.name" :subtitle="view.service.description">
        <template #overline>
          <VBtn variant="text" size="small" prepend-icon="mdi-chevron-left" to="/servicios" class="mb-1 px-1">Servicios</VBtn>
        </template>
        <template #title>
          <span class="d-inline-flex align-center gap-3">
            <RunningDot :running="view.running" />{{ view.service.name }}
          </span>
        </template>
        <template #actions>
          <RunToggle :id="view.service.id" :running="view.running" />
        </template>
      </PageHeader>

      <div class="d-flex flex-wrap gap-2 mb-4">
        <VChip variant="tonal" class="mono">
          {{ view.url }}
          <template #append><CopyButton :text="view.url" class="ms-1" /></template>
        </VChip>
        <VChip v-if="view.portUrl" variant="tonal" class="mono">
          {{ view.portUrl }}
          <template #append><CopyButton :text="view.portUrl" class="ms-1" /></template>
        </VChip>
      </div>

      <VTabs v-model="tab" class="mb-4">
        <VTab value="endpoints">Endpoints ({{ endpoints.length }})</VTab>
        <VTab value="config">
          Configuración
          <VBadge v-if="dirty" dot color="primary" inline />
        </VTab>
      </VTabs>

      <VWindow v-model="tab">
        <VWindowItem value="endpoints">
          <VCard>
            <VCardItem>
              <VCardSubtitle>Las rutas son relativas a <code class="inline">{{ view.service.prefix }}</code></VCardSubtitle>
              <template #append>
                <VBtn color="primary" size="small" prepend-icon="mdi-plus" @click="openNew">Nuevo endpoint</VBtn>
              </template>
            </VCardItem>
            <EmptyState
              v-if="!endpoints.length"
              icon="mdi-routes"
              title="Sin endpoints"
              text="Agrega las rutas que tu aplicación llama de este proveedor."
            >
              <VBtn color="primary" prepend-icon="mdi-plus" @click="openNew">Agregar endpoint</VBtn>
            </EmptyState>
            <VTable v-else hover class="clickable-rows">
              <tbody>
                <tr
                  v-for="ep in endpoints"
                  :key="ep.id"
                  :style="{ opacity: ep.enabled ? 1 : 0.5 }"
                  @click="router.push(`/servicios/${sid}/endpoints/${ep.id}`)"
                >
                  <td style="width: 90px"><MethodChip :method="ep.method" /></td>
                  <td>
                    <code>{{ ep.path }}</code>
                    <div v-if="ep.name" class="text-body-small text-medium-emphasis">{{ ep.name }}</div>
                  </td>
                  <td class="text-body-small text-medium-emphasis">
                    {{ ep.scenarioCount }} escenario{{ ep.scenarioCount === 1 ? '' : 's' }}
                    <VChip v-if="ep.activeScenario" size="x-small" color="warning" variant="tonal" class="ms-1">
                      forzado: {{ ep.activeScenario }}
                    </VChip>
                  </td>
                  <td style="width: 40px" @click.stop><CopyButton :text="ep.url" /></td>
                  <td style="width: 70px" @click.stop>
                    <VSwitch :model-value="ep.enabled" hide-details aria-label="Habilitado" @update:model-value="toggleEndpoint(ep)" />
                  </td>
                </tr>
              </tbody>
            </VTable>
          </VCard>
        </VWindowItem>

        <VWindowItem value="config">
          <div class="d-flex flex-column gap-4" style="max-width: 860px">
            <VCard title="General">
              <VCardText class="d-flex flex-column gap-3">
                <VRow dense>
                  <VCol cols="12" md="6"><FTextField v-model="form.name" label="Nombre" /></VCol>
                  <VCol cols="12" md="6"><FTextField :model-value="view.service.id" label="Identificador" class="mono" disabled /></VCol>
                </VRow>
                <FTextField v-model="form.description" label="Descripción" />
                <VRow dense align="center">
                  <VCol cols="12" md="4"><FTextField v-model="form.prefix" label="Prefijo" class="mono" /></VCol>
                  <VCol cols="12" md="4"><FTextField v-model="form.port" label="Puerto propio" type="number" class="mono" /></VCol>
                  <VCol cols="12" md="4"><VSwitch v-model="form.autoStart" label="Iniciar al abrir la app" hide-details /></VCol>
                </VRow>
                <FTextField v-model="form.upstreamUrl" label="URL del proveedor real" placeholder="https://…" class="mono" />
              </VCardText>
            </VCard>

            <VCard title="Autenticación que exige la simulación" subtitle="Si la solicitud no cumple, responde 401 antes de evaluar escenarios.">
              <VCardText class="d-flex flex-column gap-3">
                <FSelect v-model="form.authType" :items="authItems" label="Tipo" style="max-width: 320px" />
                <FTextField
                  v-if="form.authType === 'bearer'"
                  v-model="form.authToken"
                  label="Token esperado"
                  placeholder="vacío = aceptar cualquier token"
                  class="mono"
                />
                <VRow v-else-if="form.authType === 'apiKey'" dense>
                  <VCol cols="6"><FTextField v-model="form.authHeader" label="Header" class="mono" /></VCol>
                  <VCol cols="6"><FTextField v-model="form.authValue" label="Valor esperado" placeholder="vacío = cualquier valor" class="mono" /></VCol>
                </VRow>
                <VRow v-else-if="form.authType === 'basic'" dense>
                  <VCol cols="6"><FTextField v-model="form.authUser" label="Usuario" placeholder="vacío = cualquiera" class="mono" /></VCol>
                  <VCol cols="6"><FTextField v-model="form.authPass" label="Contraseña" placeholder="vacío = cualquiera" class="mono" /></VCol>
                </VRow>
              </VCardText>
            </VCard>

            <VCard title="Variables del servicio">
              <VCardText>
                <div class="text-body-small text-medium-emphasis mb-3">
                  Disponibles en plantillas como <code class="inline" v-pre>{{ vars.NOMBRE }}</code>. Tienen prioridad sobre las del espacio.
                </div>
                <KeyValueEditor v-model="form.variables" key-placeholder="API_VERSION" value-placeholder="v1" add-label="Agregar variable" />
              </VCardText>
            </VCard>

            <IssuesList :issues="issues" />
            <div class="d-flex align-center">
              <VBtn color="error" variant="tonal" prepend-icon="mdi-delete-outline" @click="remove">Eliminar servicio</VBtn>
              <VSpacer />
              <VBtn variant="text" :disabled="!dirty" class="me-2" @click="resetForm">Descartar</VBtn>
              <VBtn color="primary" :disabled="!dirty" @click="save">Guardar cambios</VBtn>
            </div>
          </div>
        </VWindowItem>
      </VWindow>

      <VDialog v-model="showNew" max-width="560">
        <VCard title="Nuevo endpoint">
          <VCardText>
            <VForm class="d-flex flex-column gap-3" @submit.prevent="createEndpoint">
              <div class="d-flex gap-2">
                <FSelect v-model="newEp.method" :items="methodItems" label="Método" class="mono" style="max-width: 140px" />
                <FTextField v-model="newEp.path" label="Ruta" placeholder="/documents/{id}/sign" class="mono" autofocus />
              </div>
              <div class="text-body-small text-medium-emphasis">
                Usa <code class="inline" v-pre>{id}</code> para parámetros y <code class="inline" v-pre>{*resto}</code> al final para capturar el resto de la ruta.
              </div>
              <FTextField v-model="newEp.name" label="Nombre (opcional)" placeholder="Solicitar firma" />
              <IssuesList :issues="newIssues" />
            </VForm>
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn variant="text" @click="showNew = false">Cancelar</VBtn>
            <VBtn color="primary" @click="createEndpoint">Crear y editar</VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </div>
</template>
