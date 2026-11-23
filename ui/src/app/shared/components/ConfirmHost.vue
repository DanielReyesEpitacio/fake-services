<script setup lang="ts">
import { confirmState, resolveConfirm } from '@/app/shared/state/confirm.ts'
</script>

<template>
  <VDialog :model-value="confirmState.open" max-width="440" @update:model-value="(v) => !v && resolveConfirm(false)">
    <VCard :title="confirmState.title">
      <VCardText>{{ confirmState.text }}</VCardText>
      <VCardActions>
        <VSpacer />
        <VBtn variant="text" @click="resolveConfirm(false)">Cancelar</VBtn>
        <VBtn :color="confirmState.danger ? 'error' : 'primary'" @click="resolveConfirm(true)">
          {{ confirmState.confirmText }}
        </VBtn>
      </VCardActions>
    </VCard>
  </VDialog>
</template>
