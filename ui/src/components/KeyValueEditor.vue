<script setup lang="ts">
import { ref, watch } from 'vue'
import { Plus, Trash2 } from 'lucide-vue-next'

const props = withDefaults(
  defineProps<{ keyPlaceholder?: string; valuePlaceholder?: string; addLabel?: string }>(),
  { keyPlaceholder: 'nombre', valuePlaceholder: 'valor', addLabel: 'Agregar' },
)
const model = defineModel<Record<string, string> | undefined>()

// Se edita como lista para permitir claves vacías o repetidas mientras se escribe.
const rows = ref<{ k: string; v: string }[]>([])

function load() {
  rows.value = Object.entries(model.value ?? {}).map(([k, v]) => ({ k, v }))
}
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

function add() {
  rows.value.push({ k: '', v: '' })
}

function remove(i: number) {
  rows.value.splice(i, 1)
  emit()
}
void props
</script>

<template>
  <div class="space-y-1.5">
    <div v-for="(r, i) in rows" :key="i" class="flex gap-1.5">
      <input v-model="r.k" class="input-mono w-2/5" :placeholder="keyPlaceholder" @input="emit" />
      <input v-model="r.v" class="input-mono flex-1" :placeholder="valuePlaceholder" @input="emit" />
      <button type="button" class="btn btn-ghost btn-sm text-zinc-400 hover:text-rose-600" aria-label="Quitar" @click="remove(i)">
        <Trash2 class="size-3.5" />
      </button>
    </div>
    <button type="button" class="btn btn-ghost btn-sm text-brand-700" @click="add"><Plus class="size-3.5" /> {{ addLabel }}</button>
  </div>
</template>
