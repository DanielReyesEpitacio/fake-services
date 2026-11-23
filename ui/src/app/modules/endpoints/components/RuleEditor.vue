<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Operator, Rule, RuleSource } from '@/infrastructure/api/types.ts'

const model = defineModel<Rule[]>({ required: true })
const mode = defineModel<'all' | 'any'>('mode', { default: 'all' })
const { t } = useI18n()

const sources = computed(() =>
  (['header', 'query', 'path', 'body', 'method', 'variable'] as RuleSource[]).map((value) => ({
    value,
    title: t(`source.${value}`),
  })),
)
const operators = computed(() =>
  (
    ['equals', 'notEquals', 'contains', 'notContains', 'exists', 'notExists', 'greaterThan', 'lessThan', 'regex'] as Operator[]
  ).map((value) => ({ value, title: t(`operator.${value}`) })),
)

const keyPlaceholder: Record<RuleSource, string> = {
  header: 'authorization',
  query: 'page',
  path: 'id',
  body: 'user.email (vacío = body completo)',
  method: '',
  variable: 'MODO',
}

const needsValue = (op: Operator) => op !== 'exists' && op !== 'notExists'

function valueText(v: unknown): string {
  if (v === undefined || v === null) return ''
  return typeof v === 'string' ? v : JSON.stringify(v)
}

/** Números, true/false y JSON se interpretan; lo demás es texto. */
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

function onOperator(r: Rule) {
  if (!needsValue(r.operator)) delete r.value
  else if (r.value === undefined) r.value = ''
}
</script>

<template>
  <div class="d-flex flex-column gap-2">
    <div v-if="model.length > 1" class="d-flex align-center gap-2 text-body-small">
      Aplica si se cumplen
      <VBtnToggle v-model="mode" mandatory density="compact" color="primary" variant="outlined" divided>
        <VBtn value="all" size="small">todas</VBtn>
        <VBtn value="any" size="small">al menos una</VBtn>
      </VBtnToggle>
    </div>
    <div v-for="(r, i) in model" :key="i" class="d-flex flex-wrap align-center gap-2">
      <FSelect v-model="r.source" :items="sources" hide-details style="max-width: 170px" />
      <FTextField
        v-if="r.source !== 'method'"
        v-model="r.key"
        class="mono"
        :placeholder="keyPlaceholder[r.source]"
        hide-details
        style="min-width: 160px"
      />
      <FSelect v-model="r.operator" :items="operators" hide-details style="max-width: 170px" @update:model-value="onOperator(r)" />
      <FTextField
        v-if="needsValue(r.operator)"
        :model-value="valueText(r.value)"
        class="mono"
        placeholder="valor"
        hide-details
        style="min-width: 140px"
        @update:model-value="r.value = parseValue($event)"
      />
      <VBtn icon="mdi-delete-outline" variant="text" size="small" aria-label="Quitar regla" @click="model.splice(i, 1)" />
    </div>
    <div v-if="!model.length" class="text-body-small text-medium-emphasis">
      Sin reglas: este escenario responde siempre que ninguno anterior coincida.
    </div>
    <div>
      <VBtn
        variant="text"
        size="small"
        color="primary"
        prepend-icon="mdi-plus"
        @click="model.push({ source: 'body', key: '', operator: 'equals', value: '' })"
      >
        Agregar regla
      </VBtn>
    </div>
  </div>
</template>
