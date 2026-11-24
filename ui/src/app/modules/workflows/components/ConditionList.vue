<script setup lang="ts">
// Lista editable de condiciones (ruta, operador, valor) de un paso.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Operator, WorkflowCondition } from '@/infrastructure/api/types.ts'

const list = defineModel<WorkflowCondition[]>({ required: true })
const { t } = useI18n()

const operators = computed(() =>
  (
    ['equals', 'notEquals', 'contains', 'notContains', 'exists', 'notExists', 'greaterThan', 'lessThan', 'regex'] as Operator[]
  ).map((value) => ({ value, title: t(`operator.${value}`) })),
)
const needsValue = (op: Operator) => op !== 'exists' && op !== 'notExists'

function valueText(v: unknown): string {
  if (v === undefined || v === null) return ''
  return typeof v === 'string' ? v : JSON.stringify(v)
}

function parseValue(s: string): unknown {
  const trimmed = s.trim()
  if (trimmed === '') return ''
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed)
  if (trimmed === 'true' || trimmed === 'false') return trimmed === 'true'
  if (/^[[{]/.test(trimmed)) {
    try {
      return JSON.parse(trimmed)
    } catch {
      /* texto */
    }
  }
  return s
}
</script>

<template>
  <div class="d-flex flex-column gap-2">
    <div v-for="(c, i) in list" :key="i" class="d-flex flex-wrap align-center gap-2">
      <FTextField v-model="c.path" class="mono" placeholder="request.body.monto" hide-details style="min-width: 200px" />
      <FSelect v-model="c.operator" :items="operators" hide-details style="max-width: 180px" />
      <FTextField
        v-if="needsValue(c.operator)"
        :model-value="valueText(c.value)"
        class="mono"
        placeholder="valor"
        hide-details
        style="min-width: 120px"
        @update:model-value="c.value = parseValue($event)"
      />
      <VBtn icon="mdi-delete-outline" variant="text" size="small" aria-label="Quitar condición" @click="list.splice(i, 1)" />
    </div>
    <div>
      <VBtn variant="text" size="small" color="primary" prepend-icon="mdi-plus" @click="list.push({ path: '', operator: 'equals', value: '' })">
        Agregar condición
      </VBtn>
    </div>
  </div>
</template>
