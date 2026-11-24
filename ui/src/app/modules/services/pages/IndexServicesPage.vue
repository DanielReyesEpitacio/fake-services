<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { refresh, store, toast, toastError } from '@/app/shared/state/live.ts'
import CopyButton from '@/app/shared/components/CopyButton.vue'
import EmptyState from '@/app/shared/components/EmptyState.vue'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import RunToggle from '@/app/shared/components/RunToggle.vue'
import RunningDot from '@/app/shared/components/RunningDot.vue'
import type { Issue } from '@/infrastructure/api/types.ts'

const route = useRoute()
const router = useRouter()

const showCreate = ref(route.query.nuevo === '1')
const draft = ref({ name: '', description: '', prefix: '', port: '', upstreamUrl: '' })
const issues = ref<Issue[]>([])
const saving = ref(false)
const filter = ref('')

const suggestedPrefix = computed(() => {
  const slug = draft.value.name
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return slug ? `/${slug}` : '/mi-servicio'
})

const previewUrl = computed(() => `${store.status?.centralUrl ?? ''}${draft.value.prefix || suggestedPrefix.value}`)

const filtered = computed(() => {
  const q = filter.value.toLowerCase()
  return store.services.filter(
    (s) => !q || s.name.toLowerCase().includes(q) || s.prefix.includes(q) || s.description.toLowerCase().includes(q),
  )
})

function openCreate() {
  draft.value = { name: '', description: '', prefix: '', port: '', upstreamUrl: '' }
  issues.value = []
  showCreate.value = true
}

function closeCreate() {
  showCreate.value = false
  if (route.query.nuevo) router.replace({ query: {} })
}

async function create() {
  if (!draft.value.name) return
  saving.value = true
  issues.value = []
  try {
    const v = await api.createService({
      name: draft.value.name,
      description: draft.value.description || undefined,
      prefix: draft.value.prefix,
      port: draft.value.port ? Number(draft.value.port) : null,
      upstreamUrl: draft.value.upstreamUrl || null,
    })
    toast(`Servicio “${v.service.name}” creado`)
    await refresh()
    router.push(`/servicios/${v.service.id}`)
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  } finally {
    saving.value = false
  }
}

async function all(running: boolean) {
  try {
    await (running ? api.startAll() : api.stopAll())
    await refresh()
  } catch (e) {
    toastError(e)
  }
}
</script>

<template>
  <div>
    <PageHeader title="Servicios" subtitle="Cada servicio simula un proveedor externo con su propio prefijo de rutas.">
      <template #actions>
        <VBtn variant="tonal" prepend-icon="mdi-play" @click="all(true)">Iniciar todos</VBtn>
        <VBtn variant="tonal" prepend-icon="mdi-stop" @click="all(false)">Detener todos</VBtn>
        <VBtn variant="tonal" prepend-icon="mdi-puzzle-outline" to="/plantillas">Desde plantilla</VBtn>
        <VBtn color="primary" prepend-icon="mdi-plus" @click="openCreate">Nuevo servicio</VBtn>
      </template>
    </PageHeader>

    <FTextField
      v-if="store.services.length > 4"
      v-model="filter"
      prepend-inner-icon="mdi-magnify"
      placeholder="Buscar servicio…"
      class="mb-4"
      style="max-width: 360px"
      hide-details
    />

    <VCard>
      <EmptyState
        v-if="!store.services.length"
        icon="mdi-server-plus-outline"
        title="Aún no hay servicios"
        text="Un servicio representa un proveedor como MiFiel, Belvo o AWS S3."
      >
        <VBtn color="primary" prepend-icon="mdi-plus" @click="openCreate">Crear el primero</VBtn>
        <VBtn variant="tonal" prepend-icon="mdi-puzzle-outline" to="/plantillas" class="ms-2">Usar una plantilla</VBtn>
      </EmptyState>
      <VTable v-else hover class="clickable-rows">
        <thead>
          <tr>
            <th style="width: 32px"></th>
            <th>Servicio</th>
            <th>URL local</th>
            <th class="text-center" style="width: 110px">Endpoints</th>
            <th style="width: 130px"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in filtered" :key="s.id" @click="router.push(`/servicios/${s.id}`)">
            <td><RunningDot :running="s.running" /></td>
            <td>
              <div class="font-weight-medium">{{ s.name }}</div>
              <div class="text-body-small text-medium-emphasis text-truncate" style="max-width: 420px">{{ s.description }}</div>
            </td>
            <td>
              <div class="d-flex align-center gap-1">
                <code class="text-body-small">{{ s.url }}</code>
                <CopyButton :text="s.url" />
              </div>
              <div v-if="s.port" class="text-body-small text-medium-emphasis">también en el puerto {{ s.port }}</div>
            </td>
            <td class="text-center">{{ s.endpointCount }}</td>
            <td class="text-right"><RunToggle :id="s.id" :running="s.running" /></td>
          </tr>
        </tbody>
      </VTable>
    </VCard>

    <VDialog :model-value="showCreate" max-width="560" @update:model-value="(v) => !v && closeCreate()">
      <VCard title="Nuevo servicio">
        <VCardText>
          <VForm class="d-flex flex-column gap-3" @submit.prevent="create">
            <FTextField v-model="draft.name" label="Nombre" placeholder="MiFiel" autofocus />
            <FTextField v-model="draft.description" label="Descripción" placeholder="Firma electrónica de documentos" />
            <VRow dense>
              <VCol cols="7"><FTextField v-model="draft.prefix" label="Prefijo de rutas" :placeholder="suggestedPrefix" class="mono" /></VCol>
              <VCol cols="5"><FTextField v-model="draft.port" label="Puerto propio (opcional)" type="number" class="mono" /></VCol>
            </VRow>
            <FTextField
              v-model="draft.upstreamUrl"
              label="URL del proveedor real (opcional)"
              placeholder="https://app.mifiel.com/api/v1"
              hint="Solo como referencia; Fake Services nunca la llama."
              persistent-hint
              class="mono"
            />
            <div class="text-body-small text-medium-emphasis">
              Quedará disponible en
              <code class="inline">{{ previewUrl }}</code>
            </div>
            <IssuesList :issues="issues" />
          </VForm>
        </VCardText>
        <VCardActions>
          <VSpacer />
          <VBtn variant="text" @click="closeCreate">Cancelar</VBtn>
          <VBtn color="primary" :disabled="!draft.name" :loading="saving" @click="create">Crear servicio</VBtn>
        </VCardActions>
      </VCard>
    </VDialog>
  </div>
</template>
