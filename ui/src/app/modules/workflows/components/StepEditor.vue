<script setup lang="ts">
import { computed } from 'vue'
import JsonEditor from '@/app/shared/components/JsonEditor.vue'
import KeyValueEditor from '@/app/shared/components/KeyValueEditor.vue'
import CodeEditor from '@/app/shared/components/CodeEditor.vue'
import ConditionList from './ConditionList.vue'
import type { Step, StepBody } from '@/infrastructure/api/types.ts'

const props = defineProps<{
  /** Ids de los demás pasos (para saltos). */
  stepIds: string[]
  /** Workflows del servicio (para ejecutar otro workflow). */
  workflows: { id: string; name: string }[]
  workflowId: string
  /** Panel angosto (inspector del diagrama): un campo por fila. */
  compact?: boolean
}>()
const step = defineModel<Step>({ required: true })

const jumpItems = computed(() => [
  { title: 'Siguiente paso', value: null },
  { title: 'Finalizar (end)', value: 'end' },
  ...props.stepIds.filter((id) => id && id !== step.value.id).map((id) => ({ title: `Ir a «${id}»`, value: id })),
])

const errorItems = computed(() => [
  { title: 'Fallar la ejecución', value: null },
  { title: 'Terminar como completada (end)', value: 'end' },
  ...props.stepIds.filter((id) => id && id !== step.value.id).map((id) => ({ title: `Ir a «${id}»`, value: id })),
])

const third = computed(() => (props.compact ? 12 : 4))
const half = computed(() => (props.compact ? 12 : 6))

const methods = ['POST', 'GET', 'PUT', 'PATCH', 'DELETE']
const otherWorkflows = computed(() =>
  props.workflows.filter((w) => w.id !== props.workflowId).map((w) => ({ title: w.name, value: w.id })),
)

const resourceOps = [
  { value: 'create', title: 'Crear' },
  { value: 'get', title: 'Leer' },
  { value: 'update', title: 'Actualizar' },
  { value: 'upsert', title: 'Crear o actualizar' },
  { value: 'delete', title: 'Eliminar' },
  { value: 'list', title: 'Listar' },
]

const num = (v: unknown) => {
  const n = Number(v)
  return Number.isFinite(n) && n > 0 ? Math.round(n) : 0
}

// -- HTTP -------------------------------------------------------------------

const bodyKinds: StepBody['type'][] = ['json', 'text', 'form', 'none']
const bodyLabels: Record<StepBody['type'], string> = { json: 'JSON', text: 'Texto', form: 'Formulario', none: 'Sin body' }

const http = computed(() => (step.value.type === 'http' ? step.value : null))

const bodyKind = computed({
  get: () => http.value?.body?.type ?? 'none',
  set: (k: StepBody['type']) => {
    if (!http.value) return
    if (k === 'json') http.value.body = { type: 'json', content: {} }
    else if (k === 'text') http.value.body = { type: 'text', content: '' }
    else if (k === 'form') http.value.body = { type: 'form', fields: {} }
    else http.value.body = { type: 'none' }
  },
})

const expectedText = computed({
  get: () => http.value?.expectedStatus?.join(', ') ?? '',
  set: (v: string) => {
    if (!http.value) return
    const list = v.split(',').map((s) => s.trim()).filter(Boolean)
    http.value.expectedStatus = list.length ? list : undefined
  },
})

function setOnError(v: string | null) {
  step.value.onError = v ?? undefined
  if (v && step.value.type === 'http') step.value.continueOnError = false
}

function addCase() {
  if (step.value.type !== 'switch') return
  step.value.cases.push({ label: `Caso ${step.value.cases.length + 1}`, conditions: [{ path: '', operator: 'equals', value: '' }] })
}

function moveCase(i: number, d: -1 | 1) {
  if (step.value.type !== 'switch') return
  const list = step.value.cases
  if (i + d < 0 || i + d >= list.length) return
  ;[list[i], list[i + d]] = [list[i + d]!, list[i]!]
}

