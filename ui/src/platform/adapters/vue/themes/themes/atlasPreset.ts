import { createSystemDefaults } from './systemDefaults.ts'
import type { EnterprisePreset } from './types.ts'

const lightStatus = {
  success: '#18794E',
  warning: '#9A6700',
  error: '#C62828',
  info: '#1565C0',
}
const darkStatus = {
  success: '#6ED6A0',
  warning: '#F5CC70',
  error: '#FF9B9B',
  info: '#90CAF9',
}

/** Angular operational console: square geometry, compact rhythm and contrasting navy navigation. */
export const atlasPreset: EnterprisePreset = {
  light: {
    colors: {
      navigation: '#14263D',
      'on-navigation': '#E4EDFA',
      header: '#FFFFFF',
      input: '#FFFFFF',
      ...lightStatus,
      primary: '#2457A7',
      secondary: '#54657B',
      background: '#F3F5F9',
      surface: '#FFFFFF',
      'surface-light': '#E9EEF5',
      'surface-bright': '#FFFFFF',
      'surface-variant': '#DEE5EF',
      'on-background': '#182438',
      'on-surface': '#182438',
      'on-surface-variant': '#34465F',
    },
    variables: {
      'font-body': 'Segoe UI, sans-serif',
      'font-heading': 'Consolas, monospace',
      'border-color': '#24364D',
      'border-opacity': 0.16,
      'hover-opacity': 0.04,
      'selected-opacity': 0.08,
    },
  },
  dark: {
    colors: {
      navigation: '#0A1320',
      'on-navigation': '#DCE8FA',
      header: '#172334',
      input: '#142031',
      ...darkStatus,
      primary: '#A6C8FF',
      secondary: '#B5C5DA',
      background: '#101722',
      surface: '#1A2433',
      'surface-light': '#263449',
      'surface-bright': '#30415A',
      'surface-variant': '#34465F',
      'on-background': '#E5ECF6',
      'on-surface': '#E5ECF6',
      'on-surface-variant': '#D2DEEE',
    },
    variables: {
      'font-body': 'Segoe UI, sans-serif',
      'font-heading': 'Consolas, monospace',
      'border-color': '#B5C5DA',
      'border-opacity': 0.22,
      'hover-opacity': 0.06,
      'selected-opacity': 0.12,
    },
  },
  defaults: createSystemDefaults({
    radius: 0,
    density: 'compact',
    space: 4,
    field: 'outlined',
    cardBorder: true,
    button: 'flat',
    navigationWidth: 248,
  }),
}
