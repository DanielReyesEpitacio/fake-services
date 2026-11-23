<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { Outcome } from '../lib/types'
defineProps<{ outcome: Outcome }>()
const { t } = useI18n()
</script>

<template>
  <span class="badge" :class="outcome === 'matched' || outcome === 'preflight' ? 'bg-zinc-100 text-zinc-600' : 'bg-rose-50 text-rose-700'">
    {{ t(`outcome.${outcome}`) }}
  </span>
</template>
