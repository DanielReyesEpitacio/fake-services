<script setup lang="ts">
import { ref, watch } from 'vue'

withDefaults(
  defineProps<{ keyPlaceholder?: string; valuePlaceholder?: string; addLabel?: string }>(),
  { keyPlaceholder: 'nombre', valuePlaceholder: 'valor', addLabel: 'Agregar' },
)
const model = defineModel<Record<string, string> | undefined>()

// Se edita como lista para permitir claves vacías o repetidas mientras se escribe.
const rows = ref<{ k: string; v: string }[]>([])
const load = () => (rows.value = Object.entries(model.value ?? {}).map(([k, v]) => ({ k, v })))
load()

let internal = false
watch(model, () => {
  if (!internal) load()
  internal = false
})

function emit() {
  const out: Record<string, string> = {}
  for (const r of rows.value) if (r.k.trim()) out[r.k.trim()] = r.v
  internal = true
  model.value = out
}

function remove(i: number) {
  rows.value.splice(i, 1)
  emit()
}
</script>

<template>
  <div class="d-flex flex-column gap-2">
    <div v-for="(r, i) in rows" :key="i" class="d-flex align-center gap-2">
      <FTextField v-model="r.k" :placeholder="keyPlaceholder" class="mono" style="max-width: 40%" hide-details @update:model-value="emit" />
      <FTextField v-model="r.v" :placeholder="valuePlaceholder" class="mono" hide-details @update:model-value="emit" />
      <VBtn icon="mdi-delete-outline" variant="text" size="small" aria-label="Quitar" @click="remove(i)" />
    </div>
    <div>
      <VBtn variant="text" size="small" prepend-icon="mdi-plus" color="primary" @click="rows.push({ k: '', v: '' })">
        {{ addLabel }}
      </VBtn>
    </div>
  </div>
</template>
