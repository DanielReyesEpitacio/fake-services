// Catálogo de apariencias disponibles. Cada look and feel se carga con un
// import dinámico para conservar el aislamiento de CSS de su entrada.

import type { VuetifyOptions } from 'vuetify'
import type { EnterpriseThemeOptions } from '@/platform/adapters/vue/themes'

type Factory = (options?: EnterpriseThemeOptions) => VuetifyOptions

export interface LookAndFeelEntry {
  id: string
  name: string
  description: string
  /** Colores de muestra para la vista previa: fondo, superficie, acento. */
  swatch: [string, string, string]
  radius: number
  load: () => Promise<Factory>
}

export const LOOK_AND_FEELS = [
  {
    id: 'cupertinoGlass',
    name: 'Cupertino Glass',
    description: 'Vidrio esmerilado, paneles flotantes y tipografía Manrope.',
    swatch: ['#D9DDE2', '#F5F6F8', '#0066CC'],
    radius: 20,
    load: () => import('@/platform/adapters/vue/themes/cupertinoGlass').then((m) => m.createCupertinoGlassLookAndFeel),
  },
  {
    id: 'fluent',
    name: 'Fluent',
    description: 'Fondo luminoso, tarjetas de vidrio y menús acrílicos.',
    swatch: ['#EDF3FA', '#F1F5FC', '#075BCD'],
    radius: 20,
    load: () => import('@/platform/adapters/vue/themes/fluent').then((m) => m.createFluentLookAndFeel),
  },
  {
    id: 'jetbrains',
    name: 'JetBrains',
    description: 'Densidad de IDE: paneles de 2px, controles de 32px y títulos monoespaciados.',
    swatch: ['#F0F1F4', '#F7F8FA', '#6B3EC7'],
    radius: 2,
    load: () => import('@/platform/adapters/vue/themes/jetbrains').then((m) => m.createJetBrainsLookAndFeel),
  },
  {
    id: 'jetbrainsGlass',
    name: 'JetBrains Glass',
    description: 'La densidad de un IDE con materiales translúcidos.',
    swatch: ['#EEF0F8', '#F2F3FA', '#6B3EC7'],
    radius: 8,
    load: () => import('@/platform/adapters/vue/themes/jetbrainsGlass').then((m) => m.createJetBrainsGlassLookAndFeel),
  },
  {
    id: 'atlas',
    name: 'Atlas',
    description: 'Consola angular: sin radios, navegación oscura y densidad compacta.',
    swatch: ['#F3F5F9', '#FFFFFF', '#2457A7'],
    radius: 0,
    load: () => import('@/platform/adapters/vue/themes/atlas').then((m) => m.createAtlasLookAndFeel),
  },
  {
    id: 'atlassian',
    name: 'Atlassian',
    description: 'Limpio y funcional, inspirado en herramientas de equipo.',
    swatch: ['#F1F2F4', '#FFFFFF', '#0C66E4'],
    radius: 6,
    load: () => import('@/platform/adapters/vue/themes/atlassian').then((m) => m.createAtlassianLookAndFeel),
  },
  {
    id: 'meridian',
    name: 'Meridian',
    description: 'Editorial: papel cálido, títulos serif y contornos finos.',
    swatch: ['#F4EFE4', '#F8F4EB', '#773F35'],
    radius: 8,
    load: () => import('@/platform/adapters/vue/themes/meridian').then((m) => m.createMeridianLookAndFeel),
  },
  {
    id: 'harbor',
    name: 'Harbor',
    description: 'Orgánico: radios amplios, verdes suaves y sin sombras.',
    swatch: ['#E3EFE7', '#E6F2EA', '#006B60'],
    radius: 24,
    load: () => import('@/platform/adapters/vue/themes/harbor').then((m) => m.createHarborLookAndFeel),
  },
  {
    id: 'airbnb',
    name: 'Airbnb',
    description: 'Tarjetas amplias con sombras suaves y chips tipo píldora.',
    swatch: ['#F7F7F7', '#FFFFFF', '#D70466'],
    radius: 24,
    load: () => import('@/platform/adapters/vue/themes/airbnb').then((m) => m.createAirbnbLookAndFeel),
  },
] as const satisfies readonly LookAndFeelEntry[]

export type LookAndFeelId = (typeof LOOK_AND_FEELS)[number]['id']

export const DEFAULT_LOOK_AND_FEEL: LookAndFeelId = 'cupertinoGlass'

export function findLookAndFeel(id: string): LookAndFeelEntry {
  return LOOK_AND_FEELS.find((l) => l.id === id) ?? LOOK_AND_FEELS[0]
}

// -- color de acento --------------------------------------------------------

export interface AccentEntry {
  id: string
  name: string
  /** `null` = conservar el color del look and feel. */
  light: { primary: string; 'on-primary': string } | null
  dark: { primary: string; 'on-primary': string } | null
}

export const ACCENTS: readonly AccentEntry[] = [
  {
    id: 'fakes',
    name: 'Violeta Fake Services',
    light: { primary: '#7C3AED', 'on-primary': '#FFFFFF' },
    dark: { primary: '#C4B5FD', 'on-primary': '#2E1065' },
  },
  { id: 'native', name: 'Del look and feel', light: null, dark: null },
  {
    id: 'emerald',
    name: 'Esmeralda',
    light: { primary: '#047857', 'on-primary': '#FFFFFF' },
    dark: { primary: '#6EE7B7', 'on-primary': '#022C22' },
  },
  {
    id: 'amber',
    name: 'Ámbar',
    light: { primary: '#B45309', 'on-primary': '#FFFFFF' },
    dark: { primary: '#FCD34D', 'on-primary': '#3B2300' },
  },
  {
    id: 'rose',
    name: 'Rosa',
    light: { primary: '#BE123C', 'on-primary': '#FFFFFF' },
    dark: { primary: '#FDA4AF', 'on-primary': '#4C0519' },
  },
]

export const DEFAULT_ACCENT = 'fakes'

export function findAccent(id: string): AccentEntry {
  return ACCENTS.find((a) => a.id === id) ?? ACCENTS[0]
}
