<script setup lang="ts">
import { computed, ref } from 'vue'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { refresh, store, toast, toastError } from '@/app/shared/state/live.ts'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import KeyValueEditor from '@/app/shared/components/KeyValueEditor.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import type { AutostartStatus, Issue, Workspace } from '@/infrastructure/api/types.ts'

const ws = ref<Workspace | null>(null)
const original = ref('')
const issues = ref<Issue[]>([])
const redactText = ref('')
const allowedHostsText = ref('')

const hostItems = [
  { title: '127.0.0.1 (solo este equipo)', value: '127.0.0.1' },
  { title: '0.0.0.0 (toda la red)', value: '0.0.0.0' },
]

const autostart = ref<AutostartStatus | null>(null)
const autostartBusy = ref(false)
api.autostart().then((a) => (autostart.value = a)).catch(() => {})

async function toggleAutostart(enabled: boolean | null) {
  autostartBusy.value = true
  try {
    autostart.value = await api.setAutostart(!!enabled)
    toast(enabled ? 'Fake Services iniciará con el sistema' : 'Inicio automático desactivado')
  } catch (e) {
    toastError(e)
  } finally {
    autostartBusy.value = false
  }
}

async function load() {
  try {
    const v = await api.activeWorkspace()
    v.workspace.variables ??= {}
    v.workspace.description ??= ''
    v.workspace.webhooks.allowedHosts ??= []
    ws.value = v.workspace
    redactText.value = v.workspace.history.redactHeaders.join(', ')
    allowedHostsText.value = (v.workspace.webhooks.allowedHosts ?? []).join(', ')
    original.value = JSON.stringify(ws.value)
  } catch (e) {
    toastError(e)
  }
}
load()

const redactList = () => redactText.value.split(',').map((s) => s.trim()).filter(Boolean)
const hostsList = () => allowedHostsText.value.split(',').map((s) => s.trim()).filter(Boolean)

const dirty = computed(() => {
  if (!ws.value) return false
  return (
    JSON.stringify({
      ...ws.value,
      history: { ...ws.value.history, redactHeaders: redactList() },
      webhooks: { ...ws.value.webhooks, allowedHosts: hostsList() },
    }) !== original.value
  )
})

async function save() {
  if (!ws.value) return
  issues.value = []
  ws.value.history.redactHeaders = redactList()
  ws.value.webhooks.allowedHosts = hostsList()
  try {
    const saved = await api.updateWorkspace(ws.value)
    saved.variables ??= {}
    saved.description ??= ''
    saved.webhooks.allowedHosts ??= []
    ws.value = saved
    original.value = JSON.stringify(ws.value)
    toast('Ajustes guardados')
    refresh()
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  }
}

const maxBodyMb = computed({
  get: () => (ws.value ? Math.round((ws.value.server.maxBodyBytes / 1024 / 1024) * 10) / 10 : 0),
  set: (v: number | string) => {
    if (ws.value) ws.value.server.maxBodyBytes = Math.max(1, Math.round(Number(v) * 1024 * 1024))
  },
})
</script>

