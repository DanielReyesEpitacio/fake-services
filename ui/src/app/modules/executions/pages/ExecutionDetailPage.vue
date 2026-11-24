<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { formatDateTime, onExecution, toast, toastError } from '@/app/shared/state/live.ts'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import ExecutionStatusChip from '../components/ExecutionStatusChip.vue'
import type { ExecutionDetail, TimelineEntry } from '@/infrastructure/api/types.ts'

const props = defineProps<{ id: string }>()
const { t } = useI18n()

const detail = ref<ExecutionDetail | null>(null)
const notFound = ref(false)
const tab = ref<'vars' | 'steps' | 'input' | 'request' | 'response'>('vars')

async function load() {
  try {
    detail.value = await api.execution(props.id)
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) notFound.value = true
    else toastError(e)
  }
}
load()

// Se actualiza en vivo con cada cambio de esta ejecución.
let pending: ReturnType<typeof setTimeout> | undefined
const stop = onExecution((e) => {
  if (e.id !== props.id) return
  clearTimeout(pending)
  pending = setTimeout(load, 100)
})
onBeforeUnmount(() => {
  stop()
  clearTimeout(pending)
})

// Reloj para la cuenta regresiva de las esperas.
const now = ref(Date.now())
const clock = setInterval(() => (now.value = Date.now()), 500)
onBeforeUnmount(() => clearInterval(clock))

const exec = computed(() => detail.value?.execution)
const active = computed(() => !!exec.value && ['pending', 'running', 'waiting'].includes(exec.value.status))
const wakeIn = computed(() => {
  const w = exec.value?.wakeAtMs
  if (!w || exec.value?.status !== 'waiting') return null
  return Math.max(0, Math.ceil((w - now.value) / 1000))
})

const kindLook: Record<string, { color?: string; icon: string }> = {
  started: { color: 'primary', icon: 'mdi-play' },
  recovered: { color: 'info', icon: 'mdi-restore' },
  waiting: { color: 'info', icon: 'mdi-timer-sand' },
  http_request: { color: 'primary', icon: 'mdi-send' },
  http_response: { color: 'success', icon: 'mdi-download' },
  retry_scheduled: { color: 'warning', icon: 'mdi-refresh' },
  event_emitted: { color: 'secondary', icon: 'mdi-broadcast' },
  variables_set: { icon: 'mdi-variable' },
  condition_evaluated: { icon: 'mdi-call-split' },
  child_started: { icon: 'mdi-sitemap-outline' },
  step_completed: { color: 'success', icon: 'mdi-check' },
  step_failed: { color: 'error', icon: 'mdi-alert' },
  error_handled: { color: 'warning', icon: 'mdi-call-missed' },
  completed: { color: 'success', icon: 'mdi-flag-checkered' },
  failed: { color: 'error', icon: 'mdi-alert-circle' },
  cancelled: { icon: 'mdi-close-circle' },
}
const look = (e: TimelineEntry) => kindLook[e.kind] ?? { icon: 'mdi-circle-small' }

