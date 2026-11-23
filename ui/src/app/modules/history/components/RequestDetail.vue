<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { api } from '@/infrastructure/api/client.ts'
import { copyText, formatBytes, formatDateTime, store, toastError } from '@/app/shared/state/live.ts'
import type { CapturedBody, RequestRecord } from '@/infrastructure/api/types.ts'
import MethodChip from '@/app/shared/components/MethodChip.vue'
import OutcomeChip from '@/app/shared/components/OutcomeChip.vue'
import StatusChip from '@/app/shared/components/StatusChip.vue'

/** Id de la solicitud a mostrar; `null` cierra el panel. */
const id = defineModel<string | null>({ required: true })

const rec = ref<RequestRecord | null>(null)
const open = computed({
  get: () => id.value !== null,
  set: (v) => {
    if (!v) id.value = null
  },
})

watch(
  id,
  async (value) => {
    rec.value = null
    if (!value) return
    try {
      rec.value = await api.request(value)
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

/** Reconstruye la solicitud como comando curl (omite headers ocultos). */
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

/** Ejecuciones de workflows que inició esta solicitud (header de respuesta). */
const executionIds = computed(() => {
  const h = rec.value?.responseHeaders.find(([k]) => k === 'x-fakes-executions')
  return h ? h[1].split(',').map((s) => s.trim()).filter(Boolean) : []
})

const sections = computed(() =>
  rec.value
    ? [
        { title: 'Solicitud', headers: rec.value.requestHeaders, body: rec.value.requestBody },
        { title: 'Respuesta', headers: rec.value.responseHeaders, body: rec.value.responseBody },
      ]
    : [],
)
</script>

<template>
  <VNavigationDrawer v-model="open" location="end" temporary width="580">
    <div v-if="!rec" class="pa-6 d-flex justify-center"><VProgressCircular indeterminate /></div>
    <div v-else class="pa-4 d-flex flex-column gap-4">
      <div class="d-flex align-center gap-2">
        <MethodChip :method="rec.method" />
        <code class="text-truncate">{{ rec.path }}{{ rec.query ? `?${rec.query}` : '' }}</code>
        <VSpacer />
        <VBtn icon="mdi-close" variant="text" size="small" aria-label="Cerrar" @click="id = null" />
      </div>
      <div class="d-flex flex-wrap align-center gap-2">
        <StatusChip :status="rec.status" />
        <OutcomeChip :outcome="rec.outcome" />
        <span class="text-body-small text-medium-emphasis">{{ rec.durationMs }} ms · {{ formatDateTime(rec.tsMs) }}</span>
        <VSpacer />
        <VBtn size="small" variant="tonal" prepend-icon="mdi-console" @click="asCurl">Copiar como curl</VBtn>
      </div>
      <VAlert v-if="rec.message" type="warning" variant="tonal" density="compact">{{ rec.message }}</VAlert>
      <VAlert v-if="executionIds.length" type="info" variant="tonal" density="compact" icon="mdi-sitemap-outline">
        Inició {{ executionIds.length === 1 ? 'una ejecución' : `${executionIds.length} ejecuciones` }} de workflow:
        <RouterLink v-for="e in executionIds" :key="e" :to="`/ejecuciones/${e}`" class="d-block mono text-body-small" @click="id = null">
          {{ e }}
        </RouterLink>
      </VAlert>

      <VTable density="compact">
        <tbody>
          <tr>
            <td class="text-medium-emphasis" style="width: 120px">Servicio</td>
            <td>
              <RouterLink v-if="rec.serviceId" :to="`/servicios/${rec.serviceId}`" @click="id = null">{{ rec.serviceId }}</RouterLink>
              <span v-else>—</span>
            </td>
          </tr>
          <tr>
            <td class="text-medium-emphasis">Endpoint</td>
            <td>
              <RouterLink
                v-if="rec.serviceId && rec.endpointId"
                :to="`/servicios/${rec.serviceId}/endpoints/${rec.endpointId}`"
                @click="id = null"
              >{{ rec.endpointId }}</RouterLink>
              <span v-else>—</span>
            </td>
          </tr>
          <tr>
            <td class="text-medium-emphasis">Escenario</td>
            <td>{{ rec.scenarioId ?? '—' }}</td>
          </tr>
          <tr>
            <td class="text-medium-emphasis">Origen</td>
            <td>{{ rec.remoteAddr ?? '—' }}</td>
          </tr>
        </tbody>
      </VTable>

      <section v-for="s in sections" :key="s.title">
        <div class="text-title-small font-weight-bold mb-2">{{ s.title }}</div>
        <VTable density="compact" class="mb-2">
          <tbody>
            <tr v-for="([k, v], i) in s.headers" :key="i">
              <td class="mono text-body-small text-medium-emphasis" style="width: 36%; vertical-align: top">{{ k }}</td>
              <td class="mono text-body-small" style="word-break: break-all">{{ v }}</td>
            </tr>
          </tbody>
        </VTable>
        <pre v-if="s.body.size" class="code-block">{{ pretty(s.body) }}</pre>
        <div v-if="s.body.truncated" class="text-body-small text-medium-emphasis mt-1">
          Body truncado ({{ formatBytes(s.body.size) }} en total).
        </div>
      </section>
    </div>
  </VNavigationDrawer>
</template>
