<script setup lang="ts">
import { toggleService } from '@/app/shared/state/live.ts'
defineProps<{ id: string; running: boolean }>()
</script>

<template>
  <VBtn
    size="small"
    :color="running ? undefined : 'primary'"
    :variant="running ? 'tonal' : undefined"
    :prepend-icon="running ? 'mdi-stop' : 'mdi-play'"
    @click.stop="toggleService(id, running)"
  >
    {{ running ? 'Detener' : 'Iniciar' }}
  </VBtn>
</template>
