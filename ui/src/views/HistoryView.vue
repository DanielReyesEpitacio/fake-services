<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { Pause, Play, Trash2 } from 'lucide-vue-next'
import { api } from '../lib/api'
import { formatTime, onRequest, store, toast, toastError } from '../lib/store'
import EmptyState from '../components/EmptyState.vue'
import MethodBadge from '../components/MethodBadge.vue'
import OutcomeBadge from '../components/OutcomeBadge.vue'
import RequestDetail from '../components/RequestDetail.vue'
import StatusCode from '../components/StatusCode.vue'
import type { RequestSummary } from '../lib/types'

const PAGE = 100
const rows = ref<RequestSummary[]>([])
const selected = ref<string | null>(null)
const serviceId = ref('')
const errorsOnly = ref(false)
const search = ref('')
const live = ref(true)
const loading = ref(false)
const more = ref(false)

async function load(append = false) {
  loading.value = true
  try {
    const before = append ? rows.value[rows.value.length - 1]?.tsMs : undefined
    const page = await api.requests({
      serviceId: serviceId.value || undefined,
      errorsOnly: errorsOnly.value,
      search: search.value || undefined,
      beforeMs: before,
      limit: PAGE,
    })
    rows.value = append ? [...rows.value, ...page] : page
    more.value = page.length === PAGE
  } catch (e) {
    toastError(e)
  } finally {
    loading.value = false
  }
}
load()

let timer: ReturnType<typeof setTimeout> | undefined
watch([serviceId, errorsOnly, search], () => {
  clearTimeout(timer)
  timer = setTimeout(() => load(), 200)
})

function matches(r: RequestSummary) {
  if (serviceId.value && r.serviceId !== serviceId.value) return false
  if (errorsOnly.value && (r.outcome === 'matched' || r.outcome === 'preflight')) return false
  if (search.value && !r.path.includes(search.value)) return false
  return true
}

const stop = onRequest((r) => {
  if (live.value && matches(r)) rows.value.unshift(r)
})
onBeforeUnmount(stop)

async function clear() {
  const scope = serviceId.value ? `del servicio “${serviceId.value}”` : 'de todo el espacio'
  if (!confirm(`¿Borrar el historial ${scope}?`)) return
  try {
    const { deleted } = await api.clearRequests(serviceId.value || undefined)
    toast(`${deleted} solicitudes eliminadas`)
    selected.value = null
    load()
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div class="flex h-full">
    <div class="min-w-0 flex-1 space-y-4 p-8">
      <header class="flex items-end justify-between">
        <div>
          <h1 class="page-title">Historial</h1>
          <p class="page-subtitle">Cada solicitud recibida por tus simulaciones, con headers, body y respuesta.</p>
        </div>
        <div class="flex gap-2">
          <button class="btn" :title="live ? 'Pausar actualización en vivo' : 'Reanudar'" @click="live = !live">
            <Pause v-if="live" class="size-3.5" />
            <Play v-else class="size-3.5" />
            {{ live ? 'En vivo' : 'Pausado' }}
          </button>
          <button class="btn btn-danger" @click="clear"><Trash2 class="size-3.5" /> Borrar</button>
        </div>
      </header>

      <div class="flex flex-wrap items-center gap-2">
        <select v-model="serviceId" class="input w-52">
          <option value="">Todos los servicios</option>
          <option v-for="s in store.services" :key="s.id" :value="s.id">{{ s.name }}</option>
        </select>
        <input v-model="search" class="input w-72" placeholder="Filtrar por ruta…" />
        <label class="flex items-center gap-2 text-sm">
          <input v-model="errorsOnly" type="checkbox" class="size-4 accent-brand-600" /> Solo errores
        </label>
      </div>

      <div class="card overflow-hidden">
        <EmptyState
          v-if="!rows.length && !loading"
          title="No hay solicitudes"
          text="Cuando tu aplicación llame a una simulación, la solicitud aparecerá aquí al instante."
        />
        <table v-else class="table">
          <thead>
            <tr>
              <th class="w-24">Hora</th>
              <th class="w-20">Método</th>
              <th>Ruta</th>
              <th class="w-32">Endpoint</th>
              <th class="w-16">Estado</th>
              <th class="w-36">Resultado</th>
              <th class="w-20 text-right">Duración</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="r in rows"
              :key="r.id"
              class="cursor-pointer hover:bg-zinc-50"
              :class="{ 'bg-brand-50': selected === r.id }"
              @click="selected = r.id"
            >
              <td class="text-xs text-zinc-500">{{ formatTime(r.tsMs) }}</td>
              <td><MethodBadge :method="r.method" /></td>
              <td class="max-w-0 truncate font-mono text-xs">{{ r.path }}<span v-if="r.query" class="text-zinc-400">?{{ r.query }}</span></td>
              <td class="truncate text-xs text-zinc-500">{{ r.endpointId ?? '—' }}</td>
              <td><StatusCode :status="r.status" /></td>
              <td><OutcomeBadge :outcome="r.outcome" /></td>
              <td class="text-right text-xs text-zinc-500">{{ r.durationMs }} ms</td>
            </tr>
          </tbody>
        </table>
        <div v-if="more" class="border-t border-zinc-100 p-2 text-center">
          <button class="btn btn-ghost btn-sm" :disabled="loading" @click="load(true)">Cargar más</button>
        </div>
      </div>
    </div>
    <RequestDetail v-if="selected" :id="selected" @close="selected = null" />
  </div>
</template>
