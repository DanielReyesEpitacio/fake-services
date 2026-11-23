<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Play, Plus, Square } from 'lucide-vue-next'
import { api, ApiError } from '../lib/api'
import { refresh, store, toast, toastError } from '../lib/store'
import CopyButton from '../components/CopyButton.vue'
import EmptyState from '../components/EmptyState.vue'
import IssuesList from '../components/IssuesList.vue'
import ModalDialog from '../components/ModalDialog.vue'
import RunToggle from '../components/RunToggle.vue'
import RunningDot from '../components/RunningDot.vue'
import type { Issue } from '../lib/types'

const route = useRoute()
const router = useRouter()

const showCreate = ref(route.query.nuevo === '1')
const form = ref({ name: '', description: '', prefix: '', port: '', upstreamUrl: '' })
const issues = ref<Issue[]>([])
const saving = ref(false)
const filter = ref('')

const suggestedPrefix = computed(() => {
  const slug = form.value.name
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return slug ? `/${slug}` : '/mi-servicio'
})

const filtered = computed(() => {
  const q = filter.value.toLowerCase()
  return store.services.filter(
    (s) => !q || s.name.toLowerCase().includes(q) || s.prefix.includes(q) || s.description.toLowerCase().includes(q),
  )
})

function openCreate() {
  form.value = { name: '', description: '', prefix: '', port: '', upstreamUrl: '' }
  issues.value = []
  showCreate.value = true
}

function closeCreate() {
  showCreate.value = false
  if (route.query.nuevo) router.replace({ query: {} })
}

async function create() {
  saving.value = true
  issues.value = []
  try {
    const v = await api.createService({
      name: form.value.name,
      description: form.value.description || undefined,
      prefix: form.value.prefix,
      port: form.value.port ? Number(form.value.port) : null,
      upstreamUrl: form.value.upstreamUrl || null,
    })
    toast(`Servicio “${v.service.name}” creado`)
    await refresh()
    router.push(`/servicios/${v.service.id}`)
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  } finally {
    saving.value = false
  }
}

async function all(running: boolean) {
  try {
    await (running ? api.startAll() : api.stopAll())
    await refresh()
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div class="space-y-6 p-8">
    <header class="flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">Servicios</h1>
        <p class="page-subtitle">Cada servicio simula un proveedor externo con su propio prefijo de rutas.</p>
      </div>
      <div class="flex gap-2">
        <button class="btn" @click="all(true)"><Play class="size-3.5" /> Iniciar todos</button>
        <button class="btn" @click="all(false)"><Square class="size-3.5" /> Detener todos</button>
        <button class="btn btn-primary" @click="openCreate"><Plus class="size-4" /> Nuevo servicio</button>
      </div>
    </header>

    <input v-if="store.services.length > 4" v-model="filter" class="input max-w-sm" placeholder="Buscar servicio…" />

    <div class="card">
      <EmptyState v-if="!store.services.length" title="Aún no hay servicios" text="Un servicio representa un proveedor como MiFiel, Belvo o AWS S3.">
        <button class="btn btn-primary" @click="openCreate"><Plus class="size-4" /> Crear el primero</button>
      </EmptyState>
      <table v-else class="table">
        <thead>
          <tr>
            <th class="w-8"></th>
            <th>Servicio</th>
            <th>URL local</th>
            <th class="w-24 text-center">Endpoints</th>
            <th class="w-28"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in filtered" :key="s.id" class="cursor-pointer hover:bg-zinc-50" @click="router.push(`/servicios/${s.id}`)">
            <td><RunningDot :running="s.running" /></td>
            <td>
              <div class="font-medium">{{ s.name }}</div>
              <div class="max-w-md truncate text-xs text-zinc-500">{{ s.description }}</div>
            </td>
            <td>
              <div class="flex items-center gap-1.5">
                <code class="text-xs">{{ s.url }}</code>
                <CopyButton :text="s.url" class="text-zinc-400 hover:text-zinc-700" />
              </div>
              <div v-if="s.port" class="text-[11px] text-zinc-500">también en el puerto {{ s.port }}</div>
            </td>
            <td class="text-center text-zinc-600">{{ s.endpointCount }}</td>
            <td class="text-right"><RunToggle :id="s.id" :running="s.running" /></td>
          </tr>
        </tbody>
      </table>
    </div>

    <ModalDialog v-if="showCreate" title="Nuevo servicio" @close="closeCreate">
      <form class="space-y-3" @submit.prevent="create">
        <div>
          <label class="label">Nombre</label>
          <input v-model="form.name" class="input" placeholder="MiFiel" autofocus required />
        </div>
        <div>
          <label class="label">Descripción</label>
          <input v-model="form.description" class="input" placeholder="Firma electrónica de documentos" />
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="label">Prefijo de rutas</label>
            <input v-model="form.prefix" class="input-mono" :placeholder="suggestedPrefix" />
          </div>
          <div>
            <label class="label">Puerto propio (opcional)</label>
            <input v-model="form.port" type="number" class="input-mono" placeholder="—" />
          </div>
        </div>
        <div>
          <label class="label">URL del proveedor real (opcional)</label>
          <input v-model="form.upstreamUrl" class="input-mono" placeholder="https://app.mifiel.com/api/v1" />
          <p class="hint">Solo como referencia; Fake Services nunca la llama.</p>
        </div>
        <p class="text-xs text-zinc-500">
          Quedará disponible en
          <code class="text-zinc-800">{{ store.status?.centralUrl }}{{ form.prefix || suggestedPrefix }}</code>
        </p>
        <IssuesList :issues="issues" />
      </form>
      <template #footer>
        <button class="btn" @click="closeCreate">Cancelar</button>
        <button class="btn btn-primary" :disabled="!form.name || saving" @click="create">Crear servicio</button>
      </template>
    </ModalDialog>
  </div>
</template>
