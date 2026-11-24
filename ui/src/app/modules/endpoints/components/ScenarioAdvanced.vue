<script setup lang="ts">
// Opciones avanzadas de un escenario: probabilidad y recurso persistente.
import { computed } from 'vue'
import JsonEditor from '@/app/shared/components/JsonEditor.vue'
import type { ResourceOp, Scenario } from '@/infrastructure/api/types.ts'

const model = defineModel<Scenario>({ required: true })

const operations: { value: ResourceOp; title: string; hint: string }[] = [
  { value: 'create', title: 'Crear', hint: 'Guarda un registro nuevo (id generado si lo dejas vacío).' },
  { value: 'get', title: 'Leer', hint: 'Lee el registro; si no existe responde la respuesta de «no existe».' },
  { value: 'update', title: 'Actualizar', hint: 'Combina los datos con el registro existente.' },
  { value: 'upsert', title: 'Crear o actualizar', hint: 'Actualiza o crea si no existe.' },
  { value: 'delete', title: 'Eliminar', hint: 'Borra el registro y lo deja disponible en la respuesta.' },
  { value: 'list', title: 'Listar', hint: 'Todos los registros de la colección en {{ resources }}.' },
]

const useProbability = computed({
  get: () => model.value.probability != null,
  set: (v) => (model.value.probability = v ? 0.5 : null),
})
const percent = computed({
  get: () => Math.round((model.value.probability ?? 0) * 100),
  set: (v) => (model.value.probability = Math.min(100, Math.max(1, Number(v) || 1)) / 100),
})

const useResource = computed({
  get: () => !!model.value.resource,
  set: (v) => {
    model.value.resource = v ? { collection: 'items', operation: 'create', recordId: '', data: {} } : null
  },
})
const res = computed(() => model.value.resource!)
const needsId = computed(() => ['get', 'update', 'upsert', 'delete'].includes(res.value?.operation))
const takesData = computed(() => ['create', 'update', 'upsert'].includes(res.value?.operation))
const canMiss = computed(() => ['get', 'update', 'delete'].includes(res.value?.operation))
const opHint = computed(() => operations.find((o) => o.value === res.value?.operation)?.hint ?? '')
</script>

<template>
  <div class="d-flex flex-column gap-3">
    <div>
      <VSwitch v-model="useProbability" label="Aplicar solo una parte de las veces (fallas intermitentes)" color="primary" hide-details />
      <div v-if="useProbability" class="d-flex align-center gap-3 mt-1">
        <VSlider v-model="percent" :min="1" :max="100" :step="1" color="primary" hide-details class="flex-1-1" />
        <FTextField v-model="percent" type="number" suffix="%" hide-details style="max-width: 110px" />
      </div>
      <div v-if="useProbability" class="text-body-small text-medium-emphasis">
        Si las condiciones se cumplen, responde este escenario el {{ percent }}% de las veces; si no, sigue con el siguiente.
      </div>
    </div>

    <div>
      <VSwitch v-model="useResource" label="Leer o guardar un recurso persistente" color="primary" hide-details />
      <div v-if="useResource && res" class="d-flex flex-column gap-3 mt-2">
        <div class="d-flex flex-wrap gap-3">
          <FTextField v-model="res.collection" label="Colección" placeholder="documents" class="mono" style="min-width: 160px" />
          <FSelect v-model="res.operation" :items="operations" label="Operación" style="min-width: 180px" />
          <FTextField
            v-if="res.operation !== 'list'"
            v-model="res.recordId"
            label="Id del registro"
            :placeholder="needsId ? '{{ request.params.id }}' : '{{ uuid() }} (vacío = generado)'"
            class="mono flex-1-1"
          />
        </div>
        <div class="text-body-small text-medium-emphasis">
          {{ opHint }} En la respuesta usa <code class="inline" v-text="'{{ resource }}'" /> (registro) o
          <code class="inline" v-text="'{{ resources }}'" /> (lista).
        </div>
        <JsonEditor
          v-if="takesData"
          v-model="res.data"
          label="Datos (los textos son plantillas)"
          hint='Ej.: {"status": "pending", "name": "{{ request.body.name }}"}'
          min-height="100px"
        />
        <JsonEditor
          v-if="canMiss"
          v-model="res.onMissing"
          label="Respuesta si no existe (opcional)"
          hint='Por defecto 404 {"error": "not_found"}. Formato: {"status": 404, "body": {"type": "json", "content": {...}}}'
          min-height="70px"
          empty-as="null"
        />
      </div>
    </div>
  </div>
</template>
