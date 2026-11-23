<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { refresh, toast, toastError } from '@/app/shared/state/live.ts'
import { confirmAction } from '@/app/shared/state/confirm.ts'
import CopyButton from '@/app/shared/components/CopyButton.vue'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import type { Issue, WorkspaceInfo } from '@/infrastructure/api/types.ts'

const router = useRouter()
const list = ref<WorkspaceInfo[]>([])

async function load() {
  try {
    list.value = await api.workspaces()
  } catch (e) {
    toastError(e)
  }
}
load()

async function activate(w: WorkspaceInfo) {
  try {
    await api.activateWorkspace(w.id)
    toast(`Espacio “${w.name}” activo`)
    await Promise.all([load(), refresh()])
    router.push('/')
  } catch (e) {
    toastError(e)
  }
}

async function forget(w: WorkspaceInfo) {
  const ok = await confirmAction({
    title: 'Quitar de la lista',
    text: `“${w.name}” dejará de aparecer aquí. Los archivos en ${w.path} no se borran.`,
    confirmText: 'Quitar',
  })
  if (!ok) return
  try {
    await api.forgetWorkspace(w.id)
    load()
  } catch (e) {
    toastError(e)
  }
}

async function reload() {
  try {
    await api.reload()
    await refresh()
    toast('Configuración recargada desde disco')
  } catch (e) {
    toastError(e)
  }
}

const modal = ref<'create' | 'register' | null>(null)
const form = ref({ name: '', description: '', path: '', activate: true })
const issues = ref<Issue[]>([])

function openModal(kind: 'create' | 'register') {
  form.value = { name: '', description: '', path: '', activate: true }
  issues.value = []
  modal.value = kind
}

async function submit() {
  issues.value = []
  try {
    if (modal.value === 'create') {
      await api.createWorkspace({
        name: form.value.name,
        description: form.value.description,
        path: form.value.path || undefined,
        activate: form.value.activate,
      })
    } else {
      await api.registerWorkspace(form.value.path, form.value.activate)
    }
    modal.value = null
    toast('Espacio listo')
    await Promise.all([load(), refresh()])
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  }
}
</script>

<template>
  <div>
    <PageHeader
      title="Espacios de trabajo"
      subtitle="Un espacio es una carpeta con archivos JSON: guárdalo en el repositorio de tu proyecto y compártelo por Git."
    >
      <template #actions>
        <VBtn variant="tonal" prepend-icon="mdi-refresh" @click="reload">Recargar desde disco</VBtn>
        <VBtn variant="tonal" prepend-icon="mdi-folder-open-outline" @click="openModal('register')">Abrir carpeta existente</VBtn>
        <VBtn color="primary" prepend-icon="mdi-plus" @click="openModal('create')">Nuevo espacio</VBtn>
      </template>
    </PageHeader>

    <VRow>
      <VCol v-for="w in list" :key="w.id" cols="12" md="6">
        <VCard :class="{ 'workspace-active': w.active }" class="h-100">
          <VCardItem>
            <VCardTitle class="d-flex align-center gap-2">
              {{ w.name }}
              <VChip v-if="w.active" color="primary" size="small">Activo</VChip>
              <VChip v-if="!w.available" color="error" size="small" variant="tonal">Carpeta no disponible</VChip>
            </VCardTitle>
            <VCardSubtitle v-if="w.description">{{ w.description }}</VCardSubtitle>
          </VCardItem>
          <VCardText>
            <div class="d-flex align-center gap-1 text-body-small text-medium-emphasis">
              <VIcon icon="mdi-folder-outline" size="16" />
              <code class="text-truncate">{{ w.path }}</code>
              <CopyButton :text="w.path" />
            </div>
          </VCardText>
          <VCardActions v-if="!w.active">
            <VBtn color="primary" :disabled="!w.available" @click="activate(w)">Activar</VBtn>
            <VBtn variant="text" @click="forget(w)">Quitar</VBtn>
          </VCardActions>
        </VCard>
      </VCol>
    </VRow>

    <VDialog :model-value="modal !== null" max-width="560" @update:model-value="(v) => !v && (modal = null)">
      <VCard :title="modal === 'create' ? 'Nuevo espacio de trabajo' : 'Abrir carpeta existente'">
        <VCardText>
          <VForm class="d-flex flex-column gap-3" @submit.prevent="submit">
            <template v-if="modal === 'create'">
              <FTextField v-model="form.name" label="Nombre" placeholder="Proyecto Pagos" autofocus />
              <FTextField v-model="form.description" label="Descripción" />
              <FTextField
                v-model="form.path"
                label="Carpeta (opcional)"
                placeholder="/home/yo/proyectos/pagos/.fakes"
                hint="Déjala vacía para guardarlo en la carpeta de datos de Fake Services."
                persistent-hint
                class="mono"
              />
            </template>
            <FTextField
              v-else
              v-model="form.path"
              label="Carpeta que contiene fakes.workspace.json"
              placeholder="/home/yo/proyectos/pagos/.fakes"
              class="mono"
              autofocus
            />
            <VCheckbox v-model="form.activate" label="Activarlo ahora" hide-details />
            <IssuesList :issues="issues" />
          </VForm>
        </VCardText>
        <VCardActions>
          <VSpacer />
          <VBtn variant="text" @click="modal = null">Cancelar</VBtn>
          <VBtn color="primary" :disabled="modal === 'create' ? !form.name : !form.path" @click="submit">
            {{ modal === 'create' ? 'Crear' : 'Abrir' }}
          </VBtn>
        </VCardActions>
      </VCard>
    </VDialog>
  </div>
</template>

<style scoped>
.workspace-active {
  outline: 2px solid rgb(var(--v-theme-primary));
  outline-offset: -2px;
}
</style>
