<script setup lang="ts">
import type { ParamSpec } from '@/infrastructure/api/types.ts'

defineProps<{ namePlaceholder?: string; addLabel?: string }>()
const model = defineModel<ParamSpec[]>({ required: true })
</script>

<template>
  <div>
    <VTable v-if="model.length" class="mb-2">
      <thead>
        <tr>
          <th style="width: 24%">Nombre</th>
          <th style="width: 110px">Obligatorio</th>
          <th style="width: 26%">Patrón (regex)</th>
          <th>Descripción</th>
          <th style="width: 48px"></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(p, i) in model" :key="i">
          <td><FTextField v-model="p.name" class="mono" :placeholder="namePlaceholder ?? 'nombre'" hide-details /></td>
          <td><VCheckbox v-model="p.required" hide-details /></td>
          <td>
            <FTextField
              :model-value="p.pattern ?? ''"
              class="mono"
              placeholder="opcional"
              hide-details
              @update:model-value="p.pattern = $event || null"
            />
          </td>
          <td><FTextField v-model="p.description" hide-details /></td>
          <td><VBtn icon="mdi-delete-outline" variant="text" size="small" aria-label="Quitar" @click="model.splice(i, 1)" /></td>
        </tr>
      </tbody>
    </VTable>
    <VBtn variant="text" size="small" color="primary" prepend-icon="mdi-plus" @click="model.push({ name: '', required: true })">
      {{ addLabel ?? 'Agregar' }}
    </VBtn>
  </div>
</template>
