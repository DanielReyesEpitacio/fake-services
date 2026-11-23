import { createApp } from 'vue'
import { createI18n } from 'vue-i18n'
import '@fontsource/jetbrains-mono/400.css'
import '@fontsource/jetbrains-mono/500.css'
import App from './App.vue'
import router from '@/app/configuration/router'
import { createAppearance } from '@/app/configuration/appearance'
import { registerFields } from '@/app/shared/components/fields'
import es from './locales/es.ts'
import './app/shared/styles/app.css'

async function initApp() {
  const i18n = createI18n({ legacy: false, locale: 'es', fallbackLocale: 'es', messages: { es } })
  const vuetify = await createAppearance()
  const app = createApp(App).use(router).use(i18n).use(vuetify)
  registerFields(app)
  app.mount('#app')
}

void initApp()
