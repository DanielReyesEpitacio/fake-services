<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ method: string }>()
const color = computed(() => {
  switch (props.method.toUpperCase()) {
    case 'GET': return 'info'
    case 'POST': return 'success'
    case 'PUT': return 'warning'
    case 'PATCH': return 'secondary'
    case 'DELETE': return 'error'
    default: return undefined
  }
})
</script>

<template>
  <VChip :color="color" size="small" label variant="tonal" class="mono font-weight-bold justify-center" style="min-width: 64px">
    {{ method === '*' ? 'ANY' : method }}
  </VChip>
</template>
