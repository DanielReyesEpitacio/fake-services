import type { ThemeDefinition, VuetifyOptions } from 'vuetify'

export type ThemeMode = 'light' | 'dark' | 'system'
export type ThemeColors = NonNullable<ThemeDefinition['colors']>
export type ThemeVariables = NonNullable<ThemeDefinition['variables']>
export type ComponentDefaults = NonNullable<VuetifyOptions['defaults']>

/** Company branding shared by both modes; mode-specific colors take precedence. */
export interface EnterpriseThemeOptions {
  colors?: ThemeColors
  light?: { colors?: ThemeColors; variables?: ThemeVariables }
  dark?: { colors?: ThemeColors; variables?: ThemeVariables }
  /** Native Vuetify component defaults, merged per component. */
  defaults?: ComponentDefaults
}

/** A native createVuetify configuration with both built-in mode names. */
export interface EnterpriseTheme extends VuetifyOptions {
  theme: {
    themes: Record<Exclude<ThemeMode, 'system'>, ThemeDefinition>
  }
  defaults: ComponentDefaults
}

export interface EnterprisePreset {
  light: ThemeDefinition
  dark: ThemeDefinition
  defaults: ComponentDefaults
}
