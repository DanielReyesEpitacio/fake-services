<script setup lang="ts">
defineProps<{ title: string; text?: string }>()
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-2 px-6 py-12 text-center">
    <div class="text-sm font-medium text-zinc-800">{{ title }}</div>
    <p v-if="text" class="max-w-md text-sm text-zinc-500">{{ text }}</p>
    <div class="mt-2"><slot /></div>
  </div>
</template>
