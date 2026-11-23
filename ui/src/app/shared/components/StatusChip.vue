<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ status: number }>()
const color = computed(() => {
  const s = props.status
  if (s < 300) return 'success'
  if (s < 400) return 'info'
  if (s < 500) return 'warning'
  return 'error'
})
</script>

<template>
  <VChip :color="color" size="small" label variant="tonal" class="mono font-weight-bold">{{ status }}</VChip>
</template>
