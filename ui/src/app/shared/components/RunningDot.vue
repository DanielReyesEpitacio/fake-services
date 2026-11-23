<script setup lang="ts">
defineProps<{ running: boolean }>()
</script>

<template>
  <span class="running-dot" :class="{ on: running }" :title="running ? 'En ejecución' : 'Detenido'" />
</template>

<style scoped>
.running-dot {
  display: inline-block;
  flex-shrink: 0;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: rgba(var(--v-theme-on-surface), 0.25);
}
.running-dot.on {
  background: #84cc16;
  box-shadow: 0 0 0 3px rgba(163, 230, 53, 0.35);
}
</style>
