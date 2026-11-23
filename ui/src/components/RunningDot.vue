<script setup lang="ts">
defineProps<{ running: boolean }>()
</script>

<template>
  <span class="relative inline-flex size-2.5" :title="running ? 'En ejecución' : 'Detenido'">
    <span v-if="running" class="absolute inline-flex size-full animate-ping rounded-full bg-live opacity-60" />
    <span class="relative inline-flex size-2.5 rounded-full" :class="running ? 'bg-lime-500' : 'bg-zinc-300'" />
  </span>
</template>
