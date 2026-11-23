<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { api } from '@/infrastructure/api/client.ts'
import { formatTime, onRequest, store, toast, toastError } from '@/app/shared/state/live.ts'
import { confirmAction } from '@/app/shared/state/confirm.ts'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import MethodChip from '@/app/shared/components/MethodChip.vue'
import OutcomeChip from '@/app/shared/components/OutcomeChip.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import StatusChip from '@/app/shared/components/StatusChip.vue'
import RequestDetail from '../components/RequestDetail.vue'
import type { RequestSummary } from '@/infrastructure/api/types.ts'

const PAGE = 100
const rows = ref<RequestSummary[]>([])
const selected = ref<string | null>(null)
const serviceId = ref<string | null>(null)
const errorsOnly = ref(false)
const search = ref('')
const live = ref(true)
const loading = ref(false)
const more = ref(false)

const serviceItems = computed(() => [
  { title: 'Todos los servicios', value: null },
  ...store.services.map((s) => ({ title: s.name, value: s.id })),
])

async function load(append = false) {
  loading.value = true
  try {
    const page = await api.requests({
      serviceId: serviceId.value ?? undefined,
      errorsOnly: errorsOnly.value,
      search: search.value || undefined,
      beforeMs: append ? rows.value[rows.value.length - 1]?.tsMs : undefined,
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
  const ok = await confirmAction({
    title: 'Borrar historial',
    text: `Se borrarán las solicitudes registradas ${scope}.`,
    confirmText: 'Borrar',
    danger: true,
  })
  if (!ok) return
  try {
    const { deleted } = await api.clearRequests(serviceId.value ?? undefined)
    toast(`${deleted} solicitudes eliminadas`)
    selected.value = null
    load()
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div>
    <PageHeader title="Historial" subtitle="Cada solicitud recibida por tus simulaciones, con headers, body y respuesta.">
      <template #actions>
        <VBtn variant="tonal" :prepend-icon="live ? 'mdi-pause' : 'mdi-play'" @click="live = !live">
          {{ live ? 'En vivo' : 'Pausado' }}
        </VBtn>
        <VBtn color="error" variant="tonal" prepend-icon="mdi-delete-outline" @click="clear">Borrar</VBtn>
      </template>
    </PageHeader>

    <div class="d-flex flex-wrap align-center gap-3 mb-4">
      <FSelect v-model="serviceId" :items="serviceItems" hide-details style="max-width: 240px" />
      <FTextField v-model="search" prepend-inner-icon="mdi-magnify" placeholder="Filtrar por ruta…" hide-details clearable style="max-width: 320px" />
      <VCheckbox v-model="errorsOnly" label="Solo errores" hide-details />
    </div>

    <VCard>
      <VProgressLinear v-if="loading" indeterminate />
      <EmptyState
        v-if="!rows.length && !loading"
        icon="mdi-radar"
        title="No hay solicitudes"
        text="Cuando tu aplicación llame a una simulación, la solicitud aparecerá aquí al instante."
      />
      <VTable v-else hover class="clickable-rows">
        <thead>
          <tr>
            <th style="width: 100px">Hora</th>
            <th style="width: 100px">Método</th>
            <th>Ruta</th>
            <th style="width: 200px">Endpoint</th>
            <th style="width: 80px">Estado</th>
            <th style="width: 180px">Resultado</th>
            <th class="text-right" style="width: 90px">Duración</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rows" :key="r.id" @click="selected = r.id">
            <td class="text-body-small text-medium-emphasis">{{ formatTime(r.tsMs) }}</td>
            <td><MethodChip :method="r.method" /></td>
            <td class="mono text-body-small text-truncate-cell">
              {{ r.path }}<span v-if="r.query" class="text-medium-emphasis">?{{ r.query }}</span>
            </td>
            <td class="text-body-small text-medium-emphasis text-truncate" style="max-width: 200px">{{ r.endpointId ?? '—' }}</td>
            <td><StatusChip :status="r.status" /></td>
            <td><OutcomeChip :outcome="r.outcome" /></td>
            <td class="text-body-small text-medium-emphasis text-right">{{ r.durationMs }} ms</td>
          </tr>
        </tbody>
      </VTable>
      <VCardActions v-if="more" class="justify-center">
        <VBtn variant="text" :loading="loading" @click="load(true)">Cargar más</VBtn>
      </VCardActions>
    </VCard>

    <RequestDetail v-model="selected" />
  </div>
</template>
