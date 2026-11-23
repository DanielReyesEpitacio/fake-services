import type { App } from 'vue'
import FCombobox from './FCombobox.vue'
import FSelect from './FSelect.vue'
import FTextField from './FTextField.vue'
import FTextarea from './FTextarea.vue'

/** Registra los campos con etiqueta externa como componentes globales. */
export function registerFields(app: App) {
  app.component('FTextField', FTextField)
  app.component('FSelect', FSelect)
  app.component('FCombobox', FCombobox)
  app.component('FTextarea', FTextarea)
}

declare module 'vue' {
  export interface GlobalComponents {
    FTextField: typeof FTextField
    FSelect: typeof FSelect
    FCombobox: typeof FCombobox
    FTextarea: typeof FTextarea
  }
}
