// Apariencia: look and feel, modo de color y acento, elegidos por el usuario.
//
// El look and feel se aplica al arrancar (cada uno trae sus defaults y su CSS
// aislado); cambiarlo guarda la preferencia y recarga la página. El modo de
// color se cambia en caliente con la API nativa de Vuetify.

import { reactive, readonly } from 'vue'
import { useTheme } from 'vuetify'
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import type { ThemeMode } from '@/platform/adapters/vue/themes'
import {
  DEFAULT_ACCENT,
  DEFAULT_LOOK_AND_FEEL,
  findAccent,
  findLookAndFeel,
  type LookAndFeelId,
} from './catalog.ts'

export * from './catalog.ts'

const STORAGE_KEY = 'fakes.appearance'

export interface AppearancePreferences {
  lookAndFeel: LookAndFeelId
  mode: ThemeMode
  accent: string
}

const defaults: AppearancePreferences = {
  lookAndFeel: DEFAULT_LOOK_AND_FEEL,
  mode: 'system',
  accent: DEFAULT_ACCENT,
}

export function loadPreferences(): AppearancePreferences {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<AppearancePreferences>
    return {
      lookAndFeel: findLookAndFeel(raw.lookAndFeel ?? defaults.lookAndFeel).id as LookAndFeelId,
      mode: raw.mode === 'light' || raw.mode === 'dark' || raw.mode === 'system' ? raw.mode : defaults.mode,
      accent: findAccent(raw.accent ?? defaults.accent).id,
    }
  } catch {
    return { ...defaults }
  }
}

function savePreferences(p: AppearancePreferences) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(p))
  } catch {
    /* almacenamiento no disponible: se usa la preferencia solo en esta sesión */
  }
}

const state = reactive<AppearancePreferences>(loadPreferences())

/** Crea la instancia de Vuetify con la apariencia guardada. */
export async function createAppearance() {
  const prefs = loadPreferences()
  const factory = await findLookAndFeel(prefs.lookAndFeel).load()
  const accent = findAccent(prefs.accent)
  const blueprint = factory({
    light: accent.light ? { colors: accent.light } : undefined,
    dark: accent.dark ? { colors: accent.dark } : undefined,
  })
  document.documentElement.dataset.lookAndFeel = prefs.lookAndFeel
  return createPlatformTheme({ blueprint, theme: { defaultTheme: prefs.mode } })
}

/** Preferencias de apariencia y acciones para cambiarlas. */
export function useAppearance() {
  const theme = useTheme()

  function setMode(mode: ThemeMode) {
    state.mode = mode
    savePreferences({ ...state })
    theme.change(mode)
  }

  /** Cambia look and feel y/o acento; requiere recargar para aplicar sus defaults. */
  function apply(changes: Partial<Pick<AppearancePreferences, 'lookAndFeel' | 'accent'>>) {
    Object.assign(state, changes)
    savePreferences({ ...state })
    location.reload()
  }

  return { preferences: readonly(state), setMode, apply, isDark: () => theme.current.value.dark }
}
