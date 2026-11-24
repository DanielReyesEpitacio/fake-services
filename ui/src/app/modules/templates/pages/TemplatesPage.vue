<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '@/infrastructure/api/client.ts'
import { refresh, toast, toastError } from '@/app/shared/state/live.ts'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import type { DetectionReport, TemplateInfo } from '@/infrastructure/api/types.ts'

const router = useRouter()
const templates = ref<TemplateInfo[]>([])
const loading = ref(true)
const installing = ref<string | null>(null)

const icons: Record<string, string> = {
  mifiel: 'mdi-draw-pen',
  finkok: 'mdi-receipt-text-outline',
  stripe: 'mdi-credit-card-outline',
  sendgrid: 'mdi-email-fast-outline',
  'aws-s3': 'mdi-bucket-outline',
  belvo: 'mdi-bank-outline',
  copomex: 'mdi-map-marker-outline',
}

async function load() {
  loading.value = true
  try {
    templates.value = await api.templates()
  } catch (e) {
    toastError(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)

async function install(t: TemplateInfo) {
  installing.value = t.id
  try {
    const r = await api.installTemplate(t.id, { onConflict: 'rename' })
    toast(`“${t.name}” instalado como ${r.serviceId}`)
    await refresh()
    router.push(`/servicios/${r.serviceId}`)
  } catch (e) {
    toastError(e)
  } finally {
    installing.value = null
  }
}

// -- detección ---------------------------------------------------------------

const path = ref('')
const detecting = ref(false)
const report = ref<DetectionReport | null>(null)

async function detect() {
  if (!path.value.trim()) return
  detecting.value = true
  try {
    report.value = await api.detect(path.value.trim())
  } catch (e) {
    report.value = null
    toastError(e)
  } finally {
    detecting.value = false
  }
}

const byId = computed(() => Object.fromEntries(templates.value.map((t) => [t.id, t])))
</script>

<template>
  <div style="max-width: 1200px">
    <PageHeader
      title="Plantillas de proveedores"
      subtitle="Servicios listos para usar, con endpoints, escenarios de error, recursos persistentes y webhooks. Instálalos y adáptalos."
    />

    <VRow>
      <VCol v-for="t in templates" :key="t.id" cols="12" sm="6" lg="4">
        <VCard class="h-100 d-flex flex-column">
          <VCardItem>
            <template #prepend>
              <VAvatar color="primary" variant="tonal" rounded>
                <VIcon :icon="icons[t.id] ?? 'mdi-puzzle-outline'" />
              </VAvatar>
            </template>
            <VCardTitle>{{ t.name }}</VCardTitle>
            <VCardSubtitle>{{ t.category }}</VCardSubtitle>
            <template #append>
              <VChip v-if="t.installed" color="success" size="small" prepend-icon="mdi-check">Instalado</VChip>
            </template>
          </VCardItem>
          <VCardText class="flex-1-1">
            <p class="text-body-medium mb-3">{{ t.description }}</p>
            <div class="d-flex flex-wrap gap-1 mb-2">
              <VChip v-for="tag in t.tags" :key="tag" size="x-small" variant="outlined">{{ tag }}</VChip>
            </div>
            <div class="text-body-small text-medium-emphasis">
              {{ t.endpoints }} endpoints · {{ t.workflows }} workflows · prefijo <code class="inline">{{ t.prefix }}</code>
            </div>
            <div v-if="t.upstreamUrl" class="text-body-small text-medium-emphasis text-truncate">
              Reemplaza a <code class="inline">{{ t.upstreamUrl }}</code>
            </div>
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn v-if="t.installed" variant="text" @click="router.push(`/servicios/${t.id}`)">Abrir</VBtn>
            <VBtn
              color="primary"
              :variant="t.installed ? 'tonal' : 'flat'"
              prepend-icon="mdi-download"
              :loading="installing === t.id"
              @click="install(t)"
            >
              {{ t.installed ? 'Instalar copia' : 'Instalar' }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VCol>
      <VCol v-if="loading" cols="12"><VProgressLinear indeterminate color="primary" /></VCol>
    </VRow>

    <VCard class="mt-6" title="Detectar integraciones de un proyecto" prepend-icon="mdi-radar">
      <VCardText class="d-flex flex-column gap-3">
        <div class="text-body-medium text-medium-emphasis">
          Analiza dependencias, URLs y variables de entorno de un proyecto en este equipo y te dice qué servicios externos usa.
        </div>
        <div class="d-flex gap-2 align-end">
          <FTextField
            v-model="path"
            label="Carpeta del proyecto"
            placeholder="/home/ana/proyectos/mi-app"
            class="mono flex-1-1"
            hide-details
            @keyup.enter="detect"
          />
          <VBtn color="primary" :loading="detecting" :disabled="!path.trim()" @click="detect">Analizar</VBtn>
        </div>

        <template v-if="report">
          <div class="text-body-small text-medium-emphasis">
            {{ report.filesScanned }} archivos analizados en <code class="inline">{{ report.root }}</code>
            <template v-if="report.truncated"> (se alcanzó el límite)</template>.
          </div>
          <VAlert v-if="!report.integrations.length" type="info" variant="tonal" density="compact">
            No se encontraron proveedores conocidos.
          </VAlert>
          <VTable v-else>
            <thead>
              <tr>
                <th>Proveedor</th>
                <th>Confianza</th>
                <th>Evidencia</th>
                <th style="width: 180px"></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="i in report.integrations" :key="i.providerId">
                <td class="font-weight-medium">{{ i.name }}</td>
                <td>
                  <VChip size="small" :color="i.confidence === 'alta' ? 'success' : 'warning'">{{ i.confidence }}</VChip>
                </td>
                <td class="text-body-small">
                  <div v-for="(e, k) in i.evidence.slice(0, 2)" :key="k" class="text-truncate" style="max-width: 460px">
                    <code class="inline">{{ e.file }}:{{ e.line }}</code> {{ e.snippet }}
                  </div>
                  <div v-if="i.evidence.length > 2" class="text-medium-emphasis">y {{ i.evidence.length - 2 }} más</div>
                </td>
                <td class="text-right">
                  <VBtn
                    v-if="i.template && byId[i.template]"
                    size="small"
                    color="primary"
                    variant="tonal"
                    :loading="installing === i.template"
                    @click="install(byId[i.template]!)"
                  >
                    {{ byId[i.template]!.installed ? 'Instalar copia' : 'Instalar plantilla' }}
                  </VBtn>
                  <span v-else class="text-body-small text-medium-emphasis">Sin plantilla</span>
                </td>
              </tr>
            </tbody>
          </VTable>
          <div v-if="report.otherHosts.length" class="text-body-small text-medium-emphasis">
            Otros dominios de APIs: {{ report.otherHosts.slice(0, 10).map((h) => h.host).join(', ') }}
          </div>
        </template>
      </VCardText>
    </VCard>
  </div>
</template>
