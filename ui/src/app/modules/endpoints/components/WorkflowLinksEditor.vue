<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import JsonEditor from '@/app/shared/components/JsonEditor.vue'
import type { WorkflowLink } from '@/infrastructure/api/types.ts'

const props = defineProps<{ sid: string; workflows: { id: string; name: string }[] }>()
const model = defineModel<WorkflowLink[]>({ required: true })
const { t } = useI18n()

const items = computed(() => props.workflows.map((w) => ({ title: w.name, value: w.id })))
const whenItems = computed(() =>
  (['responseSent', 'requestReceived'] as const).map((value) => ({ value, title: t(`startWhen.${value}`) })),
)

function add() {
  model.value.push({ workflowId: props.workflows[0]?.id ?? '', when: 'responseSent', enabled: true })
}
</script>

<template>
  <div class="d-flex flex-column gap-3">
    <div v-if="!workflows.length" class="text-body-small text-medium-emphasis">
      Este servicio aún no tiene workflows.
      <RouterLink :to="`/servicios/${sid}?tab=workflows`">Crea uno</RouterLink> para simular webhooks u otros procesos asíncronos.
    </div>
    <VCard v-for="(l, i) in model" :key="i" variant="outlined">
      <VCardText class="d-flex flex-column gap-3">
        <div class="d-flex flex-wrap align-end gap-2">
          <FSelect v-model="l.workflowId" :items="items" label="Workflow" style="min-width: 220px" class="flex-grow-1" />
          <FSelect :model-value="l.when ?? 'responseSent'" :items="whenItems" label="Cuándo" style="min-width: 220px" @update:model-value="l.when = $event" />
          <VSwitch :model-value="l.enabled !== false" label="Activo" color="primary" hide-details @update:model-value="l.enabled = !!$event" />
          <VBtn icon="mdi-open-in-new" variant="text" size="small" title="Abrir workflow" :to="`/servicios/${sid}/workflows/${l.workflowId}`" :disabled="!l.workflowId" />
          <VBtn icon="mdi-delete-outline" variant="text" size="small" aria-label="Quitar" @click="model.splice(i, 1)" />
        </div>
        <JsonEditor
          v-model="l.input"
          label="Entrada (opcional)"
          hint='Objeto JSON, p. ej. {"documento": "{{ request.params.id }}"}; disponible como input.* en el workflow.'
        />
      </VCardText>
    </VCard>
    <div>
      <VBtn variant="text" size="small" color="primary" prepend-icon="mdi-plus" :disabled="!workflows.length" @click="add">
        Iniciar un workflow
      </VBtn>
    </div>
    <div class="text-body-small text-medium-emphasis">
      El workflow recibe la solicitud original (<code class="inline">request.*</code>) y, si inicia al completar la respuesta,
      también la respuesta (<code class="inline">response.*</code>).
    </div>
  </div>
</template>
