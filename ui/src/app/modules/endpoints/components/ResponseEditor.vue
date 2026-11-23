<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import CodeEditor from '@/app/shared/components/CodeEditor.vue'
import KeyValueEditor from '@/app/shared/components/KeyValueEditor.vue'
import type { ResponseBody, ResponseDef } from '@/infrastructure/api/types.ts'

const model = defineModel<ResponseDef>({ required: true })
const { t } = useI18n()

/** Registro de editores con JSON inválido (lo provee la página del endpoint). */
const invalid = inject<Set<symbol>>('invalidJson', new Set())
const uid = Symbol('response-editor')

const kinds: ResponseBody['type'][] = ['json', 'text', 'binary', 'file', 'none']

const body = computed<ResponseBody>(() => model.value.body ?? { type: 'none' })
const jsonText = ref('')
const jsonError = ref('')

if (body.value.type === 'json') jsonText.value = JSON.stringify(body.value.content, null, 2)

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

const kind = computed({
  get: () => body.value.type,
  set: (k: ResponseBody['type']) => {
    invalid.delete(uid)
    jsonError.value = ''
    if (k === 'json') {
      model.value.body = { type: 'json', content: {} }
      jsonText.value = '{}'
    } else if (k === 'text') model.value.body = { type: 'text', content: '' }
    else if (k === 'binary') model.value.body = { type: 'binary', base64: '' }
    else if (k === 'file') model.value.body = { type: 'file', path: '' }
    else model.value.body = { type: 'none' }
  },
})

const textLang = computed(() => {
  const b = body.value
  if (b.type !== 'text') return 'text'
  return (b.contentType ?? '').includes('xml') || b.content.trimStart().startsWith('<') ? 'xml' : 'text'
})

const num = (v: unknown) => {
  const n = Number(v)
  return Number.isFinite(n) && n > 0 ? Math.round(n) : 0
}
</script>

<template>
  <div class="d-flex flex-column gap-3">
    <VRow dense>
      <VCol cols="4">
        <FTextField
          :model-value="model.status"
          label="Código HTTP"
          type="number"
          min="100"
          max="599"
          class="mono"
          hide-details
          @update:model-value="model.status = Number($event) || model.status"
        />
      </VCol>
      <VCol cols="4">
        <FTextField
          :model-value="model.delayMs ?? 0"
          label="Retraso (ms)"
          type="number"
          min="0"
          hide-details
          @update:model-value="model.delayMs = num($event)"
        />
      </VCol>
      <VCol cols="4">
        <FTextField
          :model-value="model.jitterMs ?? 0"
          label="Variación aleatoria (ms)"
          type="number"
          min="0"
          hide-details
          @update:model-value="model.jitterMs = num($event)"
        />
      </VCol>
    </VRow>

    <div>
      <div class="text-label-medium text-medium-emphasis mb-1">Headers de respuesta</div>
      <KeyValueEditor v-model="model.headers" key-placeholder="x-request-id" value-placeholder="{{ uuid() }}" add-label="Agregar header" />
    </div>

    <div>
      <div class="d-flex flex-wrap align-center justify-space-between gap-2 mb-2">
        <span class="text-label-medium text-medium-emphasis">Body</span>
        <VChipGroup v-model="kind" mandatory selected-class="text-primary" column>
          <VChip v-for="k in kinds" :key="k" :value="k" size="small" filter>{{ t(`responseBody.${k}`) }}</VChip>
        </VChipGroup>
      </div>

      <template v-if="body.type === 'json'">
        <CodeEditor v-model="jsonText" language="json" />
        <div v-if="jsonError" class="text-body-small text-error mt-1">JSON inválido: {{ jsonError }}</div>
        <div v-else class="text-body-small text-medium-emphasis mt-1">
          Usa plantillas como <code class="inline" v-pre>"{{ request.params.id }}"</code>. Si el texto es solo
          <code class="inline" v-pre>{{ expr }}</code>, se conserva el tipo (número, lista, objeto).
        </div>
      </template>

      <template v-else-if="model.body?.type === 'text'">
        <CodeEditor v-model="model.body.content" :language="textLang" />
        <FTextField
          :model-value="model.body.contentType ?? ''"
          label="Content-Type"
          placeholder="automático (text/plain, application/xml…)"
          class="mono mt-3"
          hide-details
          @update:model-value="model.body.contentType = $event || null"
        />
      </template>

      <template v-else-if="model.body?.type === 'binary'">
        <FTextarea v-model="model.body.base64" label="Contenido en base64" class="mono" rows="4" hide-details />
        <FTextField
          :model-value="model.body.contentType ?? ''"
          label="Content-Type"
          placeholder="application/octet-stream"
          class="mono mt-3"
          hide-details
          @update:model-value="model.body.contentType = $event || null"
        />
      </template>

      <template v-else-if="model.body?.type === 'file'">
        <FTextField
          v-model="model.body.path"
          label="Archivo"
          placeholder="contratos/{{ request.params.id }}.pdf"
          class="mono"
          hint="Ruta relativa a la carpeta files/ del espacio de trabajo. Admite plantillas."
          persistent-hint
        />
      </template>

      <div v-else class="text-body-small text-medium-emphasis">La respuesta no tendrá body.</div>
    </div>
  </div>
</template>
