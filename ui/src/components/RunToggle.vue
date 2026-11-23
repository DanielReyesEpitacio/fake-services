<script setup lang="ts">
import { Play, Square } from 'lucide-vue-next'
import { toggleService } from '../lib/store'
defineProps<{ id: string; running: boolean }>()
</script>

<template>
  <button
    type="button"
    class="btn btn-sm"
    :class="running ? '' : 'btn-primary'"
    :title="running ? 'Detener servicio' : 'Iniciar servicio'"
    @click.stop="toggleService(id, running)"
  >
    <Square v-if="running" class="size-3" />
    <Play v-else class="size-3" />
    {{ running ? 'Detener' : 'Iniciar' }}
  </button>
</template>