<template>
  <div style="max-width: 880px">
    <PageHeader title="Ajustes" :subtitle="`Configuración del espacio de trabajo activo (${store.status?.workspaceName ?? ''}).`" />

    <div v-if="ws" class="d-flex flex-column gap-4">
      <VCard title="Iniciar con el sistema" subtitle="Fake Services arranca en segundo plano al iniciar sesión; abre la interfaz con «fakes» o desde el menú.">
        <VCardText>
          <VSwitch
            :model-value="autostart?.enabled ?? false"
            :disabled="!autostart?.supported"
            :loading="autostartBusy"
            label="Iniciar Fake Services al iniciar sesión"
            color="primary"
            hide-details
            @update:model-value="toggleAutostart"
          />
          <div v-if="autostart" class="text-body-small text-medium-emphasis mt-2">
            {{ autostart.method }}<template v-if="autostart.command"><br /><code class="inline">{{ autostart.command }}</code></template>
          </div>
        </VCardText>
      </VCard>

      <VCard title="Espacio de trabajo">
        <VCardText class="d-flex flex-column gap-3">
          <VRow dense>
            <VCol cols="12" md="6"><FTextField v-model="ws.name" label="Nombre" /></VCol>
            <VCol cols="12" md="6"><FTextField :model-value="store.status?.workspacePath" label="Carpeta" class="mono" disabled /></VCol>
          </VRow>
          <FTextField v-model="ws.description" label="Descripción" />
        </VCardText>
      </VCard>

      <VCard title="Servidor de simulación">
        <VCardText class="d-flex flex-column gap-3">
          <VRow dense>
            <VCol cols="12" md="5"><FSelect v-model="ws.server.host" :items="hostItems" label="Dirección" /></VCol>
            <VCol cols="6" md="3"><FTextField v-model.number="ws.server.port" label="Puerto central" type="number" class="mono" /></VCol>
            <VCol cols="6" md="4"><FTextField v-model="maxBodyMb" label="Body máximo (MB)" type="number" step="0.5" min="0.5" /></VCol>
          </VRow>
          <VAlert v-if="ws.server.host === '0.0.0.0'" type="warning" variant="tonal" density="compact">
            Otros equipos de tu red podrán llamar a las simulaciones. Úsalo solo en redes de confianza.
          </VAlert>
          <VSwitch v-model="ws.server.cors" label="Responder CORS automáticamente (para frontends en el navegador)" color="primary" hide-details />
        </VCardText>
      </VCard>

      <VCard title="Webhooks de los workflows" subtitle="Protección SSRF: a dónde pueden llamar las solicitudes HTTP de los workflows.">
        <VCardText class="d-flex flex-column gap-3">
          <VSwitch v-model="ws!.webhooks.allowLoopback" label="Permitir localhost (127.0.0.1, ::1)" color="primary" hide-details />
          <VSwitch v-model="ws!.webhooks.allowPrivateNetworks" label="Permitir redes privadas (10.x, 172.16-31.x, 192.168.x)" color="primary" hide-details />
          <FTextField
            v-model="allowedHostsText"
            label="Hosts permitidos siempre"
            placeholder="host.docker.internal, 192.168.1.20"
            class="mono"
            hint="Separados por comas. Las direcciones de metadatos de nube (169.254.169.254) nunca se permiten."
            persistent-hint
          />
        </VCardText>
      </VCard>

      <VCard title="Variables globales">
        <VCardText>
          <div class="text-body-small text-medium-emphasis mb-3">
            Disponibles en todas las plantillas como <code class="inline" v-pre>{{ vars.NOMBRE }}</code>.
          </div>
          <KeyValueEditor v-model="ws!.variables" key-placeholder="CALLBACK_BASE" value-placeholder="http://localhost:3000" add-label="Agregar variable" />
        </VCardText>
      </VCard>

      <VCard title="Historial">
        <VCardText class="d-flex flex-column gap-3">
          <VSwitch v-model="ws!.history.enabled" label="Guardar historial de solicitudes" color="primary" hide-details />
          <VRow dense>
            <VCol cols="12" md="6"><FTextField v-model.number="ws!.history.maxEntries" label="Solicitudes a conservar" type="number" min="10" /></VCol>
            <VCol cols="12" md="6">
              <FTextField v-model.number="ws!.history.maxBodyCaptureBytes" label="Bytes de body guardados por solicitud" type="number" min="0" />
            </VCol>
          </VRow>
          <FTextField
            v-model="redactText"
            label="Headers con datos sensibles (se ocultan)"
            class="mono"
            hint="Separados por comas. El valor se guarda parcialmente oculto: Bearer abcd••••"
            persistent-hint
          />
        </VCardText>
      </VCard>

      <IssuesList :issues="issues" />
      <div class="d-flex justify-end gap-2">
        <VBtn variant="text" :disabled="!dirty" @click="load">Descartar</VBtn>
        <VBtn color="primary" :disabled="!dirty" @click="save">Guardar ajustes</VBtn>
      </div>

      <VAlert type="info" variant="tonal" icon="mdi-robot-outline" title="Agentes de IA y API REST">
        La API de administración está en <code class="inline">/api/v1</code> de esta misma dirección. Obtén el token con
        <code class="inline">fakes token</code>. El servidor MCP para agentes llega en la fase 3 y usará las mismas operaciones.
      </VAlert>
    </div>
  </div>
</template>
