<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { EditorView, basicSetup } from 'codemirror'
import { Compartment, EditorState } from '@codemirror/state'
import { json } from '@codemirror/lang-json'
import { xml } from '@codemirror/lang-xml'

const props = withDefaults(
  defineProps<{ language?: 'json' | 'xml' | 'text'; readonly?: boolean; minHeight?: string; label?: string }>(),
  { language: 'json', readonly: false, minHeight: '120px', label: undefined },
)
const model = defineModel<string>({ required: true })

const host = ref<HTMLDivElement>()
let view: EditorView | undefined
const lang = new Compartment()
const langExt = (l: string) => (l === 'json' ? json() : l === 'xml' ? xml() : [])

// Colores tomados del tema activo de Vuetify: se adapta al look and feel y al modo.
const theme = EditorView.theme({
  '&': {
    fontSize: '13px',
    color: 'rgb(var(--v-theme-on-surface))',
    backgroundColor: 'rgba(var(--v-theme-on-surface), 0.04)',
    border: '1px solid rgba(var(--v-border-color), var(--v-border-opacity))',
    borderRadius: '10px',
  },
  '&.cm-focused': { outline: 'none', borderColor: 'rgb(var(--v-theme-primary))' },
  '.cm-scroller': {
    fontFamily: "'JetBrains Mono', ui-monospace, monospace",
    minHeight: props.minHeight,
    maxHeight: '480px',
  },
  '.cm-content': { caretColor: 'rgb(var(--v-theme-primary))' },
  '.cm-gutters': {
    backgroundColor: 'transparent',
    color: 'rgba(var(--v-theme-on-surface), 0.45)',
    borderRight: '1px solid rgba(var(--v-border-color), var(--v-border-opacity))',
  },
  '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'rgba(var(--v-theme-primary), 0.06)' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
    backgroundColor: 'rgba(var(--v-theme-primary), 0.2) !important',
  },
})

onMounted(() => {
  view = new EditorView({
    parent: host.value!,
    state: EditorState.create({
      doc: model.value,
      extensions: [
        basicSetup,
        theme,
        lang.of(langExt(props.language)),
        EditorView.lineWrapping,
        EditorState.readOnly.of(props.readonly),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) model.value = u.state.doc.toString()
        }),
      ],
    }),
  })
})

watch(model, (v) => {
  if (view && v !== view.state.doc.toString()) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } })
  }
})
watch(
  () => props.language,
  (l) => view?.dispatch({ effects: lang.reconfigure(langExt(l)) }),
)
onBeforeUnmount(() => view?.destroy())
</script>

<template>
  <div>
    <div v-if="label" class="text-label-medium text-medium-emphasis mb-1">{{ label }}</div>
    <div ref="host" />
  </div>
</template>
