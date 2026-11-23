<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { EditorView, basicSetup } from 'codemirror'
import { Compartment, EditorState } from '@codemirror/state'
import { json } from '@codemirror/lang-json'
import { xml } from '@codemirror/lang-xml'

const props = withDefaults(
  defineProps<{ language?: 'json' | 'xml' | 'text'; readonly?: boolean; minHeight?: string }>(),
  { language: 'json', readonly: false, minHeight: '120px' },
)
const model = defineModel<string>({ required: true })

const host = ref<HTMLDivElement>()
let view: EditorView | undefined
const lang = new Compartment()

const langExt = (l: string) => (l === 'json' ? json() : l === 'xml' ? xml() : [])

const theme = EditorView.theme({
  '&': { fontSize: '13px', border: '1px solid #d4d4d8', borderRadius: '8px', backgroundColor: '#fff' },
  '&.cm-focused': { outline: 'none', borderColor: '#8b5cf6', boxShadow: '0 0 0 3px #ede9fe' },
  '.cm-scroller': { fontFamily: "'JetBrains Mono', ui-monospace, monospace", minHeight: props.minHeight, maxHeight: '480px' },
  '.cm-gutters': { backgroundColor: '#fafafa', borderRight: '1px solid #f4f4f5', borderRadius: '8px 0 0 8px' },
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
  <div ref="host" />
</template>
