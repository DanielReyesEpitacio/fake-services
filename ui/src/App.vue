<script setup lang="ts">
import { onMounted } from 'vue'
import ShellLayout from '@/app/shared/layouts/shell/ShellLayout.vue'
import ConfirmHost from '@/app/shared/components/ConfirmHost.vue'
import { startLive, toastQueue } from '@/app/shared/state/live.ts'

onMounted(startLive)
</script>

<template>
  <VApp>
    <ShellLayout />
    <ConfirmHost />
    <VSnackbarQueue v-model="toastQueue" location="bottom end" />
  </VApp>
</template>
