<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import CodeEditor from './CodeEditor.vue'
import KeyValueEditor from './KeyValueEditor.vue'
import type { ResponseBody, ResponseDef } from '../lib/types'

const model = defineModel<ResponseDef>({ required: true })
const { t } = useI18n()

/** Registro de editores con JSON inválido (lo provee la vista del endpoint). */
const invalid = inject<Set<symbol>>('invalidJson', new Set())
const uid = Symbol('response-editor')

const kinds: ResponseBody['type'][] = ['json', 'text', 'binary', 'file', 'none']
const commonStatus = [200, 201, 202, 204, 400, 401, 403, 404, 409, 422, 429, 500, 502, 503]

const body = computed<ResponseBody>(() => model.value.body ?? { type: 'none' })

const jsonText = ref('')
const jsonError = ref('')

function loadJson() {
  if (body.value.type === 'json') {
    jsonText.value = JSON.stringify(body.value.content, null, 2)
    jsonError.value = ''
    invalid.delete(uid)
  }
}
loadJson()

watch(jsonText, (txt) => {
  if (body.value.type !== 'json') return
  try {
    const parsed = txt.trim() === '' ? null : JSON.parse(txt)
    jsonError.value = ''
    invalid.delete(uid)
    if (JSON.stringify(parsed) !== JSON.stringify(body.value.content)) {
      model.value.body = { type: 'json', content: parsed }
    }
  } catch (e) {
    jsonError.value = (e as Error).message
    invalid.add(uid)
  }
})

onBeforeUnmount(() => invalid.delete(uid))

function setKind(kind: ResponseBody['type']) {
  invalid.delete(uid)
  jsonError.value = ''
  switch (kind) {
    case 'json':
      model.value.body = { type: 'json', content: {} }
      jsonText.value = '{}'
      break
    case 'text':
      model.value.body = { type: 'text', content: '' }
      break
    case 'binary':
      model.value.body = { type: 'binary', base64: '' }
      break
    case 'file':
      model.value.body = { type: 'file', path: '' }
      break
    default:
      model.value.body = { type: 'none' }
  }
}

const textLang = computed(() => {
  const b = body.value
  if (b.type !== 'text') return 'text'
  const ct = b.contentType ?? ''
  return ct.includes('xml') || b.content.trimStart().startsWith('<') ? 'xml' : 'text'
})

function setNum(key: 'delayMs' | 'jitterMs', v: string) {
  const n = Number(v)
  model.value[key] = Number.isFinite(n) && n > 0 ? Math.round(n) : 0
}
</script>

<template>
  <div class="space-y-3">
    <div class="grid grid-cols-3 gap-3">
      <div>
        <label class="label">Código HTTP</label>
        <input v-model.number="model.status" type="number" min="100" max="599" class="input-mono" list="status-codes" />
        <datalist id="status-codes">
          <option v-for="s in commonStatus" :key="s" :value="s" />
        </datalist>
      </div>
      <div>
        <label class="label">Retraso (ms)</label>
        <input :value="model.delayMs ?? 0" type="number" min="0" class="input-mono" @input="setNum('delayMs', ($event.target as HTMLInputElement).value)" />
      </div>
      <div>
        <label class="label">Variación aleatoria (ms)</label>
        <input :value="model.jitterMs ?? 0" type="number" min="0" class="input-mono" @input="setNum('jitterMs', ($event.target as HTMLInputElement).value)" />
      </div>
    </div>

    <div>
      <label class="label">Headers de respuesta</label>
      <KeyValueEditor v-model="model.headers" key-placeholder="x-request-id" value-placeholder="{{ uuid() }}" add-label="Agregar header" />
    </div>

    <div>
      <div class="mb-1 flex items-center justify-between">
        <label class="label !mb-0">Body</label>
        <div class="flex gap-1">
          <button
            v-for="k in kinds"
            :key="k"
            type="button"
            class="badge cursor-pointer border"
            :class="body.type === k ? 'border-brand-200 bg-brand-50 text-brand-700' : 'border-transparent text-zinc-500 hover:bg-zinc-100'"
            @click="setKind(k)"
          >
            {{ t(`responseBody.${k}`) }}
          </button>
        </div>
      </div>

      <template v-if="body.type === 'json'">
        <CodeEditor v-model="jsonText" language="json" />
        <p v-if="jsonError" class="mt-1 text-xs text-rose-600">JSON inválido: {{ jsonError }}</p>
        <p v-else class="hint">
          Usa plantillas como <code v-pre>"{{ request.params.id }}"</code>. Si el texto es solo
          <code v-pre>{{ expr }}</code>, se conserva el tipo (número, lista, objeto).
        </p>
      </template>

      <template v-else-if="model.body?.type === 'text'">
        <CodeEditor v-model="model.body.content" :language="textLang" />
        <div class="mt-2 grid grid-cols-2 gap-3">
          <div>
            <label class="label">Content-Type</label>
            <input
              :value="model.body.contentType ?? ''"
              class="input-mono"
              placeholder="automático (text/plain, application/xml…)"
              @input="model.body.contentType = ($event.target as HTMLInputElement).value || null"
            />
          </div>
        </div>
      </template>

      <template v-else-if="model.body?.type === 'binary'">
        <textarea v-model="model.body.base64" class="input-mono h-24" placeholder="Contenido en base64" />
        <input
          :value="model.body.contentType ?? ''"
          class="input-mono mt-2"
          placeholder="application/octet-stream"
          @input="model.body.contentType = ($event.target as HTMLInputElement).value || null"
        />
      </template>

      <template v-else-if="model.body?.type === 'file'">
        <input v-model="model.body.path" class="input-mono" placeholder="contratos/{{ request.params.id }}.pdf" />
        <p class="hint">Ruta relativa a la carpeta <code>files/</code> del espacio de trabajo. Admite plantillas.</p>
      </template>

      <p v-else class="text-xs text-zinc-500">La respuesta no tendrá body.</p>
    </div>
  </div>
</template>
