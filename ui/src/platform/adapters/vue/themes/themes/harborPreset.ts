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

/** Soft organic workspace: 24px geometry, generous rhythm and flat green-tinted surfaces. */
export const harborPreset: EnterprisePreset = {
  light: {
    colors: {
      navigation: '#D0E7DD',
      'on-navigation': '#204639',
      header: '#E6F2EA',
      input: '#E4F0E8',
      ...lightStatus,
      primary: '#006B60',
      secondary: '#526A86',
      background: '#E3EFE7',
      surface: '#F8FCF8',
      'surface-light': '#DCEDE2',
      'surface-bright': '#FFFFFF',
      'surface-variant': '#D6E7E1',
      'on-background': '#18332D',
      'on-surface': '#18332D',
      'on-surface-variant': '#39594F',
    },
    variables: {
      'font-body': 'Segoe UI, sans-serif',
      'font-heading': 'Trebuchet MS, sans-serif',
      'border-color': '#28564A',
      'border-opacity': 0.12,
      'hover-opacity': 0.04,
      'selected-opacity': 0.08,
    },
  },
  dark: {
    colors: {
      navigation: '#102D24',
      'on-navigation': '#CBEADB',
      header: '#17382D',
      input: '#26483B',
      ...darkStatus,
      primary: '#75D8C4',
      secondary: '#B2C7E3',
      background: '#101C19',
      surface: '#1A2C26',
      'surface-light': '#273D35',
      'surface-bright': '#344F44',
      'surface-variant': '#3C594D',
      'on-background': '#E1F0E8',
      'on-surface': '#E1F0E8',
      'on-surface-variant': '#CEE5D8',
    },
    variables: {
      'font-body': 'Segoe UI, sans-serif',
      'font-heading': 'Trebuchet MS, sans-serif',
      'border-color': '#B6D4C5',
      'border-opacity': 0.18,
      'hover-opacity': 0.06,
      'selected-opacity': 0.12,
    },
  },
  defaults: createSystemDefaults({
    radius: 24,
    density: 'default',
    space: 6,
    field: 'solo-filled',
    cardBorder: false,
    button: 'flat',
    navigationWidth: 288,
  }),
}
