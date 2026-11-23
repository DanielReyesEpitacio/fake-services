<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { Plus, Trash2 } from 'lucide-vue-next'
import type { Operator, Rule, RuleSource } from '../lib/types'

const model = defineModel<Rule[]>({ required: true })
const mode = defineModel<'all' | 'any'>('mode', { default: 'all' })
const { t } = useI18n()

const sources: RuleSource[] = ['header', 'query', 'path', 'body', 'method', 'variable']
const operators: Operator[] = [
  'equals', 'notEquals', 'contains', 'notContains', 'exists', 'notExists', 'greaterThan', 'lessThan', 'regex',
]

const keyPlaceholder: Record<RuleSource, string> = {
  header: 'authorization',
  query: 'page',
  path: 'id',
  body: 'user.email (vacío = body completo)',
  method: '—',
  variable: 'MODO',
}

function needsValue(op: Operator) {
  return op !== 'exists' && op !== 'notExists'
}

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

function add() {
  model.value.push({ source: 'body', key: '', operator: 'equals', value: '' })
}

function onOperator(r: Rule) {
  if (!needsValue(r.operator)) delete r.value
  else if (r.value === undefined) r.value = ''
}
</script>

<template>
  <div class="space-y-2">
    <div v-if="model.length > 1" class="flex items-center gap-2 text-xs text-zinc-600">
      Aplica si se cumplen
      <select v-model="mode" class="input w-auto py-0.5 text-xs">
        <option value="all">todas las reglas</option>
        <option value="any">al menos una regla</option>
      </select>
    </div>
    <div v-for="(r, i) in model" :key="i" class="flex flex-wrap items-center gap-1.5">
      <select v-model="r.source" class="input w-40">
        <option v-for="s in sources" :key="s" :value="s">{{ t(`source.${s}`) }}</option>
      </select>
      <input v-if="r.source !== 'method'" v-model="r.key" class="input-mono w-56 flex-1" :placeholder="keyPlaceholder[r.source]" />
      <select v-model="r.operator" class="input w-40" @change="onOperator(r)">
        <option v-for="o in operators" :key="o" :value="o">{{ t(`operator.${o}`) }}</option>
      </select>
      <input
        v-if="needsValue(r.operator)"
        :value="valueText(r.value)"
        class="input-mono w-48 flex-1"
        placeholder="valor"
        @input="r.value = parseValue(($event.target as HTMLInputElement).value)"
      />
      <button type="button" class="btn btn-ghost btn-sm text-zinc-400 hover:text-rose-600" aria-label="Quitar regla" @click="model.splice(i, 1)">
        <Trash2 class="size-3.5" />
      </button>
    </div>
    <p v-if="!model.length" class="text-xs text-zinc-500">
      Sin reglas: este escenario responde siempre que ninguno anterior coincida.
    </p>
    <button type="button" class="btn btn-ghost btn-sm text-brand-700" @click="add"><Plus class="size-3.5" /> Agregar regla</button>
  </div>
</template>
