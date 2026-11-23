<script setup lang="ts">
const model = defineModel<boolean>({ required: true })
defineProps<{ label?: string; disabled?: boolean }>()
</script>

<template>
  <button
    type="button"
    role="switch"
    :aria-checked="model"
    :aria-label="label"
    :disabled="disabled"
    class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition disabled:opacity-50"
    :class="model ? 'bg-brand-600' : 'bg-zinc-300'"
    @click="model = !model"
  >
    <span class="inline-block size-4 rounded-full bg-white shadow transition" :class="model ? 'translate-x-4.5' : 'translate-x-0.5'" />
  </button>
</template>
