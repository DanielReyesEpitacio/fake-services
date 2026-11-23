<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ method: string }>()
const cls = computed(() => {
  switch (props.method.toUpperCase()) {
    case 'GET': return 'bg-sky-100 text-sky-800'
    case 'POST': return 'bg-lime-100 text-lime-800'
    case 'PUT': return 'bg-amber-100 text-amber-800'
    case 'PATCH': return 'bg-violet-100 text-violet-800'
    case 'DELETE': return 'bg-rose-100 text-rose-800'
    default: return 'bg-zinc-100 text-zinc-700'
  }
})
</script>

<template>
  <span class="badge w-16 justify-center font-mono" :class="cls">{{ method === '*' ? 'ANY' : method }}</span>
</template>
