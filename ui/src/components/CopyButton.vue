<script setup lang="ts">
import { Copy } from 'lucide-vue-next'
import { copyText } from '../lib/store'
defineProps<{ text: string }>()
</script>

<template>
  <button type="button" title="Copiar" aria-label="Copiar" class="shrink-0 transition" @click.stop="copyText(text)">
    <Copy class="size-3.5" />
  </button>
</template>
