<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Send } from 'lucide-vue-next'
import CodeEditor from './CodeEditor.vue'
import KeyValueEditor from './KeyValueEditor.vue'
import StatusCode from './StatusCode.vue'
import type { Endpoint } from '../lib/types'

const props = defineProps<{ endpoint: Endpoint; url: string }>()

/** Sustituye `{param}` por valores de ejemplo para tener una URL lista. */
function exampleUrl(): string {
  return props.url.replace(/\{\*?([\w-]+)\}/g, (_, name: string) => {
    const spec = props.endpoint.request?.pathParams?.find((p) => p.name === name)
    return spec?.example ?? `${name}_123`
  })
}

const method = ref(props.endpoint.method === '*' ? 'GET' : props.endpoint.method)
const url = ref(exampleUrl())
const headers = ref<Record<string, string>>({})
const scenario = ref('')
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
  <div class="space-y-3">
    <div class="flex gap-1.5">
      <select v-model="method" class="input-mono w-28">
        <option v-for="m in ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']" :key="m">{{ m }}</option>
      </select>
      <input v-model="url" class="input-mono flex-1" @keydown.enter="send" />
      <button class="btn btn-primary" :disabled="sending" @click="send"><Send class="size-3.5" /> Enviar</button>
    </div>
    <div class="grid grid-cols-2 gap-3">
      <div>
        <label class="label">Headers</label>
        <KeyValueEditor v-model="headers" key-placeholder="authorization" value-placeholder="Bearer …" add-label="Agregar header" />
      </div>
      <div>
        <label class="label">Forzar escenario</label>
        <select v-model="scenario" class="input">
          <option value="">Según reglas</option>
          <option v-for="s in endpoint.scenarios" :key="s.id" :value="s.id">{{ s.name }}</option>
        </select>
        <p class="hint">Envía el header <code>x-fakes-scenario</code>.</p>
      </div>
    </div>
    <div v-if="hasBody">
      <label class="label">Body</label>
      <CodeEditor v-model="body" language="json" min-height="80px" />
    </div>
    <p v-if="error" class="text-sm text-rose-600">{{ error }}</p>
    <div v-if="result" class="space-y-2 rounded-lg border border-zinc-200 p-3">
      <div class="flex items-center gap-2 text-sm">
        <StatusCode :status="result.status" />
        <span class="text-xs text-zinc-500">{{ result.ms }} ms</span>
      </div>
      <details class="text-xs">
        <summary class="cursor-pointer text-zinc-500">Headers ({{ result.headers.length }})</summary>
        <div v-for="([k, v], i) in result.headers" :key="i" class="font-mono"><span class="text-zinc-500">{{ k }}:</span> {{ v }}</div>
      </details>
      <pre class="max-h-80 overflow-auto rounded bg-zinc-50 p-2 text-xs">{{ result.body || '(sin body)' }}</pre>
    </div>
  </div>
</template>
