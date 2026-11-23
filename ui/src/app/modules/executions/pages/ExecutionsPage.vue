<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { api } from '@/infrastructure/api/client.ts'
import { formatDateTime, onExecution, store, toast, toastError } from '@/app/shared/state/live.ts'
import { confirmAction } from '@/app/shared/state/confirm.ts'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import ExecutionStatusChip from '../components/ExecutionStatusChip.vue'
import type { ExecutionStatus, ExecutionSummary } from '@/infrastructure/api/types.ts'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()

const rows = ref<ExecutionSummary[]>([])
const loading = ref(false)
const serviceId = ref<string | null>((route.query.servicio as string) ?? null)
const workflowId = ref<string | null>((route.query.workflow as string) ?? null)
const status = ref<string | null>(null)

const serviceItems = computed(() => [
  { title: 'Todos los servicios', value: null },
  ...store.services.map((s) => ({ title: s.name, value: s.id })),
])
const statusItems = computed(() => [
  { title: 'Todos los estados', value: null },
  { title: 'Activas (pendientes, en ejecución o esperando)', value: 'pending,running,waiting' },
  ...(['completed', 'failed', 'cancelled'] as ExecutionStatus[]).map((s) => ({ title: t(`executionStatus.${s}`), value: s })),
])

async function load() {
  loading.value = true
  try {
    rows.value = await api.executions({
      serviceId: serviceId.value ?? undefined,
      workflowId: workflowId.value ?? undefined,
      status: status.value ?? undefined,
      limit: 200,
    })
  } catch (e) {
    toastError(e)
  } finally {
    loading.value = false
  }
}
load()
watch([serviceId, workflowId, status], load)

const stop = onExecution((e) => {
  const i = rows.value.findIndex((r) => r.id === e.id)
  if (i >= 0) rows.value[i] = e
  else if ((!serviceId.value || e.serviceId === serviceId.value) && (!workflowId.value || e.workflowId === workflowId.value)) {
    rows.value.unshift(e)
  }
})
onBeforeUnmount(stop)

function duration(e: ExecutionSummary): string {
  const end = e.finishedMs ?? Date.now()
  const s = Math.max(0, Math.round((end - e.createdMs) / 1000))
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`
}

const isActive = (e: ExecutionSummary) => ['pending', 'running', 'waiting'].includes(e.status)

async function cancel(e: ExecutionSummary) {
  try {
    await api.cancelExecution(e.id)
    toast('Ejecución cancelada')
  } catch (err) {
    toastError(err)
  }
}

async function clearFinished() {
  const ok = await confirmAction({
    title: 'Borrar ejecuciones terminadas',
    text: 'Se borrarán las ejecuciones completadas, fallidas y canceladas del espacio. Las activas se conservan.',
    confirmText: 'Borrar',
    danger: true,
  })
  if (!ok) return
  try {
    const { deleted } = await api.clearExecutions()
    toast(`${deleted} ejecuciones eliminadas`)
    load()
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div>
    <PageHeader title="Ejecuciones" subtitle="Cada vez que se inicia un workflow: estado, paso actual y línea de tiempo.">
      <template #actions>
        <VBtn variant="tonal" prepend-icon="mdi-refresh" @click="load">Actualizar</VBtn>
        <VBtn color="error" variant="tonal" prepend-icon="mdi-delete-outline" @click="clearFinished">Borrar terminadas</VBtn>
      </template>
    </PageHeader>

    <div class="d-flex flex-wrap align-center gap-3 mb-4">
      <FSelect v-model="serviceId" :items="serviceItems" hide-details style="max-width: 240px" />
      <FSelect v-model="status" :items="statusItems" hide-details style="max-width: 340px" />
      <VChip v-if="workflowId" closable @click:close="workflowId = null">Workflow: {{ workflowId }}</VChip>
    </div>

    <VCard>
      <VProgressLinear v-if="loading" indeterminate />
      <EmptyState
        v-if="!rows.length && !loading"
        icon="mdi-sitemap-outline"
        title="No hay ejecuciones"
        text="Se crean cuando un escenario inicia un workflow, al ejecutarlo manualmente o de forma periódica."
      />
      <VTable v-else hover class="clickable-rows">
        <thead>
          <tr>
            <th style="width: 150px">Estado</th>
            <th>Workflow</th>
            <th style="width: 190px">Origen</th>
            <th style="width: 170px">Paso actual</th>
            <th style="width: 190px">Inicio</th>
            <th style="width: 100px">Duración</th>
            <th style="width: 60px"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="e in rows" :key="e.id" @click="router.push(`/ejecuciones/${e.id}`)">
            <td><ExecutionStatusChip :status="e.status" /></td>
            <td>
              <div class="font-weight-medium">{{ e.workflowName }}</div>
              <div class="text-body-small text-medium-emphasis">{{ e.serviceId }}</div>
            </td>
            <td class="text-body-small">{{ t(`trigger.${e.trigger}`) }}</td>
            <td class="mono text-body-small">{{ isActive(e) ? (e.currentStep ?? '—') : '—' }}</td>
            <td class="text-body-small text-medium-emphasis">{{ formatDateTime(e.createdMs) }}</td>
            <td class="text-body-small text-medium-emphasis">{{ duration(e) }}</td>
            <td @click.stop>
              <VBtn v-if="isActive(e)" icon="mdi-stop-circle-outline" variant="text" size="small" title="Cancelar" @click="cancel(e)" />
            </td>
          </tr>
        </tbody>
      </VTable>
    </VCard>
  </div>
</template>