function timeOf(ms: number) {
  return new Date(ms).toLocaleTimeString('es-MX', { hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23' }) +
    `.${String(ms % 1000).padStart(3, '0')}`
}

function json(v: unknown) {
  return v === undefined || v === null ? '—' : JSON.stringify(v, null, 2)
}

const templateHint = computed(() => '{' + '{ ' + tab.value + '.… }' + '}')

const ctx = computed(() => (exec.value?.context ?? {}) as Record<string, unknown>)

async function cancel() {
  try {
    await api.cancelExecution(props.id)
    toast('Ejecución cancelada')
    load()
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div>
    <EmptyState v-if="notFound" icon="mdi-help-circle-outline" title="Esta ejecución no existe" text="Puede que se haya borrado.">
      <VBtn to="/ejecuciones">Ver ejecuciones</VBtn>
    </EmptyState>

    <template v-else-if="exec">
      <PageHeader :title="exec.workflowName">
        <template #overline>
          <VBtn variant="text" size="small" prepend-icon="mdi-chevron-left" to="/ejecuciones" class="mb-1 px-1">Ejecuciones</VBtn>
        </template>
        <template #subtitle>
          <span class="mono">{{ exec.id }}</span>
        </template>
        <template #actions>
          <VBtn variant="tonal" prepend-icon="mdi-sitemap-outline" :to="`/servicios/${exec.serviceId}/workflows/${exec.workflowId}`">
            Ver workflow
          </VBtn>
          <VBtn v-if="active" color="error" variant="tonal" prepend-icon="mdi-stop-circle-outline" @click="cancel">Cancelar</VBtn>
        </template>
      </PageHeader>

      <VRow class="mb-2">
        <VCol cols="12" sm="6" lg="3">
          <VCard><VCardText>
            <div class="text-body-small text-medium-emphasis mb-1">Estado</div>
            <ExecutionStatusChip :status="exec.status" />
            <div v-if="wakeIn !== null" class="text-body-small text-medium-emphasis mt-1">Continúa en {{ wakeIn }} s</div>
          </VCardText></VCard>
        </VCol>
        <VCol cols="12" sm="6" lg="3">
          <VCard><VCardText>
            <div class="text-body-small text-medium-emphasis mb-1">Origen</div>
            <div class="font-weight-medium">{{ t(`trigger.${exec.trigger}`) }}</div>
            <RouterLink v-if="exec.parentId" :to="`/ejecuciones/${exec.parentId}`" class="text-body-small">Ejecución padre</RouterLink>
          </VCardText></VCard>
        </VCol>
        <VCol cols="12" sm="6" lg="3">
          <VCard><VCardText>
            <div class="text-body-small text-medium-emphasis mb-1">Paso actual</div>
            <div class="mono">{{ active ? (exec.currentStep ?? '—') : '—' }}</div>
            <div class="text-body-small text-medium-emphasis">{{ exec.stepsExecuted }} pasos ejecutados</div>
          </VCardText></VCard>
        </VCol>
        <VCol cols="12" sm="6" lg="3">
          <VCard><VCardText>
            <div class="text-body-small text-medium-emphasis mb-1">Inicio</div>
            <div class="text-body-medium">{{ formatDateTime(exec.createdMs) }}</div>
            <div v-if="exec.finishedMs" class="text-body-small text-medium-emphasis">
              Duró {{ ((exec.finishedMs - exec.createdMs) / 1000).toFixed(1) }} s
            </div>
          </VCardText></VCard>
        </VCol>
      </VRow>

      <VAlert v-if="exec.error" type="error" variant="tonal" class="mb-4" :title="exec.status === 'cancelled' ? 'Cancelada' : 'Error'">
        {{ exec.error }}
      </VAlert>

      <VRow>
        <VCol cols="12" lg="7">
          <VCard title="Línea de tiempo">
            <VCardText>
              <VTimeline side="end" density="compact" truncate-line="both" align="start">
                <VTimelineItem
                  v-for="e in detail!.timeline"
                  :key="e.seq"
                  :dot-color="look(e).color ?? 'surface-variant'"
                  :icon="look(e).icon"
                  size="small"
                  fill-dot
                >
                  <div class="d-flex flex-wrap align-center gap-2">
                    <span class="mono text-body-small text-medium-emphasis">{{ timeOf(e.tsMs) }}</span>
                    <VChip v-if="e.stepId" size="x-small" variant="tonal" class="mono">{{ e.stepId }}</VChip>
                  </div>
                  <div class="text-body-medium mt-1">{{ e.message }}</div>
                  <details v-if="e.data !== undefined && e.data !== null" class="mt-1">
                    <summary class="text-body-small text-medium-emphasis" style="cursor: pointer">Detalles</summary>
                    <pre class="code-block mt-1">{{ json(e.data) }}</pre>
                  </details>
                </VTimelineItem>
              </VTimeline>
            </VCardText>
          </VCard>
        </VCol>

        <VCol cols="12" lg="5">
          <VCard title="Contexto de la ejecución">
            <VCardText>
              <VTabs v-model="tab" class="mb-3">
                <VTab value="vars">Variables</VTab>
                <VTab value="steps">Pasos</VTab>
                <VTab value="input">Entrada</VTab>
                <VTab value="request">Solicitud</VTab>
                <VTab value="response">Respuesta</VTab>
              </VTabs>
              <pre class="code-block">{{ json(ctx[tab]) }}</pre>
              <div class="text-body-small text-medium-emphasis mt-2">
                Úsalos en plantillas como <code class="inline">{{ templateHint }}</code>.
              </div>
            </VCardText>
          </VCard>
        </VCol>
      </VRow>
    </template>
  </div>
</template>
