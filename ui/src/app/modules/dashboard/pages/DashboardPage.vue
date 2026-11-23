<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatTime, store } from '@/app/shared/state/live.ts'
import CopyButton from '@/app/shared/components/CopyButton.vue'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import MethodChip from '@/app/shared/components/MethodChip.vue'
import OutcomeChip from '@/app/shared/components/OutcomeChip.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import RunToggle from '@/app/shared/components/RunToggle.vue'
import RunningDot from '@/app/shared/components/RunningDot.vue'
import StatusChip from '@/app/shared/components/StatusChip.vue'
import RequestDetail from '@/app/modules/history/components/RequestDetail.vue'

const selected = ref<string | null>(null)

const errors = computed(() =>
  store.recent.filter((r) => r.outcome !== 'matched' && r.outcome !== 'preflight').slice(0, 6),
)
const listenerErrors = computed(() => store.status?.listeners.filter((l) => l.error) ?? [])

const metrics = computed(() => [
  {
    label: 'Servicios activos',
    value: `${store.status?.servicesRunning ?? 0} / ${store.status?.servicesTotal ?? 0}`,
    icon: 'mdi-server-outline',
  },
  { label: 'Endpoints', value: store.status?.endpointsTotal ?? 0, icon: 'mdi-routes' },
  { label: 'Solicitudes registradas', value: store.status?.requestsTotal ?? 0, icon: 'mdi-swap-horizontal' },
  { label: 'Workflows en ejecución', value: '—', icon: 'mdi-sitemap-outline', hint: 'Disponible en la fase 2' },
])
</script>

<template>
  <div>
    <PageHeader title="Inicio">
      <template #subtitle>
        Tus simulaciones responden en <code class="inline">{{ store.status?.centralUrl }}</code>
      </template>
      <template #actions>
        <VBtn color="primary" prepend-icon="mdi-plus" to="/servicios?nuevo=1">Nuevo servicio</VBtn>
      </template>
    </PageHeader>

    <VAlert v-for="l in listenerErrors" :key="l.port" type="error" variant="tonal" class="mb-3">{{ l.error }}</VAlert>
    <VAlert
      v-if="store.status?.problems.length"
      type="warning"
      variant="tonal"
      title="Algunos archivos de configuración no se pudieron cargar"
      class="mb-3"
    >
      <div v-for="p in store.status.problems" :key="p.file" class="text-body-small">
        <code class="inline">{{ p.file }}</code>: {{ p.message }}
      </div>
    </VAlert>

    <VRow class="mb-2">
      <VCol v-for="m in metrics" :key="m.label" cols="12" sm="6" lg="3">
        <VCard>
          <VCardText class="d-flex align-center gap-3">
            <VAvatar color="primary" :icon="m.icon" />
            <div>
              <div class="text-body-small text-medium-emphasis">{{ m.label }}</div>
              <div class="text-headline-small font-weight-bold">{{ m.value }}</div>
              <div v-if="m.hint" class="text-body-small text-disabled">{{ m.hint }}</div>
            </div>
          </VCardText>
        </VCard>
      </VCol>
    </VRow>

    <VRow>
      <VCol cols="12" lg="5">
        <VCard title="Servicios" class="h-100">
          <template #append><VBtn variant="text" size="small" to="/servicios">Ver todos</VBtn></template>
          <EmptyState
            v-if="!store.services.length"
            icon="mdi-server-plus-outline"
            title="Aún no hay servicios"
            text="Crea uno para empezar a simular un proveedor externo."
          />
          <VList v-else>
            <VListItem v-for="s in store.services" :key="s.id" :to="`/servicios/${s.id}`">
              <template #prepend><RunningDot :running="s.running" class="me-3" /></template>
              <VListItemTitle class="font-weight-medium">{{ s.name }}</VListItemTitle>
              <VListItemSubtitle class="mono">{{ s.url }}</VListItemSubtitle>
              <template #append>
                <CopyButton :text="s.url" class="me-2" />
                <RunToggle :id="s.id" :running="s.running" />
              </template>
            </VListItem>
          </VList>
        </VCard>
      </VCol>

      <VCol cols="12" lg="7">
        <VCard title="Solicitudes recientes" class="h-100">
          <template #append><VBtn variant="text" size="small" to="/historial">Ver historial</VBtn></template>
          <EmptyState
            v-if="!store.recent.length"
            icon="mdi-radar"
            title="Sin solicitudes todavía"
            text="Apunta tu aplicación a la URL de un servicio y verás aquí cada llamada en tiempo real."
          />
          <VTable v-else hover class="clickable-rows">
            <tbody>
              <tr v-for="r in store.recent.slice(0, 12)" :key="r.id" @click="selected = r.id">
                <td class="text-body-small text-medium-emphasis" style="width: 90px">{{ formatTime(r.tsMs) }}</td>
                <td style="width: 90px"><MethodChip :method="r.method" /></td>
                <td class="mono text-body-small text-truncate-cell">{{ r.path }}</td>
                <td style="width: 70px"><StatusChip :status="r.status" /></td>
                <td class="text-body-small text-medium-emphasis text-right" style="width: 70px">{{ r.durationMs }} ms</td>
              </tr>
            </tbody>
          </VTable>
        </VCard>
      </VCol>

      <VCol v-if="errors.length" cols="12">
        <VCard title="Errores recientes">
          <VList>
            <VListItem v-for="r in errors" :key="r.id" @click="selected = r.id">
              <template #prepend><OutcomeChip :outcome="r.outcome" class="me-3" /></template>
              <VListItemTitle class="mono text-body-small">{{ r.method }} {{ r.path }}</VListItemTitle>
              <VListItemSubtitle>{{ r.message }}</VListItemSubtitle>
              <template #append><span class="text-body-small text-medium-emphasis">{{ formatTime(r.tsMs) }}</span></template>
            </VListItem>
          </VList>
        </VCard>
      </VCol>
    </VRow>

    <RequestDetail v-model="selected" />
  </div>
</template>
