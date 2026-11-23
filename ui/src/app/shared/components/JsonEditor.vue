<script setup lang="ts">
// Editor JSON con validación: solo actualiza el modelo con JSON válido y
// registra los errores para que la página bloquee el guardado.
import { inject, onBeforeUnmount, ref, watch } from 'vue'
import CodeEditor from './CodeEditor.vue'

const props = withDefaults(
  defineProps<{ label?: string; minHeight?: string; hint?: string; emptyAs?: 'undefined' | 'object' | 'null' }>(),
  { label: undefined, minHeight: '80px', hint: undefined, emptyAs: 'undefined' },
)
const model = defineModel<unknown>()
const invalid = inject<Set<symbol>>('invalidJson', new Set())
const uid = Symbol('json-editor')

const stringify = (v: unknown) => (v === undefined ? '' : JSON.stringify(v, null, 2))
const text = ref(stringify(model.value))
const error = ref('')

watch(model, (v) => {
  // Cambios externos (descartar, recargar): se reflejan si difieren.
  try {
    const current = text.value.trim() ? JSON.parse(text.value) : undefined
    if (JSON.stringify(current) !== JSON.stringify(v)) text.value = stringify(v)
  } catch {
    text.value = stringify(v)
  }
})

watch(text, (t) => {
  if (!t.trim()) {
    error.value = ''
    invalid.delete(uid)
    model.value = props.emptyAs === 'object' ? {} : props.emptyAs === 'null' ? null : undefined
    return
  }
  try {
    const parsed = JSON.parse(t)
    error.value = ''
    invalid.delete(uid)
    if (JSON.stringify(parsed) !== JSON.stringify(model.value)) model.value = parsed
  } catch (e) {
    error.value = (e as Error).message
    invalid.add(uid)
  }
})

onBeforeUnmount(() => invalid.delete(uid))
</script>

<template>
  <div>
    <CodeEditor v-model="text" language="json" :min-height="minHeight" :label="label" />
    <div v-if="error" class="text-body-small text-error mt-1">JSON inválido: {{ error }}</div>
    <div v-else-if="hint" class="text-body-small text-medium-emphasis mt-1">{{ hint }}</div>
  </div>
</template>
