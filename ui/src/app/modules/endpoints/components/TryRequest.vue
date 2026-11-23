<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import CodeEditor from '@/app/shared/components/CodeEditor.vue'
import KeyValueEditor from '@/app/shared/components/KeyValueEditor.vue'
import StatusChip from '@/app/shared/components/StatusChip.vue'
import type { Endpoint } from '@/infrastructure/api/types.ts'

const props = defineProps<{ endpoint: Endpoint; url: string }>()

/** Sustituye `{param}` por valores de ejemplo para tener una URL lista. */
function exampleUrl(): string {
  return props.url.replace(/\{\*?([\w-]+)\}/g, (_, name: string) => {
    const spec = props.endpoint.request?.pathParams?.find((p) => p.name === name)
    return spec?.example ?? `${name}_123`
  })
}

const methods = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']
const method = ref(props.endpoint.method === '*' ? 'GET' : props.endpoint.method)
const url = ref(exampleUrl())
const headers = ref<Record<string, string>>({})
const scenario = ref<string | null>(null)
const body = ref('')

watch(
  () => props.endpoint,
  (ep) => {
    if (!body.value && ep.request?.body?.example !== undefined) {
      body.value = JSON.stringify(ep.request.body.example, null, 2)
    }
    if (!Object.keys(headers.value).length && ep.request?.body?.kind === 'json') {
      headers.value = { 'content-type': 'application/json' }
    }
  },
  { immediate: true },
)

const scenarios = computed(() => [
  { title: 'Según reglas', value: null },
  ...props.endpoint.scenarios.map((s) => ({ title: s.name, value: s.id })),
])

const sending = ref(false)
const result = ref<{ status: number; ms: number; headers: [string, string][]; body: string } | null>(null)
const error = ref('')
const hasBody = computed(() => !['GET', 'HEAD'].includes(method.value))

async function send() {
  sending.value = true
  error.value = ''
  result.value = null
  const h: Record<string, string> = { ...headers.value }
  if (scenario.value) h['x-fakes-scenario'] = scenario.value
  const started = performance.now()
  try {
    const resp = await fetch(url.value, {
      method: method.value,
      headers: h,
      body: hasBody.value && body.value ? body.value : undefined,
    })
    const text = await resp.text()
    let pretty = text
    try {
      pretty = JSON.stringify(JSON.parse(text), null, 2)
    } catch {
      /* no es JSON */
    }
    result.value = {
      status: resp.status,
      ms: Math.round(performance.now() - started),
      headers: [...resp.headers.entries()],
      body: pretty,
    }
  } catch (e) {
    error.value = `No se pudo completar la solicitud: ${(e as Error).message}. ¿El servicio está iniciado y CORS habilitado?`
  } finally {
    sending.value = false
  }
}
</script>

<template>
  <div class="d-flex flex-column gap-4">
    <div class="d-flex gap-2 align-center">
      <FSelect v-model="method" :items="methods" class="mono" hide-details style="max-width: 130px" />
      <FTextField v-model="url" class="mono" hide-details @keydown.enter="send" />
      <VBtn color="primary" prepend-icon="mdi-send" :loading="sending" @click="send">Enviar</VBtn>
    </div>
    <VRow>
      <VCol cols="12" md="7">
        <div class="text-label-medium text-medium-emphasis mb-1">Headers</div>
        <KeyValueEditor v-model="headers" key-placeholder="authorization" value-placeholder="Bearer …" add-label="Agregar header" />
      </VCol>
      <VCol cols="12" md="5">
        <FSelect
          v-model="scenario"
          :items="scenarios"
          label="Forzar escenario"
          hint="Envía el header x-fakes-scenario."
          persistent-hint
        />
      </VCol>
    </VRow>
    <CodeEditor v-if="hasBody" v-model="body" language="json" min-height="80px" label="Body" />
    <VAlert v-if="error" type="error" variant="tonal">{{ error }}</VAlert>
    <VCard v-if="result">
      <VCardText>
        <div class="d-flex align-center gap-2 mb-3">
          <StatusChip :status="result.status" />
          <span class="text-body-small text-medium-emphasis">{{ result.ms }} ms</span>
        </div>
        <VExpansionPanels class="mb-3">
          <VExpansionPanel :title="`Headers (${result.headers.length})`">
            <VExpansionPanelText>
              <div v-for="([k, v], i) in result.headers" :key="i" class="mono text-body-small">
                <span class="text-medium-emphasis">{{ k }}:</span> {{ v }}
              </div>
            </VExpansionPanelText>
          </VExpansionPanel>
        </VExpansionPanels>
        <pre class="code-block">{{ result.body || '(sin body)' }}</pre>
      </VCardText>
    </VCard>
  </div>
</template>
