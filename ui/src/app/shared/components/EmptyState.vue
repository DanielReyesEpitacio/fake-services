<script setup lang="ts">
defineProps<{ title: string; text?: string; icon?: string }>()
</script>

<template>
  <VEmptyState :icon="icon ?? 'mdi-ghost-outline'" :title="title" :text="text" size="56">
    <template v-if="$slots.default" #actions><slot /></template>
  </VEmptyState>
</template>
