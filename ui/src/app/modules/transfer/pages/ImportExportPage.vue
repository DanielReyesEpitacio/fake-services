<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { api, ApiError } from '@/infrastructure/api/client.ts'
import { refresh, store, toast, toastError } from '@/app/shared/state/live.ts'
import IssuesList from '@/app/shared/components/IssuesList.vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import type { Bundle, ImportMode, ImportReport, Issue, OnConflict, SpecImportReport } from '@/infrastructure/api/types.ts'

const router = useRouter()

// -- exportar ---------------------------------------------------------------

const selected = ref<string[]>([])
const includeWorkspace = ref(true)
const includeFiles = ref(true)
const includeResources = ref(false)
const exporting = ref(false)

watch(
  () => store.services.map((s) => s.id).join(','),
  () => (selected.value = store.services.map((s) => s.id)),
  { immediate: true },
)

const allSelected = computed(() => selected.value.length === store.services.length)

async function doExport() {
  exporting.value = true
  try {
    const name = await api.exportBundle({
      services: allSelected.value ? [] : selected.value,
      workspace: includeWorkspace.value,
      files: includeFiles.value,
      resources: includeResources.value,
    })
    toast(`Exportado: ${name}`)
  } catch (e) {
    toastError(e)
  } finally {
    exporting.value = false
  }
}

// -- importar ---------------------------------------------------------------

const file = ref<File | File[] | null>(null)
const bundle = ref<Bundle | null>(null)
const fileError = ref('')
const mode = ref<ImportMode>('merge')
const onConflict = ref<OnConflict>('rename')
const workspaceName = ref('')
const preview = ref<ImportReport | null>(null)
const result = ref<ImportReport | null>(null)
const issues = ref<Issue[]>([])
const busy = ref(false)
// Especificación (OpenAPI / Postman) en lugar de un bundle
const specContent = ref<string | null>(null)
const specName = ref('')
const specPrefix = ref('')
const specPreview = ref<SpecImportReport | null>(null)
const specResult = ref<SpecImportReport | null>(null)

watch(file, async (f) => {
  bundle.value = null
  preview.value = null
  result.value = null
  issues.value = []
  fileError.value = ''
  specContent.value = null
  specPreview.value = null
  specResult.value = null
  specName.value = ''
  specPrefix.value = ''
  const one = Array.isArray(f) ? f[0] : f
  if (!one) return
  const text = await one.text()
  let data: { format?: string; workspace?: { name?: string } } | null = null
  try {
    data = JSON.parse(text)
  } catch {
    data = null
  }
  if (data?.format !== 'fake-services') {
    // OpenAPI, Swagger o Postman (JSON o YAML): el servidor detecta el formato.
    specContent.value = text
    await checkSpec()
    return
  }
  try {
    bundle.value = data as Bundle
    workspaceName.value = data.workspace?.name ? `${data.workspace.name} (importado)` : 'Espacio importado'
    await check()
  } catch {
    fileError.value = 'No se pudo leer el archivo.'
  }
})

watch([mode, onConflict], () => {
  if (bundle.value) check()
  else if (specContent.value) checkSpec()
})

function specRequest(dryRun: boolean) {
  return {
    content: specContent.value!,
    name: specName.value || undefined,
    prefix: specPrefix.value || undefined,
    onConflict: onConflict.value,
    dryRun,
  }
}

async function checkSpec() {
  if (!specContent.value) return
  issues.value = []
  fileError.value = ''
  try {
    specPreview.value = await api.importSpec(specRequest(true))
  } catch (e) {
    specPreview.value = null
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else fileError.value = e instanceof Error ? e.message : String(e)
  }
}

async function doImportSpec() {
  busy.value = true
  issues.value = []
  try {
    specResult.value = await api.importSpec(specRequest(false))
    toast('Especificación importada')
    await refresh()
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  } finally {
    busy.value = false
  }
}

const specFormatLabel = computed(() =>
  specPreview.value?.format === 'postman' ? 'Colección de Postman' : 'OpenAPI / Swagger',
)

function request(dryRun: boolean) {
  return {
    bundle: bundle.value!,
    mode: mode.value,
    onConflict: onConflict.value,
    workspaceName: workspaceName.value,
    dryRun,
  }
}

async function check() {
  if (!bundle.value) return
  issues.value = []
  try {
    preview.value = await api.importBundle(request(true))
  } catch (e) {
    preview.value = null
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  }
}

async function doImport() {
  if (!bundle.value) return
  busy.value = true
  issues.value = []
  try {
    result.value = await api.importBundle(request(false))
    toast('Importación completa')
    await refresh()
  } catch (e) {
    if (e instanceof ApiError && e.issues.length) issues.value = e.issues
    else toastError(e)
  } finally {
    busy.value = false
  }
}

