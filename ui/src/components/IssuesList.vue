<script setup lang="ts">
import { CircleAlert } from 'lucide-vue-next'
import type { Issue } from '../lib/types'
defineProps<{ issues: Issue[] }>()
</script>

<template>
  <div v-if="issues.length" class="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">
    <div class="mb-1 flex items-center gap-1.5 font-medium"><CircleAlert class="size-4" /> Revisa estos campos</div>
    <ul class="space-y-0.5">
      <li v-for="(i, n) in issues" :key="n"><code class="text-xs">{{ i.field }}</code>: {{ i.message }}</li>
    </ul>
  </div>
</template>
