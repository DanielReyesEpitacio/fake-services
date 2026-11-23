import type { ComponentDefaults } from './types.ts'

interface SystemRules {
  radius: 0 | 6 | 8 | 24
  density: 'compact' | 'comfortable' | 'default'
  space: 4 | 5 | 6
  field: 'outlined' | 'solo-filled'
  cardBorder: boolean
  button: 'flat' | 'outlined'
  navigationWidth: number
}

/** Applies one shape, density and spacing language to every supported surface. */
export function createSystemDefaults(rules: SystemRules): ComponentDefaults {
  const { radius: rounded, density, space, field, cardBorder, button, navigationWidth } = rules
  const input = {
    rounded,
    density,
    variant: field,
    color: 'primary',
    flat: true,
    singleLine: true,
    hideDetails: 'auto',
    bgColor: 'input',
  }
  return {
    global: { density },
    VBtn: {
      rounded,
      density: 'default',
      height: density === 'compact' ? 36 : 44,
      variant: button,
      color: 'primary',
      elevation: 0,
      ripple: false,
      class: 'text-none font-weight-bold',
    },
    VCard: { rounded, variant: 'flat', border: cardBorder, elevation: 0, color: 'surface' },
    VCardTitle: {
      class: `px-${space} pt-${space} pb-2 text-title-large font-weight-bold text-wrap`,
    },
    VCardSubtitle: { class: `px-${space} pb-2 text-body-medium` },
    VCardText: { class: `pa-${space}` },
    VCardActions: { class: `pa-${space} pt-0` },
    VTextField: { ...input },
    VSelect: { ...input },
    VAutocomplete: { ...input },
    VTextarea: { ...input },
    VField: { rounded },
    VDataTable: { density, hover: true, rounded, color: 'primary' },
    VTable: { density, hover: true, rounded },
    VList: { density, bgColor: 'transparent', color: 'primary', rounded: 0 },
    VListItem: {
      rounded,
      density,
      color: 'primary',
      variant: 'text',
      ripple: false,
    },
    VChip: { rounded, variant: cardBorder ? 'outlined' : 'tonal', size: 'small' },
    VAvatar: { rounded, size: 40, variant: 'tonal' },
    VAlert: { rounded, variant: cardBorder ? 'outlined' : 'tonal', density },
    VProgressLinear: { rounded, height: 6, color: 'primary' },
    VCheckbox: {
      color: 'primary',
      density,
      ripple: false,
      hideDetails: true,
      trueIcon: rounded === 24 ? 'mdi-check-circle' : 'mdi-checkbox-marked',
      falseIcon: rounded === 24 ? 'mdi-circle-outline' : 'mdi-checkbox-blank-outline',
    },
    VSwitch: { color: 'primary', density, flat: true, ripple: false, inset: true },
    VTabs: { color: 'primary' },
    VTab: { rounded, ripple: false, class: 'text-none' },
    VAppBar: {
      color: 'header',
      elevation: 0,
      border: cardBorder,
      height: density === 'compact' ? 64 : 80,
    },
    VNavigationDrawer: {
      color: 'navigation',
      elevation: 0,
      border: cardBorder,
      width: navigationWidth,
    },
    VMenu: { rounded },
    VSheet: { rounded },
    VDialog: { rounded },
  }
}
