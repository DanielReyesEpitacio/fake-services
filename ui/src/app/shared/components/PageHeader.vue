<script setup lang="ts">
defineProps<{ title: string; subtitle?: string }>()
</script>

<template>
  <div class="d-flex flex-wrap align-end justify-space-between gap-3 mb-5">
    <div class="min-w-0">
      <slot name="overline" />
      <h1 class="text-headline-small font-weight-bold">
        <slot name="title">{{ title }}</slot>
      </h1>
      <div v-if="subtitle || $slots.subtitle" class="text-body-medium text-medium-emphasis mt-1">
        <slot name="subtitle">{{ subtitle }}</slot>
      </div>
    </div>
    <div v-if="$slots.actions" class="d-flex flex-wrap align-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
