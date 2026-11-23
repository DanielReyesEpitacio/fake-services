<script setup lang="ts">
import { CircleAlert, CircleCheck, X } from 'lucide-vue-next'
import { dismiss, store } from '../lib/store'
</script>

<template>
  <div class="pointer-events-none fixed right-4 bottom-4 z-50 flex w-96 flex-col gap-2">
    <TransitionGroup
      enter-from-class="translate-y-2 opacity-0"
      enter-active-class="transition"
      leave-to-class="opacity-0"
      leave-active-class="transition"
    >
      <div
        v-for="t in store.toasts"
        :key="t.id"
        class="pointer-events-auto flex items-start gap-2 rounded-lg border px-3 py-2.5 text-sm shadow-lg"
        :class="t.kind === 'error' ? 'border-rose-200 bg-rose-50 text-rose-800' : 'border-zinc-200 bg-white text-zinc-800'"
      >
        <CircleAlert v-if="t.kind === 'error'" class="mt-0.5 size-4 shrink-0" />
        <CircleCheck v-else class="mt-0.5 size-4 shrink-0 text-lime-600" />
        <span class="flex-1">{{ t.text }}</span>
        <button class="opacity-60 hover:opacity-100" aria-label="Cerrar" @click="dismiss(t.id)">
          <X class="size-4" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>
