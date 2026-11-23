<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ExecutionStatus } from '@/infrastructure/api/types.ts'

const props = defineProps<{ status: ExecutionStatus }>()
const { t } = useI18n()
const look = computed(() => {
  switch (props.status) {
    case 'completed': return { color: 'success', icon: 'mdi-check-circle' }
    case 'failed': return { color: 'error', icon: 'mdi-alert-circle' }
    case 'cancelled': return { color: undefined, icon: 'mdi-close-circle' }
    case 'waiting': return { color: 'info', icon: 'mdi-timer-sand' }
    case 'running': return { color: 'primary', icon: 'mdi-play-circle' }
    default: return { color: undefined, icon: 'mdi-circle-outline' }
  }
})
</script>

<template>
  <VChip :color="look.color" :prepend-icon="look.icon" size="small" variant="tonal">{{ t(`executionStatus.${status}`) }}</VChip>
</template>
