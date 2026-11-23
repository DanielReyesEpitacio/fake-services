<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { Outcome } from '@/infrastructure/api/types.ts'
defineProps<{ outcome: Outcome }>()
const { t } = useI18n()
</script>

<template>
  <VChip
    size="small"
    variant="tonal"
    :color="outcome === 'matched' || outcome === 'preflight' ? undefined : 'error'"
  >
    {{ t(`outcome.${outcome}`) }}
  </VChip>
</template>
