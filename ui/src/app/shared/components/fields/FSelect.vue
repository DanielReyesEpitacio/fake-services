<script setup lang="ts">
// VSelect con la etiqueta fuera del campo: los look and feels usan campos de una
// sola línea, donde la etiqueta interna desaparece al tener valor.
import { computed, useAttrs, useId } from 'vue'

defineOptions({ inheritAttrs: false })
defineProps<{ label?: string }>()

const attrs = useAttrs()
const autoId = useId()
const fieldId = computed(() => (attrs.id as string | undefined) ?? autoId)
const outer = computed(() => ({ class: attrs.class, style: attrs.style }))
const inner = computed(() => {
  const { class: _c, style: _s, ...rest } = attrs
  return { ...rest, id: fieldId.value }
})
</script>

<template>
  <div class="f-field" v-bind="outer">
    <label v-if="label" class="f-field__label" :for="fieldId">{{ label }}</label>
    <VSelect v-bind="inner">
      <template v-for="(_, name) in $slots" #[name]="scope"><slot :name="name" v-bind="scope ?? {}" /></template>
    </VSelect>
  </div>
</template>
