<script setup lang="ts">
// Ajustes del workflow (lo que muestra el disparador): datos generales,
// ejecución periódica, variables, escenarios que lo inician y ejecuciones.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatTime } from '@/app/shared/state/live.ts'
import JsonEditor from '@/app/shared/components/JsonEditor.vue'
import ExecutionStatusChip from '@/app/modules/executions/components/ExecutionStatusChip.vue'
import type { ExecutionSummary, Workflow, WorkflowUsage } from '@/infrastructure/api/types.ts'

defineProps<{
  sid: string
  wid: string
  usedBy: WorkflowUsage[]
  executions: ExecutionSummary[]
  /** Ejecución cuyo recorrido se muestra en el diagrama. */
  traceId?: string | null
  /** Muestra el botón para ver el recorrido en el diagrama. */
  canTrace?: boolean
}>()
const emit = defineEmits<{ trace: [id: string | null]; remove: [] }>()
const wf = defineModel<Workflow>({ required: true })
const { t } = useI18n()

const scheduled = computed({
  get: () => !!wf.value.schedule,
  set: (v) => (wf.value.schedule = v ? { everyMs: 60000 } : null),
})
</script>

<template>
  <div class="d-flex flex-column gap-4">
    <VCard title="General">
      <VCardText class="d-flex flex-column gap-3">
        <FTextField v-model="wf.name" label="Nombre" />
        <FTextarea v-model="wf.description" label="Descripción" rows="2" />
        <FTextField :model-value="wf.id" label="Identificador" class="mono" disabled />
        <VSwitch v-model="wf.enabled" label="Habilitado" color="primary" hide-details />
        <FTextField v-model.number="wf.maxSteps" label="Pasos máximos por ejecución" type="number" min="1" hint="Evita bucles infinitos." persistent-hint />
      </VCardText>
    </VCard>

    <VCard title="Ejecución periódica">
      <VCardText class="d-flex flex-column gap-3">
        <VSwitch v-model="scheduled" label="Ejecutar cada cierto tiempo" color="primary" hide-details />
        <FTextField
          v-if="wf.schedule"
          v-model.number="wf.schedule.everyMs"
          label="Intervalo (ms)"
          type="number"
          min="1000"
          hint="Solo mientras el servicio está iniciado."
          persistent-hint
        />
      </VCardText>
    </VCard>

    <VCard title="Variables iniciales">
      <VCardText>
        <JsonEditor v-model="wf.variables" empty-as="object" hint="Disponibles como vars.* (se combinan con las del servicio)." />
      </VCardText>
    </VCard>

    <VCard title="Lo inician">
      <VCardText>
        <div v-if="!usedBy.length" class="text-body-small text-medium-emphasis">
          Ningún escenario lo inicia todavía. Vincúlalo desde un endpoint, en la sección «Workflows» de un escenario.
        </div>
        <VList v-else density="compact">
          <VListItem
            v-for="u in usedBy"
            :key="`${u.endpointId}/${u.scenarioId}`"
            :to="`/servicios/${sid}/endpoints/${u.endpointId}`"
            prepend-icon="mdi-routes"
            :title="u.endpointId"
            :subtitle="`Escenario: ${u.scenarioId}`"
          />
        </VList>
      </VCardText>
    </VCard>

    <VCard title="Ejecuciones recientes">
      <template #append><VBtn variant="text" size="small" :to="`/ejecuciones?workflow=${wid}&servicio=${sid}`">Ver todas</VBtn></template>
      <VList v-if="executions.length" density="compact">
        <VListItem v-for="e in executions" :key="e.id" :to="canTrace ? undefined : `/ejecuciones/${e.id}`" :active="traceId === e.id" @click="canTrace && emit('trace', traceId === e.id ? null : e.id)">
          <template #prepend><ExecutionStatusChip :status="e.status" class="me-3" /></template>
          <VListItemTitle class="text-body-small">{{ t(`trigger.${e.trigger}`) }}</VListItemTitle>
          <VListItemSubtitle v-if="canTrace" class="text-body-small">{{ traceId === e.id ? 'Recorrido visible' : 'Ver recorrido' }}</VListItemSubtitle>
          <template #append>
            <span class="text-body-small text-medium-emphasis me-1">{{ formatTime(e.createdMs) }}</span>
            <VBtn v-if="canTrace" icon="mdi-open-in-new" variant="text" size="x-small" title="Abrir ejecución" :to="`/ejecuciones/${e.id}`" @click.stop />
          </template>
        </VListItem>
      </VList>
      <VCardText v-else class="text-body-small text-medium-emphasis">Aún no se ha ejecutado.</VCardText>
    </VCard>

    <div>
      <VBtn color="error" variant="tonal" prepend-icon="mdi-delete-outline" @click="emit('remove')">Eliminar workflow</VBtn>
    </div>
  </div>
</template>
