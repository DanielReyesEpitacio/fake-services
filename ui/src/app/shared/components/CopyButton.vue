<script setup lang="ts">
import { copyText } from '@/app/shared/state/live.ts'
defineProps<{ text: string }>()
</script>

<template>
  <VBtn
    icon="mdi-content-copy"
    variant="text"
    size="x-small"
    density="comfortable"
    :height="24"
    :width="24"
    title="Copiar"
    aria-label="Copiar"
    @click.stop="copyText(text)"
  />
</template>
