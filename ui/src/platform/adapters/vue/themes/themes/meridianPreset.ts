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

/** Warm editorial workspace: 8px geometry, paper surfaces and fine outlined controls. */
export const meridianPreset: EnterprisePreset = {
  light: {
    colors: {
      navigation: '#EAE2D6',
      'on-navigation': '#423B31',
      header: '#F8F4EB',
      input: '#FFFCF5',
      ...lightStatus,
      primary: '#773F35',
      secondary: '#806D42',
      background: '#F4EFE4',
      surface: '#FFFCF6',
      'surface-light': '#ECE4D6',
      'surface-bright': '#FFFFFF',
      'surface-variant': '#E3E1DB',
      'on-background': '#292B37',
      'on-surface': '#292B37',
      'on-surface-variant': '#555460',
    },
    variables: {
      'font-body': 'Segoe UI, sans-serif',
      'font-heading': 'Georgia, serif',
      'border-color': '#464655',
      'border-opacity': 0.2,
      'hover-opacity': 0.03,
      'selected-opacity': 0.07,
    },
  },
  dark: {
    colors: {
      navigation: '#272320',
      'on-navigation': '#EFE4D5',
      header: '#211E1B',
      input: '#302A25',
      ...darkStatus,
      primary: '#E7B2A2',
      secondary: '#DCC295',
      background: '#181613',
      surface: '#25211D',
      'surface-light': '#303039',
      'surface-bright': '#3D3D48',
      'surface-variant': '#44434F',
      'on-background': '#EFEDF2',
      'on-surface': '#EFEDF2',
      'on-surface-variant': '#DED9E4',
    },
    variables: {
      'font-body': 'Segoe UI, sans-serif',
      'font-heading': 'Georgia, serif',
      'border-color': '#D0CBD6',
      'border-opacity': 0.24,
      'hover-opacity': 0.05,
      'selected-opacity': 0.1,
    },
  },
  defaults: createSystemDefaults({
    radius: 8,
    density: 'comfortable',
    space: 5,
    field: 'outlined',
    cardBorder: true,
    button: 'outlined',
    navigationWidth: 272,
  }),
}
