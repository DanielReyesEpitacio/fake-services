<script setup lang="ts">
import { ref, watch } from 'vue'
import { X } from 'lucide-vue-next'
import { api } from '../lib/api'
import { copyText, formatBytes, formatDateTime, store, toastError } from '../lib/store'
import type { CapturedBody, RequestRecord } from '../lib/types'
import MethodBadge from './MethodBadge.vue'
import OutcomeBadge from './OutcomeBadge.vue'
import StatusCode from './StatusCode.vue'

const props = defineProps<{ id: string }>()
const emit = defineEmits<{ close: [] }>()

const rec = ref<RequestRecord | null>(null)

watch(
  () => props.id,
  async (id) => {
    rec.value = null
    try {
      rec.value = await api.request(id)
    } catch (e) {
      toastError(e)
    }
  },
  { immediate: true },
)

function pretty(b: CapturedBody): string {
  if (b.encoding === 'base64') return `(binario, ${formatBytes(b.size)} en base64)\n${b.content}`
  try {
    return JSON.stringify(JSON.parse(b.content), null, 2)
  } catch {
    return b.content
  }
}

/** Reconstruye la solicitud como comando curl (sin headers ocultos). */
function asCurl() {
  const r = rec.value
  if (!r) return
  const skip = new Set(['host', 'content-length', 'connection', 'accept-encoding'])
  const parts = [`curl -X ${r.method}`]
  for (const [k, v] of r.requestHeaders) {
    if (!skip.has(k) && !v.includes('••••')) parts.push(`-H '${k}: ${v.replace(/'/g, "'\\''")}'`)
  }
  if (r.requestBody.size && r.requestBody.encoding === 'utf8') {
    parts.push(`--data-raw '${r.requestBody.content.replace(/'/g, "'\\''")}'`)
  }
  const base = store.status?.centralUrl ?? 'http://127.0.0.1:4000'
  parts.push(`'${base}${r.path}${r.query ? `?${r.query}` : ''}'`)
  copyText(parts.join(' \\\n  '))
}
</script>

<template>
  <aside class="flex h-full w-[560px] shrink-0 flex-col border-l border-zinc-200 bg-white">
    <div class="card-header">
      <div class="flex min-w-0 items-center gap-2">
        <template v-if="rec">
          <MethodBadge :method="rec.method" />
          <code class="truncate text-sm">{{ rec.path }}{{ rec.query ? `?${rec.query}` : '' }}</code>
        </template>
      </div>
      <button class="btn btn-ghost btn-sm" aria-label="Cerrar" @click="emit('close')"><X class="size-4" /></button>
    </div>
    <div v-if="!rec" class="p-4 text-sm text-zinc-500">Cargando…</div>
    <div v-else class="flex-1 space-y-4 overflow-y-auto p-4 text-sm">
      <div class="flex flex-wrap items-center gap-2">
        <StatusCode :status="rec.status" />
        <OutcomeBadge :outcome="rec.outcome" />
        <span class="text-xs text-zinc-500">{{ rec.durationMs }} ms · {{ formatDateTime(rec.tsMs) }}</span>
        <button class="btn btn-sm ml-auto" @click="asCurl">Copiar como curl</button>
      </div>
      <div v-if="rec.message" class="rounded-lg bg-amber-50 p-2.5 text-xs text-amber-900">{{ rec.message }}</div>
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
        <dt class="text-zinc-500">Servicio</dt>
        <dd>
          <RouterLink v-if="rec.serviceId" :to="`/servicios/${rec.serviceId}`" class="text-brand-700 hover:underline">{{ rec.serviceId }}</RouterLink>
          <span v-else>—</span>
        </dd>
        <dt class="text-zinc-500">Endpoint</dt>
        <dd>
          <RouterLink
            v-if="rec.serviceId && rec.endpointId"
            :to="`/servicios/${rec.serviceId}/endpoints/${rec.endpointId}`"
            class="text-brand-700 hover:underline"
          >{{ rec.endpointId }}</RouterLink>
          <span v-else>—</span>
        </dd>
        <dt class="text-zinc-500">Escenario</dt>
        <dd>{{ rec.scenarioId ?? '—' }}</dd>
        <dt class="text-zinc-500">Origen</dt>
        <dd>{{ rec.remoteAddr ?? '—' }}</dd>
      </dl>

      <section>
        <h3 class="mb-1.5 text-xs font-semibold tracking-wide text-zinc-500 uppercase">Solicitud</h3>
        <table class="mb-2 w-full font-mono text-xs">
          <tbody>
            <tr v-for="([k, v], i) in rec.requestHeaders" :key="i" class="align-top">
              <td class="w-1/3 py-0.5 pr-2 text-zinc-500">{{ k }}</td>
              <td class="py-0.5 break-all">{{ v }}</td>
            </tr>
          </tbody>
        </table>
        <pre v-if="rec.requestBody.size" class="max-h-72 overflow-auto rounded-lg bg-zinc-50 p-3 text-xs">{{ pretty(rec.requestBody) }}</pre>
        <p v-if="rec.requestBody.truncated" class="hint">Body truncado ({{ formatBytes(rec.requestBody.size) }} en total).</p>
      </section>

      <section>
        <h3 class="mb-1.5 text-xs font-semibold tracking-wide text-zinc-500 uppercase">Respuesta</h3>
        <table class="mb-2 w-full font-mono text-xs">
          <tbody>
            <tr v-for="([k, v], i) in rec.responseHeaders" :key="i" class="align-top">
              <td class="w-1/3 py-0.5 pr-2 text-zinc-500">{{ k }}</td>
              <td class="py-0.5 break-all">{{ v }}</td>
            </tr>
          </tbody>
        </table>
        <pre v-if="rec.responseBody.size" class="max-h-96 overflow-auto rounded-lg bg-zinc-50 p-3 text-xs">{{ pretty(rec.responseBody) }}</pre>
        <p v-if="rec.responseBody.truncated" class="hint">Body truncado ({{ formatBytes(rec.responseBody.size) }} en total).</p>
      </section>
    </div>
  </aside>
</template>
