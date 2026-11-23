import 'vuetify/styles'
import '@mdi/font/css/materialdesignicons.css'
import { createVuetify } from 'vuetify'
import type { VuetifyOptions } from 'vuetify'
import * as components from 'vuetify/components'
import * as directives from 'vuetify/directives'
import { createAtlasLookAndFeel } from './atlas.ts'

/** Native Vuetify options and instance, with the platform appearance as a blueprint. */
export function createPlatformTheme(options: VuetifyOptions = {}) {
  return createVuetify({
    blueprint: createAtlasLookAndFeel(),
    ...options,
    components: { ...components, ...options.components },
    directives: { ...directives, ...options.directives },
  })
}
