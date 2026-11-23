<script setup lang="ts">
import { Plus, Trash2 } from 'lucide-vue-next'
import type { ParamSpec } from '../lib/types'

defineProps<{ namePlaceholder?: string; addLabel?: string }>()
const model = defineModel<ParamSpec[]>({ required: true })

function add() {
  model.value.push({ name: '', required: true })
}
</script>

<template>
  <div>
    <table v-if="model.length" class="table mb-1">
      <thead>
        <tr>
          <th class="w-1/4">Nombre</th>
          <th class="w-20">Obligatorio</th>
          <th class="w-1/4">Patrón (regex)</th>
          <th>Descripción</th>
          <th class="w-8"></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(p, i) in model" :key="i">
          <td class="!px-1"><input v-model="p.name" class="input-mono" :placeholder="namePlaceholder ?? 'nombre'" /></td>
          <td class="text-center"><input v-model="p.required" type="checkbox" class="size-4 accent-brand-600" /></td>
          <td class="!px-1">
            <input
              :value="p.pattern ?? ''"
              class="input-mono"
              placeholder="opcional"
              @input="p.pattern = ($event.target as HTMLInputElement).value || null"
            />
          </td>
          <td class="!px-1">
            <input
              :value="p.description ?? ''"
              class="input"
              @input="p.description = ($event.target as HTMLInputElement).value"
            />
          </td>
          <td class="!px-1">
            <button type="button" class="btn btn-ghost btn-sm text-zinc-400 hover:text-rose-600" aria-label="Quitar" @click="model.splice(i, 1)">
              <Trash2 class="size-3.5" />
            </button>
          </td>
        </tr>
      </tbody>
    </table>
    <button type="button" class="btn btn-ghost btn-sm text-brand-700" @click="add">
      <Plus class="size-3.5" /> {{ addLabel ?? 'Agregar' }}
    </button>
  </div>
</template>
