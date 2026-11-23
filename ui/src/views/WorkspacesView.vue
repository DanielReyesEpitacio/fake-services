<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { FolderInput, Plus, RefreshCw } from 'lucide-vue-next'
import { api, ApiError } from '../lib/api'
import { refresh, toast, toastError } from '../lib/store'
import CopyButton from '../components/CopyButton.vue'
import IssuesList from '../components/IssuesList.vue'
import ModalDialog from '../components/ModalDialog.vue'
import type { Issue, WorkspaceInfo } from '../lib/types'

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
  if (!confirm(`¿Quitar “${w.name}” de la lista? Los archivos en ${w.path} no se borran.`)) return
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
  <div class="space-y-6 p-8">
    <header class="flex items-end justify-between">
      <div>
        <h1 class="page-title">Espacios de trabajo</h1>
        <p class="page-subtitle">
          Un espacio es una carpeta con archivos JSON: puedes guardarlo en el repositorio de tu proyecto y compartirlo por Git.
        </p>
      </div>
      <div class="flex gap-2">
        <button class="btn" @click="reload"><RefreshCw class="size-3.5" /> Recargar desde disco</button>
        <button class="btn" @click="openModal('register')"><FolderInput class="size-3.5" /> Abrir carpeta existente</button>
        <button class="btn btn-primary" @click="openModal('create')"><Plus class="size-4" /> Nuevo espacio</button>
      </div>
    </header>

    <div class="grid grid-cols-2 gap-4">
      <div
        v-for="w in list"
        :key="w.id"
        class="card p-4"
        :class="w.active ? 'ring-2 ring-brand-500' : ''"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <h2 class="font-semibold">{{ w.name }}</h2>
              <span v-if="w.active" class="badge bg-brand-100 text-brand-700">Activo</span>
              <span v-if="!w.available" class="badge bg-rose-50 text-rose-700">Carpeta no disponible</span>
            </div>
            <p v-if="w.description" class="mt-0.5 text-sm text-zinc-500">{{ w.description }}</p>
            <div class="mt-2 flex items-center gap-1.5 text-xs text-zinc-500">
              <code class="truncate">{{ w.path }}</code>
              <CopyButton :text="w.path" class="hover:text-zinc-800" />
            </div>
          </div>
          <div class="flex shrink-0 flex-col gap-1.5">
            <button v-if="!w.active" class="btn btn-sm btn-primary" :disabled="!w.available" @click="activate(w)">Activar</button>
            <button v-if="!w.active" class="btn btn-sm btn-ghost text-zinc-500" @click="forget(w)">Quitar</button>
          </div>
        </div>
      </div>
    </div>

    <ModalDialog
      v-if="modal"
      :title="modal === 'create' ? 'Nuevo espacio de trabajo' : 'Abrir carpeta existente'"
      @close="modal = null"
    >
      <form class="space-y-3" @submit.prevent="submit">
        <template v-if="modal === 'create'">
          <div>
            <label class="label">Nombre</label>
            <input v-model="form.name" class="input" placeholder="Proyecto Pagos" autofocus />
          </div>
          <div>
            <label class="label">Descripción</label>
            <input v-model="form.description" class="input" />
          </div>
          <div>
            <label class="label">Carpeta (opcional)</label>
            <input v-model="form.path" class="input-mono" placeholder="/home/yo/proyectos/pagos/.fakes" />
            <p class="hint">Déjala vacía para guardarlo en la carpeta de datos de Fake Services.</p>
          </div>
        </template>
        <div v-else>
          <label class="label">Carpeta que contiene fakes.workspace.json</label>
          <input v-model="form.path" class="input-mono" placeholder="/home/yo/proyectos/pagos/.fakes" autofocus />
        </div>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="form.activate" type="checkbox" class="size-4 accent-brand-600" /> Activarlo ahora
        </label>
        <IssuesList :issues="issues" />
      </form>
      <template #footer>
        <button class="btn" @click="modal = null">Cancelar</button>
        <button class="btn btn-primary" :disabled="modal === 'create' ? !form.name : !form.path" @click="submit">
          {{ modal === 'create' ? 'Crear' : 'Abrir' }}
        </button>
      </template>
    </ModalDialog>
  </div>
</template>
