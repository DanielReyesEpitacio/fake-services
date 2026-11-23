<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ status: number }>()
const cls = computed(() => {
  const s = props.status
  if (s < 300) return 'text-lime-700 bg-lime-50'
  if (s < 400) return 'text-sky-700 bg-sky-50'
  if (s < 500) return 'text-amber-700 bg-amber-50'
  return 'text-rose-700 bg-rose-50'
})
</script>

<template>
  <span class="badge font-mono" :class="cls">{{ status }}</span>
</template>
