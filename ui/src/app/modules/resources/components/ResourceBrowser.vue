<script setup lang="ts">
// Explorador de recursos simulados persistentes de un servicio.
import { computed, onMounted, provide, ref, watch } from 'vue'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { confirmAction as confirm } from '@/app/shared/state/confirm.ts'
import { toast, toastError } from '@/app/shared/state/live.ts'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import JsonEditor from '@/app/shared/components/JsonEditor.vue'
import type { CollectionInfo, ResourceRecord } from '@/infrastructure/api/types.ts'

const props = defineProps<{ sid: string }>()
const emit = defineEmits<{ count: [number] }>()

const collections = ref<CollectionInfo[]>([])
const current = ref<string | null>(null)
const records = ref<ResourceRecord[]>([])
const filter = ref('')
const loading = ref(false)

const invalid = ref(new Set<symbol>())
provide('invalidJson', invalid.value)

async function loadCollections() {
  try {
    collections.value = await api.collections(props.sid)
    emit('count', collections.value.reduce((n, c) => n + c.count, 0))
    if (current.value && !collections.value.some((c) => c.collection === current.value)) current.value = null
    if (!current.value && collections.value.length) current.value = collections.value[0]!.collection
  } catch (e) {
    toastError(e)
  }
}

async function loadRecords() {
  if (!current.value) {
    records.value = []
    return
  }
  loading.value = true
  try {
    records.value = await api.resources(props.sid, current.value)
  } catch (e) {
    toastError(e)
  } finally {
    loading.value = false
  }
}

async function reload() {
  await loadCollections()
  await loadRecords()
}

onMounted(reload)
watch(current, loadRecords)
watch(() => props.sid, reload)

const filtered = computed(() => {
  const q = filter.value.toLowerCase()
  return q ? records.value.filter((r) => JSON.stringify(r.data).toLowerCase().includes(q)) : records.value
})

function preview(r: ResourceRecord) {
  const { id: _id, ...rest } = r.data
  const s = JSON.stringify(rest)
  return s.length > 140 ? s.slice(0, 140) + '…' : s
}

// -- edición ------------------------------------------------------------------

const dialog = ref(false)
const editing = ref<{ isNew: boolean; collection: string; id: string; data: unknown }>({
  isNew: true,
  collection: '',
  id: '',
  data: {},
})
const saving = ref(false)
const error = ref('')

function openNew() {
  editing.value = { isNew: true, collection: current.value ?? '', id: '', data: {} }
  error.value = ''
  dialog.value = true
}

function openEdit(r: ResourceRecord) {
  const { id: _id, ...rest } = r.data
  editing.value = { isNew: false, collection: current.value!, id: r.id, data: rest }
  error.value = ''
  dialog.value = true
}

async function save() {
  saving.value = true
  error.value = ''
  try {
    const e = editing.value
    await api.putResource(props.sid, e.collection.trim(), e.id.trim(), e.data ?? {})
    toast('Recurso guardado')
    dialog.value = false
    current.value = e.collection.trim()
    await reload()
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    saving.value = false
  }
}

async function remove(r: ResourceRecord) {
  if (!(await confirm({ title: 'Eliminar recurso', text: `Se borrará ${current.value}/${r.id}.`, confirmText: 'Eliminar', danger: true }))) return
  try {
    await api.deleteResource(props.sid, current.value!, r.id)
    await reload()
  } catch (e) {
    toastError(e)
  }
}

async function clearCollection() {
  if (!current.value) return
  if (
    !(await confirm({
      title: 'Vaciar colección',
      text: `Se borrarán los ${records.value.length} registros de «${current.value}».`,
      confirmText: 'Vaciar',
      danger: true,
    }))
  )
    return
  try {
    await api.clearResources(props.sid, current.value)
    current.value = null
    await reload()
  } catch (e) {
    toastError(e)
  }
}

const fmt = (ms: number) => new Date(ms).toLocaleString('es-MX')
</script>

<template>
  <VCard>
    <VCardItem>
      <VCardSubtitle>
        Datos que las simulaciones guardan y leen entre llamadas (escenarios con «recurso» y pasos de workflow).
      </VCardSubtitle>
      <template #append>
        <div class="d-flex gap-2">
          <VBtn size="small" variant="text" prepend-icon="mdi-refresh" @click="reload">Actualizar</VBtn>
          <VBtn size="small" color="primary" prepend-icon="mdi-plus" @click="openNew">Nuevo registro</VBtn>
        </div>
      </template>
    </VCardItem>

    <EmptyState
      v-if="!collections.length"
      icon="mdi-database-outline"
      title="Sin recursos guardados"
      text="Cuando un escenario cree un recurso (por ejemplo, POST /documents) aparecerá aquí. También puedes crear registros a mano para preparar una prueba."
    />

    <template v-else>
      <VCardText class="d-flex flex-wrap align-center gap-2 pb-0">
        <VChipGroup v-model="current" mandatory selected-class="text-primary">
          <VChip v-for="c in collections" :key="c.collection" :value="c.collection" filter>
            {{ c.collection }} <span class="ms-1 text-medium-emphasis">({{ c.count }})</span>
          </VChip>
        </VChipGroup>
        <VSpacer />
        <FTextField v-model="filter" placeholder="Buscar…" prepend-inner-icon="mdi-magnify" hide-details style="max-width: 240px" />
        <VBtn size="small" variant="text" color="error" prepend-icon="mdi-delete-sweep-outline" @click="clearCollection">Vaciar</VBtn>
      </VCardText>
      <VProgressLinear v-if="loading" indeterminate color="primary" />
      <VTable hover class="clickable-rows">
        <thead>
          <tr>
            <th style="width: 260px">Id</th>
            <th>Datos</th>
            <th style="width: 170px">Actualizado</th>
            <th style="width: 50px"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in filtered" :key="r.id" @click="openEdit(r)">
            <td><code class="text-body-small">{{ r.id }}</code></td>
            <td class="text-body-small mono text-truncate" style="max-width: 520px">{{ preview(r) }}</td>
            <td class="text-body-small text-medium-emphasis">{{ fmt(r.updatedMs) }}</td>
            <td @click.stop>
              <VBtn icon="mdi-delete-outline" size="small" variant="text" aria-label="Eliminar" @click="remove(r)" />
            </td>
          </tr>
        </tbody>
      </VTable>
    </template>

    <VDialog v-model="dialog" max-width="720">
      <VCard :title="editing.isNew ? 'Nuevo registro' : `${editing.collection}/${editing.id}`">
        <VCardText class="d-flex flex-column gap-3">
          <div v-if="editing.isNew" class="d-flex gap-3">
            <FTextField v-model="editing.collection" label="Colección" placeholder="documents" class="mono flex-1-1" />
            <FTextField v-model="editing.id" label="Id" placeholder="doc_123" class="mono flex-1-1" />
          </div>
          <JsonEditor v-model="editing.data" label="Datos (JSON)" min-height="240px" empty-as="object" />
          <VAlert v-if="error" type="error" variant="tonal" density="compact">{{ error }}</VAlert>
        </VCardText>
        <VCardActions>
          <VSpacer />
          <VBtn variant="text" @click="dialog = false">Cancelar</VBtn>
          <VBtn
            color="primary"
            :loading="saving"
            :disabled="!editing.collection.trim() || !editing.id.trim() || invalid.size > 0"
            @click="save"
          >
            Guardar
          </VBtn>
        </VCardActions>
      </VCard>
    </VDialog>
  </VCard>
</template>
