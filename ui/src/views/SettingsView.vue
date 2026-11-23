<script setup lang="ts">
import { computed, ref } from 'vue'
import { Bot } from 'lucide-vue-next'
import { api, ApiError } from '../lib/api'
import { refresh, store, toast, toastError } from '../lib/store'
import IssuesList from '../components/IssuesList.vue'
import KeyValueEditor from '../components/KeyValueEditor.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
import type { Issue, Workspace } from '../lib/types'

const ws = ref<Workspace | null>(null)
const original = ref('')
const issues = ref<Issue[]>([])
const redactText = ref('')

async function load() {
  try {
    const v = await api.activeWorkspace()
    v.workspace.variables ??= {}
    ws.value = v.workspace
    redactText.value = v.workspace.history.redactHeaders.join(', ')
    original.value = JSON.stringify(ws.value)
  } catch (e) {
    toastError(e)
  }
}
load()

const dirty = computed(() => {
  if (!ws.value) return false
  const redact = redactText.value.split(',').map((s) => s.trim()).filter(Boolean)
  return JSON.stringify({ ...ws.value, history: { ...ws.value.history, redactHeaders: redact } }) !== original.value
})

async function save() {
  if (!ws.value) return
  issues.value = []
  ws.value.history.redactHeaders = redactText.value.split(',').map((s) => s.trim()).filter(Boolean)
  try {
    ws.value = await api.updateWorkspace(ws.value)
    ws.value.variables ??= {}
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
  set: (v: number) => {
    if (ws.value) ws.value.server.maxBodyBytes = Math.max(1, Math.round(v * 1024 * 1024))
  },
})
</script>

<template>
  <div class="max-w-3xl space-y-5 p-8">
    <header>
      <h1 class="page-title">Ajustes</h1>
      <p class="page-subtitle">Configuración del espacio de trabajo activo ({{ store.status?.workspaceName }}).</p>
    </header>

    <template v-if="ws">
      <div class="card space-y-3 p-4">
        <h2 class="card-title">Espacio de trabajo</h2>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">Nombre</label>
            <input v-model="ws.name" class="input" />
          </div>
          <div>
            <label class="label">Carpeta</label>
            <input :value="store.status?.workspacePath" class="input-mono" disabled />
          </div>
        </div>
        <div>
          <label class="label">Descripción</label>
          <input v-model="ws.description" class="input" />
        </div>
      </div>

      <div class="card space-y-3 p-4">
        <h2 class="card-title">Servidor de simulación</h2>
        <div class="grid grid-cols-3 gap-3">
          <div>
            <label class="label">Dirección</label>
            <select v-model="ws.server.host" class="input-mono">
              <option value="127.0.0.1">127.0.0.1 (solo este equipo)</option>
              <option value="0.0.0.0">0.0.0.0 (toda la red)</option>
            </select>
          </div>
          <div>
            <label class="label">Puerto central</label>
            <input v-model.number="ws.server.port" type="number" class="input-mono" />
          </div>
          <div>
            <label class="label">Body máximo (MB)</label>
            <input v-model.number="maxBodyMb" type="number" step="0.5" min="0.5" class="input-mono" />
          </div>
        </div>
        <p v-if="ws.server.host === '0.0.0.0'" class="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-900">
          Otros equipos de tu red podrán llamar a las simulaciones. Úsalo solo en redes de confianza.
        </p>
        <label class="flex items-center gap-2 text-sm">
          <ToggleSwitch v-model="ws.server.cors" label="CORS" />
          Responder CORS automáticamente (para frontends en el navegador)
        </label>
      </div>

      <div class="card space-y-3 p-4">
        <h2 class="card-title">Variables globales</h2>
        <p class="text-xs text-zinc-500">Disponibles en todas las plantillas como <code v-pre>{{ vars.NOMBRE }}</code>.</p>
        <KeyValueEditor v-model="ws!.variables" key-placeholder="CALLBACK_BASE" value-placeholder="http://localhost:3000" add-label="Agregar variable" />
      </div>

      <div class="card space-y-3 p-4">
        <h2 class="card-title">Historial</h2>
        <label class="flex items-center gap-2 text-sm">
          <ToggleSwitch v-model="ws!.history.enabled" label="Guardar historial" /> Guardar historial de solicitudes
        </label>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">Solicitudes a conservar</label>
            <input v-model.number="ws!.history.maxEntries" type="number" min="10" class="input-mono" />
          </div>
          <div>
            <label class="label">Bytes de body guardados por solicitud</label>
            <input v-model.number="ws!.history.maxBodyCaptureBytes" type="number" min="0" class="input-mono" />
          </div>
        </div>
        <div>
          <label class="label">Headers con datos sensibles (se ocultan)</label>
          <input v-model="redactText" class="input-mono" />
          <p class="hint">Separados por comas. El valor se guarda parcialmente oculto: <code>Bearer abcd••••</code>.</p>
        </div>
      </div>

      <IssuesList :issues="issues" />
      <div class="flex justify-end gap-2">
        <button class="btn" :disabled="!dirty" @click="load">Descartar</button>
        <button class="btn btn-primary" :disabled="!dirty" @click="save">Guardar ajustes</button>
      </div>

      <div class="card flex items-start gap-3 border-dashed p-4">
        <Bot class="mt-0.5 size-5 text-brand-600" />
        <div class="text-sm">
          <div class="font-medium">Agentes de IA y API REST</div>
          <p class="mt-0.5 text-zinc-600">
            La API de administración está en <code>/api/v1</code> de esta misma dirección. Obtén el token con
            <code>fakes token</code>. El servidor MCP para agentes llega en la fase 3 y usará las mismas operaciones.
          </p>
        </div>
      </div>
    </template>
  </div>
</template>
