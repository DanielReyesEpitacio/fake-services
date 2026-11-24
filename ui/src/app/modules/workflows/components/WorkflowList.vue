<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { store, toast, toastError } from '@/app/shared/state/live.ts'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import type { Issue, WorkflowSummary } from '@/infrastructure/api/types.ts'

const props = defineProps<{ sid: string }>()
const emit = defineEmits<{ count: [n: number] }>()
const router = useRouter()

const list = ref<WorkflowSummary[]>([])

async function load() {
  try {
    list.value = await api.workflows(props.sid)
    emit('count', list.value.length)
  } catch (e) {
    toastError(e)
  }
}
load()
watch(() => store.configVersion, load)

const showNew = ref(false)
const draft = ref({ name: '', template: 'webhook' as 'webhook' | 'empty' })
const issues = ref<Issue[]>([])

async function create() {
  issues.value = []
  const steps =
    draft.value.template === 'webhook'
      ? [
          { id: 'espera', type: 'wait', durationMs: 5000 },
          {
            id: 'notificar',
            type: 'http',
            method: 'POST',
            url: '{{ request.body.callback_url }}',
            body: { type: 'json', content: { id: '{{ response.body.id }}', status: 'completed' } },
            retry: { maxAttempts: 3, backoffMs: 1000, multiplier: 2 },
          },
        ]
      : []
  try {
    const v = await api.createWorkflow(props.sid, { name: draft.value.name, enabled: true, steps } as never)
    toast('Workflow creado')
    router.push(`/servicios/${props.sid}/workflows/${v.workflow.id}`)
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  }
}

const importInput = ref<HTMLInputElement>()

async function importWorkflow(ev: Event) {
  const f = (ev.target as HTMLInputElement).files?.[0]
  ;(ev.target as HTMLInputElement).value = ''
  if (!f) return
  let wf: Record<string, unknown>
  try {
    wf = JSON.parse(await f.text())
  } catch {
    toast('El archivo no es JSON válido.', 'error')
    return
  }
  if (!Array.isArray(wf?.steps) || typeof wf?.name !== 'string') {
    toast('El archivo no parece un workflow (faltan name o steps).', 'error')
    return
  }
  try {
    let v
    try {
      v = await api.createWorkflow(props.sid, wf as never)
    } catch (e) {
      // Si el id ya existe, se importa como copia.
      if (!(e instanceof ApiError && e.status === 409)) throw e
      v = await api.createWorkflow(props.sid, { ...wf, id: '', name: `${wf.name} (importado)` } as never)
    }
    toast(`Workflow «${v.workflow.name}» importado`)
    router.push(`/servicios/${props.sid}/workflows/${v.workflow.id}`)
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) {
      toast(`No se pudo importar: ${e.issues.map((i) => `${i.field}: ${i.message}`).join('; ')}`, 'error')
    } else toastError(e)
  }
}

async function run(w: WorkflowSummary) {
  try {
    const e = await api.runWorkflow(props.sid, w.id, {})
    router.push(`/ejecuciones/${e.id}`)
  } catch (err) {
    toastError(err)
  }
}
</script>

<template>
  <VCard>
    <VCardItem>
      <VCardSubtitle>Secuencias asíncronas: esperas, webhooks, condiciones y reintentos.</VCardSubtitle>
      <template #append>
        <input ref="importInput" type="file" accept=".json,application/json" hidden @change="importWorkflow" />
        <VBtn variant="text" size="small" prepend-icon="mdi-upload" class="me-1" @click="importInput?.click()">Importar</VBtn>
        <VBtn color="primary" size="small" prepend-icon="mdi-plus" @click="(showNew = true), (draft = { name: '', template: 'webhook' })">
          Nuevo workflow
        </VBtn>
      </template>
    </VCardItem>
    <EmptyState
      v-if="!list.length"
      icon="mdi-sitemap-outline"
      title="Sin workflows"
      text="Un workflow simula lo que el proveedor hace después de responder, como enviar webhooks."
    >
      <VBtn color="primary" prepend-icon="mdi-plus" @click="showNew = true">Crear workflow</VBtn>
    </EmptyState>
    <VTable v-else hover class="clickable-rows">
      <tbody>
        <tr v-for="w in list" :key="w.id" :style="{ opacity: w.enabled ? 1 : 0.55 }" @click="router.push(`/servicios/${sid}/workflows/${w.id}`)">
          <td style="width: 40px"><VIcon icon="mdi-sitemap-outline" color="primary" /></td>
          <td>
            <div class="font-weight-medium">{{ w.name }}</div>
            <div v-if="w.description" class="text-body-small text-medium-emphasis">{{ w.description }}</div>
          </td>
          <td class="text-body-small text-medium-emphasis">{{ w.stepCount }} paso{{ w.stepCount === 1 ? '' : 's' }}</td>
          <td class="text-body-small">
            <VChip v-if="w.usedBy.length" size="small" variant="tonal" prepend-icon="mdi-routes">
              {{ w.usedBy.length }} escenario{{ w.usedBy.length === 1 ? '' : 's' }}
            </VChip>
            <VChip v-if="w.scheduleEveryMs" size="small" variant="tonal" prepend-icon="mdi-clock-outline" class="ms-1">
              cada {{ Math.round(w.scheduleEveryMs / 1000) }} s
            </VChip>
          </td>
          <td class="text-right" style="width: 120px" @click.stop>
            <VBtn size="small" variant="tonal" prepend-icon="mdi-play" :disabled="!w.enabled" @click="run(w)">Ejecutar</VBtn>
          </td>
        </tr>
      </tbody>
    </VTable>

    <VDialog v-model="showNew" max-width="520">
      <VCard title="Nuevo workflow">
        <VCardText class="d-flex flex-column gap-3">
          <FTextField v-model="draft.name" label="Nombre" placeholder="Notificar firma" autofocus />
          <VRadioGroup v-model="draft.template" hide-details>
            <VRadio value="webhook" label="Plantilla: esperar y enviar un webhook al callback_url" />
            <VRadio value="empty" label="Vacío" />
          </VRadioGroup>
          <IssuesList :issues="issues" />
        </VCardText>
        <VCardActions>
          <VSpacer />
          <VBtn variant="text" @click="showNew = false">Cancelar</VBtn>
          <VBtn color="primary" :disabled="!draft.name" @click="create">Crear y editar</VBtn>
        </VCardActions>
      </VCard>
    </VDialog>
  </VCard>
</template>
