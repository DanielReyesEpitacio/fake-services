<script setup lang="ts">
import { X } from 'lucide-vue-next'
defineProps<{ title: string; wide?: boolean }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-40 flex items-start justify-center overflow-y-auto bg-ink/40 p-6 pt-20" @mousedown.self="emit('close')">
      <div class="card w-full shadow-xl" :class="wide ? 'max-w-3xl' : 'max-w-lg'" role="dialog" :aria-label="title">
        <div class="card-header">
          <h2 class="card-title text-base">{{ title }}</h2>
          <button class="btn btn-ghost btn-sm" aria-label="Cerrar" @click="emit('close')"><X class="size-4" /></button>
        </div>
        <div class="p-4"><slot /></div>
        <div v-if="$slots.footer" class="flex justify-end gap-2 border-t border-zinc-100 px-4 py-3">
          <slot name="footer" />
        </div>
      </div>
    </div>
  </Teleport>
</template>