function reset() {
  file.value = null
}

const summary = computed(() => {
  const b = bundle.value
  if (!b) return null
  return {
    services: b.services.length,
    endpoints: b.services.reduce((n, s) => n + (s.endpoints?.length ?? 0), 0),
    workflows: b.services.reduce((n, s) => n + (s.workflows?.length ?? 0), 0),
    files: Object.keys(b.files ?? {}).length,
    workspace: !!b.workspace,
  }
})
</script>

<template>
  <div style="max-width: 1100px">
    <PageHeader
      title="Importar y exportar"
      subtitle="Lleva tus simulaciones a otra máquina, compártelas con tu equipo o guárdalas como respaldo en un solo archivo JSON."
    />

    <VRow>
      <!-- Exportar -->
      <VCol cols="12" md="6">
        <VCard title="Exportar" prepend-icon="mdi-download" class="h-100">
          <VCardText class="d-flex flex-column gap-3">
            <div class="text-body-medium text-medium-emphasis">
              Incluye endpoints, escenarios y workflows de cada servicio.
            </div>
            <div>
              <div class="d-flex align-center justify-space-between">
                <span class="f-field__label mb-0">Servicios</span>
                <VBtn
                  variant="text"
                  size="small"
                  @click="selected = allSelected ? [] : store.services.map((s) => s.id)"
                >
                  {{ allSelected ? 'Ninguno' : 'Todos' }}
                </VBtn>
              </div>
              <VCheckbox
                v-for="s in store.services"
                :key="s.id"
                v-model="selected"
                :value="s.id"
                :label="`${s.name} (${s.endpointCount} endpoints, ${s.workflowCount} workflows)`"
                hide-details
                density="compact"
              />
              <div v-if="!store.services.length" class="text-body-small text-medium-emphasis">No hay servicios en este espacio.</div>
            </div>
            <VSwitch v-model="includeWorkspace" label="Incluir ajustes del espacio (servidor, variables, webhooks)" color="primary" hide-details />
            <VSwitch v-model="includeFiles" label="Incluir archivos de la carpeta files/" color="primary" hide-details />
            <VSwitch v-model="includeResources" label="Incluir recursos simulados guardados (documentos, objetos...)" color="primary" hide-details />
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn color="primary" prepend-icon="mdi-download" :loading="exporting" :disabled="!selected.length" @click="doExport">
              Descargar archivo
            </VBtn>
          </VCardActions>
        </VCard>
      </VCol>

      <!-- Importar -->
      <VCol cols="12" md="6">
        <VCard title="Importar" prepend-icon="mdi-upload" class="h-100">
          <VCardText class="d-flex flex-column gap-3">
            <VFileInput
              v-model="file"
              accept=".json,.yaml,.yml,application/json"
              label="Exportación de Fake Services, OpenAPI/Swagger o colección de Postman"
              prepend-icon=""
              prepend-inner-icon="mdi-file-upload-outline"
              show-size
              hide-details
            />
            <VAlert v-if="fileError" type="error" variant="tonal" density="compact">{{ fileError }}</VAlert>

            <!-- Especificación -->
            <template v-if="specContent && (specPreview || issues.length)">
              <VAlert v-if="specPreview" type="info" variant="tonal" density="compact" icon="mdi-file-document-outline">
                {{ specFormatLabel }}: se creará el servicio <strong>{{ specPreview.serviceId }}</strong> con
                {{ specPreview.endpoints }} endpoints. Cada respuesta documentada es un escenario (la primera 2xx es la
                predeterminada; las demás se fuerzan con el header <code class="inline">x-fakes-scenario</code>).
              </VAlert>
              <div class="d-flex gap-3">
                <FTextField v-model="specName" label="Nombre del servicio" placeholder="El del documento" class="flex-1-1" @change="checkSpec" />
                <FTextField v-model="specPrefix" label="Prefijo" placeholder="/<id>" class="flex-1-1" @change="checkSpec" />
              </div>
              <div>
                <div class="f-field__label">Si el servicio ya existe</div>
                <VRadioGroup v-model="onConflict" hide-details>
                  <VRadio value="rename" label="Importarlo como copia" />
                  <VRadio value="replace" label="Reemplazar el existente" />
                  <VRadio value="skip" label="Omitirlo" />
                </VRadioGroup>
              </div>
              <IssuesList :issues="issues" />
              <VAlert v-if="specPreview && !specResult && specPreview.warnings.length" type="warning" variant="tonal" density="compact" title="Avisos">
                <ul class="ps-4 text-body-small">
                  <li v-for="(w, i) in specPreview.warnings.slice(0, 12)" :key="i">{{ w }}</li>
                  <li v-if="specPreview.warnings.length > 12">y {{ specPreview.warnings.length - 12 }} más…</li>
                </ul>
              </VAlert>
              <VAlert v-if="specResult" type="success" variant="tonal" title="Importación completa">
                <div class="text-body-small">Servicio {{ specResult.serviceId }} con {{ specResult.endpoints }} endpoints.</div>
                <div class="d-flex gap-2 mt-2">
                  <VBtn size="small" variant="tonal" @click="router.push(`/servicios/${specResult.serviceId}`)">Abrir servicio</VBtn>
                  <VBtn size="small" variant="text" @click="reset">Importar otro</VBtn>
                </div>
              </VAlert>
            </template>

            <template v-if="summary">
              <VAlert type="info" variant="tonal" density="compact" icon="mdi-package-variant-closed">
                {{ summary.services }} servicio{{ summary.services === 1 ? '' : 's' }}, {{ summary.endpoints }} endpoints,
                {{ summary.workflows }} workflows<template v-if="summary.files">, {{ summary.files }} archivos</template>
                <template v-if="summary.workspace"> y ajustes del espacio</template>.
                <div v-if="bundle?.exportedAt" class="text-body-small">Exportado el {{ new Date(bundle.exportedAt).toLocaleString('es-MX') }}</div>
              </VAlert>

              <div>
                <div class="f-field__label">Destino</div>
                <VRadioGroup v-model="mode" hide-details>
                  <VRadio value="merge" :label="`Agregar al espacio actual (${store.status?.workspaceName ?? ''})`" />
                  <VRadio value="newWorkspace" label="Crear un espacio nuevo y activarlo" />
                </VRadioGroup>
              </div>
              <FTextField v-if="mode === 'newWorkspace'" v-model="workspaceName" label="Nombre del espacio nuevo" />

              <div v-if="mode === 'merge'">
                <div class="f-field__label">Si un servicio ya existe</div>
                <VRadioGroup v-model="onConflict" hide-details>
                  <VRadio value="rename" label="Importarlo como copia (otro identificador y prefijo)" />
                  <VRadio value="replace" label="Reemplazar el existente" />
                  <VRadio value="skip" label="Conservar el existente y omitirlo" />
                </VRadioGroup>
              </div>

              <IssuesList :issues="issues" />

              <VAlert v-if="preview && !result" type="success" variant="tonal" density="compact" title="Se puede importar">
                <ul class="ps-4 text-body-small">
                  <li v-if="preview.created.length">Nuevos: {{ preview.created.join(', ') }}</li>
                  <li v-if="preview.replaced.length">Se reemplazan: {{ preview.replaced.join(', ') }}</li>
                  <li v-if="preview.skipped.length">Se omiten: {{ preview.skipped.join(', ') }}</li>
                  <li v-for="r in preview.renamed" :key="r.from">{{ r.from }} se importará como {{ r.to }}</li>
                  <li v-for="(w, i) in preview.warnings" :key="i">{{ w }}</li>
                </ul>
              </VAlert>

              <VAlert v-if="result" type="success" variant="tonal" title="Importación completa">
                <div class="text-body-small">
                  {{ result.created.length + result.replaced.length }} servicio(s), {{ result.endpoints }} endpoints,
                  {{ result.workflows }} workflows, {{ result.files }} archivo(s)<template v-if="result.resources">
                  y {{ result.resources }} recurso(s)</template>.
                </div>
                <div class="d-flex gap-2 mt-2">
                  <VBtn size="small" variant="tonal" @click="router.push('/servicios')">Ver servicios</VBtn>
                  <VBtn size="small" variant="text" @click="reset">Importar otro</VBtn>
                </div>
              </VAlert>
            </template>
          </VCardText>
          <VCardActions v-if="specContent && specPreview && !specResult">
            <VSpacer />
            <VBtn variant="text" @click="reset">Cancelar</VBtn>
            <VBtn color="primary" prepend-icon="mdi-upload" :loading="busy" @click="doImportSpec">Crear servicio</VBtn>
          </VCardActions>
          <VCardActions v-if="bundle && !result">
            <VSpacer />
            <VBtn variant="text" @click="reset">Cancelar</VBtn>
            <VBtn color="primary" prepend-icon="mdi-upload" :loading="busy" :disabled="!preview" @click="doImport">Importar</VBtn>
          </VCardActions>
        </VCard>
      </VCol>
    </VRow>

    <VAlert type="info" variant="tonal" icon="mdi-robot-outline" class="mt-4">
      También desde la API: <code class="inline">GET /api/v1/export</code>, <code class="inline">POST /api/v1/import</code> y
      <code class="inline">POST /api/v1/import/spec</code> (OpenAPI o Postman)
      (esquema en <code class="inline">/api/v1/schemas/bundle</code>). Los workflows sueltos se exportan e importan desde su pantalla.
    </VAlert>
  </div>
</template>
