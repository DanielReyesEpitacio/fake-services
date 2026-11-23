import { jetbrainsGlassBase } from './jetbrainsGlassBase.ts'
import type { EnterprisePreset } from './types.ts'

const defaults = jetbrainsGlassBase.defaults

/** Compact editor geometry over frosted graphite, violet and cyan surfaces. */
export const jetbrainsGlassPreset: EnterprisePreset = {
  light: {
    colors: {
      ...jetbrainsGlassBase.light.colors,
      background: '#EEF0F8',
      surface: '#FFFFFF',
      navigation: '#E9ECF6',
      header: '#F2F3FA',
      input: '#FAFAFF',
      'glass-violet': '#B39AEF',
      'glass-cyan': '#7CCEDB',
      'glass-pink': '#EBA9D2',
    },
    variables: {
      ...jetbrainsGlassBase.light.variables,
      'jb-glass-opacity': 0.62,
      'jb-glass-stroke': 0.7,
      'jb-glass-glow': 0.44,
      'jb-glass-shadow': 0.12,
    },
  },
  dark: {
    colors: {
      ...jetbrainsGlassBase.dark.colors,
      background: '#14151E',
      surface: '#292A3B',
      navigation: '#202130',
      header: '#222332',
      input: '#191B29',
      'glass-violet': '#6D42C1',
      'glass-cyan': '#146A7D',
      'glass-pink': '#963F7E',
    },
    variables: {
      ...jetbrainsGlassBase.dark.variables,
      'jb-glass-opacity': 0.66,
      'jb-glass-stroke': 0.14,
      'jb-glass-glow': 0.36,
      'jb-glass-shadow': 0.36,
    },
  },
  defaults: {
    ...defaults,
    VApp: { class: 'lf-jb-base lf-jetbrains-glass' },
    VCard: { ...defaults.VCard, rounded: 8, border: false, class: 'lf-jb-glass-panel' },
    VCardTitle: {
      ...defaults.VCardTitle,
      class: `${defaults.VCardTitle?.class} lf-jb-glass-title`,
    },
    VBtn: { ...defaults.VBtn, class: `${defaults.VBtn?.class} lf-jb-glass-button` },
    VNavigationDrawer: {
      ...defaults.VNavigationDrawer,
      border: false,
      class: 'lf-jb-glass-navigation',
    },
    VAppBar: { ...defaults.VAppBar, border: false, class: 'lf-jb-glass-appbar' },
    VSelect: { ...defaults.VSelect, menuProps: { contentClass: 'lf-jb-glass-menu' } },
    VAutocomplete: { ...defaults.VAutocomplete, menuProps: { contentClass: 'lf-jb-glass-menu' } },
    VCombobox: { ...defaults.VAutocomplete, menuProps: { contentClass: 'lf-jb-glass-menu' } },
    VMenu: { rounded: 8, contentClass: 'lf-jb-glass-menu' },
    VDialog: { rounded: 10, contentClass: 'lf-jb-glass-dialog' },
    VTooltip: { contentClass: 'lf-jb-glass-tooltip' },
  },
}