function ensureRetry() {
  if (http.value && !http.value.retry) http.value.retry = { maxAttempts: 0, backoffMs: 1000, multiplier: 2 }
  return http.value!.retry!
}
</script>

<template>
  <div class="d-flex flex-column gap-4">
    <VRow dense>
      <VCol cols="12" :md="third"><FTextField v-model="step.id" label="Identificador" class="mono" hint="Resultados en steps.<id>" persistent-hint /></VCol>
      <VCol cols="12" :md="third"><FTextField v-model="step.name" label="Nombre (opcional)" /></VCol>
      <VCol v-if="!['condition', 'switch', 'end', 'fail'].includes(step.type)" cols="12" :md="third">
        <FSelect
          :model-value="step.next ?? null"
          :items="jumpItems"
          label="Después"
          @update:model-value="step.next = $event ?? undefined"
        />
      </VCol>
      <VCol v-if="step.type !== 'end' && step.type !== 'fail'" cols="12" :md="third">
        <FSelect
          :model-value="step.onError ?? null"
          :items="errorItems"
          label="Si falla"
          hint="Salida «Fallo»: el error queda en steps.<id>.error."
          persistent-hint
          @update:model-value="setOnError($event)"
        />
      </VCol>
    </VRow>

    <!-- Esperar -->
    <template v-if="step.type === 'wait'">
      <div class="d-flex align-end gap-3">
        <FTextField
          :model-value="step.durationMs"
          label="Duración (ms)"
          type="number"
          min="0"
          class="mono"
          style="max-width: 220px"
          @update:model-value="step.type === 'wait' && (step.durationMs = num($event))"
        />
        <div class="d-flex gap-1 pb-1">
          <VChip v-for="ms in [1000, 5000, 15000, 60000]" :key="ms" size="small" @click="step.type === 'wait' && (step.durationMs = ms)">
            {{ ms >= 60000 ? `${ms / 60000} min` : `${ms / 1000} s` }}
          </VChip>
        </div>
      </div>
    </template>

    <!-- HTTP -->
    <template v-else-if="step.type === 'http'">
      <div class="d-flex gap-2" :class="{ 'flex-wrap': compact }">
        <FSelect v-model="step.method" :items="methods" label="Método" class="mono" style="max-width: 140px" />
        <FTextField
          v-model="step.url"
          label="URL"
          class="mono"
          placeholder="{{ request.body.callback_url }}"
          hint="Admite plantillas: datos de la solicitud original, de la respuesta y de pasos anteriores."
          persistent-hint
        />
      </div>
      <VRow>
        <VCol cols="12" :md="half">
          <div class="f-field__label">Headers</div>
          <KeyValueEditor v-model="step.headers" key-placeholder="x-signature" value-placeholder="{{ vars.SECRET }}" add-label="Agregar header" />
        </VCol>
        <VCol cols="12" :md="half">
          <div class="f-field__label">Query</div>
          <KeyValueEditor v-model="step.query" key-placeholder="event" value-placeholder="signed" add-label="Agregar parámetro" />
        </VCol>
      </VRow>
      <div>
        <div class="d-flex align-center justify-space-between mb-2">
          <span class="f-field__label mb-0">Body</span>
          <VChipGroup v-model="bodyKind" mandatory selected-class="text-primary">
            <VChip v-for="k in bodyKinds" :key="k" :value="k" size="small" filter>{{ bodyLabels[k] }}</VChip>
          </VChipGroup>
        </div>
        <JsonEditor
          v-if="step.body?.type === 'json'"
          v-model="step.body.content"
          empty-as="null"
          hint="Si un texto es solo {{ expr }} se conserva el tipo."
        />
        <template v-else-if="step.body?.type === 'text'">
          <CodeEditor v-model="step.body.content" language="text" min-height="80px" />
          <FTextField
            :model-value="step.body.contentType ?? ''"
            label="Content-Type"
            placeholder="text/plain"
            class="mono mt-2"
            @update:model-value="step.body?.type === 'text' && (step.body.contentType = $event || null)"
          />
        </template>
        <KeyValueEditor v-else-if="step.body?.type === 'form'" v-model="step.body.fields" key-placeholder="status" value-placeholder="signed" add-label="Agregar campo" />
      </div>
      <VRow dense>
        <VCol cols="6" md="3">
          <FTextField :model-value="step.timeoutMs ?? 10000" label="Tiempo límite (ms)" type="number" @update:model-value="step.type === 'http' && (step.timeoutMs = num($event) || 10000)" />
        </VCol>
        <VCol cols="6" md="3">
          <FTextField :model-value="step.delayMs ?? 0" label="Retraso previo (ms)" type="number" @update:model-value="step.type === 'http' && (step.delayMs = num($event) || undefined)" />
        </VCol>
        <VCol cols="12" md="6">
          <FTextField v-model="expectedText" label="Códigos esperados" placeholder="2xx" class="mono" hint="Ej.: 2xx, 409, 200-299. Vacío = 2xx." persistent-hint />
        </VCol>
      </VRow>
      <VRow dense>
        <VCol cols="4">
          <FTextField :model-value="step.retry?.maxAttempts ?? 0" label="Reintentos" type="number" min="0" max="10" @update:model-value="ensureRetry().maxAttempts = num($event)" />
        </VCol>
        <VCol cols="4">
          <FTextField :model-value="step.retry?.backoffMs ?? 1000" label="Espera entre reintentos (ms)" type="number" @update:model-value="ensureRetry().backoffMs = num($event) || 1000" />
        </VCol>
        <VCol cols="4">
          <FTextField :model-value="step.retry?.multiplier ?? 2" label="Multiplicador" type="number" step="0.5" min="1" @update:model-value="ensureRetry().multiplier = Number($event) || 1" />
        </VCol>
      </VRow>
      <VSwitch
        v-if="!step.onError"
        v-model="step.continueOnError"
        label="Si falla tras los reintentos, continuar con el siguiente paso"
        color="primary"
        hide-details
      />
    </template>

    <!-- Emitir evento -->
    <template v-else-if="step.type === 'emit'">
      <FTextField v-model="step.event" label="Nombre del evento" placeholder="documento.firmado" class="mono" />
      <JsonEditor v-model="step.data" label="Datos (opcional)" />
    </template>

    <!-- Variables -->
    <template v-else-if="step.type === 'setVariables'">
      <JsonEditor
        v-model="step.values"
        empty-as="object"
        label="Variables"
        min-height="100px"
        hint='Objeto JSON, p. ej. {"intentos": "{{ vars.intentos + 1 }}"}. Quedan en vars.'
      />
    </template>

    <!-- Condición -->
    <template v-else-if="step.type === 'condition'">
      <ConditionList v-model="step.conditions" />
      <div class="text-body-small text-medium-emphasis">
        Rutas disponibles: <code class="inline">request.*</code>, <code class="inline">response.*</code>,
        <code class="inline">input.*</code>, <code class="inline">vars.*</code>, <code class="inline">steps.&lt;id&gt;.*</code>,
        <code class="inline">execution.status</code>.
      </div>
      <VRow dense>
        <VCol cols="12" :md="third">
          <FSelect
            :model-value="step.mode ?? 'all'"
            :items="[{ title: 'Todas se cumplen', value: 'all' }, { title: 'Al menos una', value: 'any' }]"
            label="Modo"
            @update:model-value="step.type === 'condition' && (step.mode = $event)"
          />
        </VCol>
        <VCol :cols="compact ? 12 : 6" :md="third">
          <FSelect :model-value="step.onTrue ?? null" :items="jumpItems" label="Si se cumple" @update:model-value="step.type === 'condition' && (step.onTrue = $event ?? undefined)" />
        </VCol>
        <VCol :cols="compact ? 12 : 6" :md="third">
          <FSelect :model-value="step.onFalse ?? null" :items="jumpItems" label="Si no se cumple" @update:model-value="step.type === 'condition' && (step.onFalse = $event ?? undefined)" />
        </VCol>
      </VRow>
    </template>

    <!-- Dividir según -->
    <template v-else-if="step.type === 'switch'">
      <div class="text-body-small text-medium-emphasis">
        Los casos se evalúan en orden; gana el primero que se cumple. Cada caso es una salida en el diagrama.
      </div>
      <VCard v-for="(c, i) in step.cases" :key="i" variant="outlined" class="pa-3">
        <div class="d-flex align-center gap-2 mb-2">
          <FTextField v-model="c.label" :placeholder="`Caso ${i + 1}`" label="Nombre de la salida" hide-details class="flex-1-1" />
          <VBtn icon="mdi-arrow-up" variant="text" size="small" title="Subir" :disabled="i === 0" @click="moveCase(i, -1)" />
          <VBtn icon="mdi-delete-outline" variant="text" size="small" title="Quitar caso" @click="step.type === 'switch' && step.cases.splice(i, 1)" />
        </div>
        <ConditionList v-model="c.conditions" />
        <VRow dense class="mt-1">
          <VCol cols="12" :md="half">
            <FSelect
              :model-value="c.mode ?? 'all'"
              :items="[{ title: 'Todas se cumplen', value: 'all' }, { title: 'Al menos una', value: 'any' }]"
              label="Modo"
              @update:model-value="c.mode = $event"
            />
          </VCol>
          <VCol cols="12" :md="half">
            <FSelect :model-value="c.next ?? null" :items="jumpItems" label="Ir a" @update:model-value="c.next = $event ?? undefined" />
          </VCol>
        </VRow>
      </VCard>
      <div class="d-flex flex-wrap align-end gap-3">
        <VBtn variant="tonal" size="small" prepend-icon="mdi-plus" @click="addCase">Agregar caso</VBtn>
        <VSpacer />
        <FSelect
          :model-value="step.otherwise ?? null"
          :items="jumpItems"
          label="Si ningún caso se cumple"
          style="min-width: 220px"
          @update:model-value="step.type === 'switch' && (step.otherwise = $event ?? undefined)"
        />
      </div>
    </template>

    <!-- Ejecutar otro workflow -->
    <template v-else-if="step.type === 'runWorkflow'">
      <FSelect v-model="step.workflowId" :items="otherWorkflows" label="Workflow" no-data-text="No hay otros workflows en este servicio" />
      <JsonEditor v-model="step.input" label="Entrada (opcional)" hint="Objeto JSON disponible como input.* en el workflow hijo." />
      <VSwitch v-model="step.wait" label="Esperar a que termine antes de continuar" color="primary" hide-details />
    </template>

    <!-- Recurso persistente -->
    <template v-else-if="step.type === 'resource'">
      <div class="d-flex flex-wrap gap-3">
        <FTextField v-model="step.collection" label="Colección" placeholder="documents" class="mono" style="min-width: 160px" />
        <FSelect v-model="step.operation" :items="resourceOps" label="Operación" style="min-width: 180px" />
        <FTextField
          v-if="step.operation !== 'list'"
          v-model="step.recordId"
          label="Id del registro"
          placeholder="{{ response.body.id }}"
          class="mono flex-1-1"
        />
      </div>
      <JsonEditor
        v-if="['create', 'update', 'upsert'].includes(step.operation)"
        v-model="step.data"
        label="Datos (los textos son plantillas)"
        hint="En update/upsert se combinan con el registro existente; null borra un campo."
      />
      <VSwitch v-model="step.failIfMissing" label="Fallar si el registro no existe" color="primary" hide-details />
      <div class="text-body-small text-medium-emphasis">
        El resultado queda en <code class="inline" v-text="'steps.' + step.id + '.resource'" /> (o
        <code class="inline">resources</code> al listar) y <code class="inline" v-text="'steps.' + step.id + '.found'" />.
      </div>
    </template>

    <!-- Simular error -->
    <template v-else-if="step.type === 'fail'">
      <FTextField v-model="step.message" label="Mensaje de error" placeholder="El documento {{ input.id }} expiró" />
    </template>

    <div v-else class="text-body-small text-medium-emphasis">La ejecución termina aquí como completada.</div>
  </div>
</template>
