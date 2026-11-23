<script setup lang="ts">
import { computed, ref } from 'vue'
import { CircleAlert, Plus } from 'lucide-vue-next'
import { store, formatTime } from '../lib/store'
import CopyButton from '../components/CopyButton.vue'
import EmptyState from '../components/EmptyState.vue'
import MethodBadge from '../components/MethodBadge.vue'
import OutcomeBadge from '../components/OutcomeBadge.vue'
import RequestDetail from '../components/RequestDetail.vue'
import RunToggle from '../components/RunToggle.vue'
import RunningDot from '../components/RunningDot.vue'
import StatusCode from '../components/StatusCode.vue'

const selected = ref<string | null>(null)

const errors = computed(() =>
  store.recent.filter((r) => r.outcome !== 'matched' && r.outcome !== 'preflight').slice(0, 6),
)
const listenerErrors = computed(() => store.status?.listeners.filter((l) => l.error) ?? [])

const metrics = computed(() => [
  { label: 'Servicios activos', value: `${store.status?.servicesRunning ?? 0} / ${store.status?.servicesTotal ?? 0}` },
  { label: 'Endpoints', value: store.status?.endpointsTotal ?? 0 },
  { label: 'Solicitudes registradas', value: store.status?.requestsTotal ?? 0 },
  { label: 'Workflows en ejecución', value: '—', hint: 'Disponible en la fase 2' },
])
</script>

<template>
  <div class="flex h-full">
    <div class="min-w-0 flex-1 space-y-6 p-8">
      <header class="flex items-end justify-between">
        <div>
          <h1 class="page-title">Inicio</h1>
          <p class="page-subtitle">
            Tus simulaciones responden en
            <code class="text-zinc-800">{{ store.status?.centralUrl }}</code>
          </p>
        </div>
        <RouterLink to="/servicios?nuevo=1" class="btn btn-primary"><Plus class="size-4" /> Nuevo servicio</RouterLink>
      </header>

      <div
        v-for="l in listenerErrors"
        :key="l.port"
        class="flex items-center gap-2 rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800"
      >
        <CircleAlert class="size-4" /> {{ l.error }}
      </div>
      <div v-if="store.status?.problems.length" class="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
        <div class="mb-1 font-medium">Algunos archivos de configuración no se pudieron cargar</div>
        <div v-for="p in store.status.problems" :key="p.file" class="text-xs"><code>{{ p.file }}</code>: {{ p.message }}</div>
      </div>

      <div class="grid grid-cols-4 gap-3">
        <div v-for="m in metrics" :key="m.label" class="rounded-xl bg-white p-4 ring-1 ring-zinc-200">
          <div class="text-xs text-zinc-500">{{ m.label }}</div>
          <div class="mt-1 text-2xl font-semibold tracking-tight">{{ m.value }}</div>
          <div v-if="m.hint" class="text-[11px] text-zinc-400">{{ m.hint }}</div>
        </div>
      </div>

      <div class="grid grid-cols-5 gap-6">
        <section class="card col-span-2">
          <div class="card-header">
            <h2 class="card-title">Servicios</h2>
            <RouterLink to="/servicios" class="text-xs text-brand-700 hover:underline">Ver todos</RouterLink>
          </div>
          <EmptyState v-if="!store.services.length" title="Aún no hay servicios" text="Crea uno para empezar a simular un proveedor externo." />
          <ul v-else class="divide-y divide-zinc-100">
            <li v-for="s in store.services" :key="s.id" class="flex items-center gap-3 px-4 py-2.5">
              <RunningDot :running="s.running" />
              <div class="min-w-0 flex-1">
                <RouterLink :to="`/servicios/${s.id}`" class="text-sm font-medium hover:text-brand-700">{{ s.name }}</RouterLink>
                <div class="flex items-center gap-1 text-xs text-zinc-500">
                  <code class="truncate">{{ s.url }}</code>
                  <CopyButton :text="s.url" class="text-zinc-400 hover:text-zinc-700" />
                </div>
              </div>
              <RunToggle :id="s.id" :running="s.running" />
            </li>
          </ul>
        </section>

        <section class="card col-span-3">
          <div class="card-header">
            <h2 class="card-title">Solicitudes recientes</h2>
            <RouterLink to="/historial" class="text-xs text-brand-700 hover:underline">Ver historial</RouterLink>
          </div>
          <EmptyState
            v-if="!store.recent.length"
            title="Sin solicitudes todavía"
            text="Apunta tu aplicación a la URL de un servicio y verás aquí cada llamada en tiempo real."
          />
          <table v-else class="table">
            <tbody>
              <tr
                v-for="r in store.recent.slice(0, 12)"
                :key="r.id"
                class="cursor-pointer hover:bg-zinc-50"
                :class="{ 'bg-brand-50': selected === r.id }"
                @click="selected = r.id"
              >
                <td class="w-20 text-xs text-zinc-500">{{ formatTime(r.tsMs) }}</td>
                <td class="w-20"><MethodBadge :method="r.method" /></td>
                <td class="max-w-0 truncate font-mono text-xs">{{ r.path }}</td>
                <td class="w-16"><StatusCode :status="r.status" /></td>
                <td class="w-16 text-right text-xs text-zinc-500">{{ r.durationMs }} ms</td>
              </tr>
            </tbody>
          </table>
        </section>
      </div>

      <section v-if="errors.length" class="card">
        <div class="card-header"><h2 class="card-title">Errores recientes</h2></div>
        <ul class="divide-y divide-zinc-100">
          <li
            v-for="r in errors"
            :key="r.id"
            class="flex cursor-pointer items-center gap-3 px-4 py-2 text-sm hover:bg-zinc-50"
            @click="selected = r.id"
          >
            <OutcomeBadge :outcome="r.outcome" />
            <code class="text-xs">{{ r.method }} {{ r.path }}</code>
            <span class="flex-1 truncate text-xs text-zinc-500">{{ r.message }}</span>
            <span class="text-xs text-zinc-400">{{ formatTime(r.tsMs) }}</span>
          </li>
        </ul>
      </section>
    </div>
    <RequestDetail v-if="selected" :id="selected" @close="selected = null" />
  </div>
</template>
