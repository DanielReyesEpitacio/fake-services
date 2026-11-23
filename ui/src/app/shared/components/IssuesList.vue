<script setup lang="ts">
import type { Issue } from '@/infrastructure/api/types.ts'
defineProps<{ issues: Issue[] }>()
</script>

<template>
  <VAlert v-if="issues.length" type="error" variant="tonal" title="Revisa estos campos" class="mb-3">
    <ul class="mt-1 ps-4">
      <li v-for="(i, n) in issues" :key="n"><code class="inline">{{ i.field }}</code>: {{ i.message }}</li>
    </ul>
  </VAlert>
</template>
