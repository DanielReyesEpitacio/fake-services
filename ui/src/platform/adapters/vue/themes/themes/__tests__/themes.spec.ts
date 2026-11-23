import { describe, expect, it, vi } from 'vitest'
import { createApp } from 'vue'
import { createPlatformTheme } from '../../createPlatformTheme.ts'
import { createVuetify } from 'vuetify'
import { createAtlasLookAndFeel } from '../../atlas.ts'
import { createMeridianLookAndFeel } from '../../meridian.ts'
import { createHarborLookAndFeel } from '../../harbor.ts'
import { createAtlassianLookAndFeel } from '../../atlassian.ts'
import { createAirbnbLookAndFeel } from '../../airbnb.ts'
import { createJetBrainsLookAndFeel } from '../../jetbrains.ts'
import { createFluentLookAndFeel } from '../../fluent.ts'
import { createJetBrainsGlassLookAndFeel } from '../../jetbrainsGlass.ts'
import { createCupertinoGlassLookAndFeel } from '../../cupertinoGlass.ts'

// Vuetify detects matchMedia support when its module is imported.
vi.hoisted(() => {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => Object.assign(new EventTarget(), { matches: false })),
  )
})

describe.each([
  createAtlasLookAndFeel,
  createMeridianLookAndFeel,
  createHarborLookAndFeel,
  createAtlassianLookAndFeel,
  createAirbnbLookAndFeel,
  createJetBrainsLookAndFeel,
  createFluentLookAndFeel,
  createJetBrainsGlassLookAndFeel,
  createCupertinoGlassLookAndFeel,
])('%s', (factory) => {
  it('uses native component defaults without registering aliases', () => {
    const theme = factory()
    expect(theme.aliases).toBeUndefined()
    expect(theme.defaults.VBtn).toBeDefined()
  })

  it.each(['light', 'dark', 'system'] as const)(
    'uses the native %s mode and theme overrides',
    async (mode) => {
      const appearance = factory()
      const vuetify = createPlatformTheme({
        blueprint: appearance,
        theme: {
          defaultTheme: mode,
          themes: {
            light: { dark: false, colors: { primary: '#123456' } },
            dark: { dark: true, colors: { primary: '#ABCDEF' } },
          },
        },
      })
      expect(vuetify.theme.isSystem.value).toBe(mode === 'system')
      expect(vuetify.theme.name.value).toBe(mode === 'system' ? 'light' : mode)
      expect(vuetify.theme.themes.value.light?.colors.primary).toBe('#123456')
      expect(vuetify.theme.themes.value.dark?.colors.primary).toBe('#ABCDEF')
      expect(vuetify.theme.themes.value.light?.colors.navigation).toBe(
        appearance.theme.themes.light.colors?.navigation,
      )
      expect(vuetify.defaults.value?.VBtn).toEqual(appearance.defaults.VBtn)
      await vuetify.theme.change('dark')
      expect(vuetify.theme.current.value.dark).toBe(true)
      await vuetify.theme.change('light')
      expect(vuetify.theme.current.value.dark).toBe(false)
      await vuetify.theme.change('system')
      expect(vuetify.theme.isSystem.value).toBe(true)
    },
  )

  it('registers working light and dark themes with native foreground colors', async () => {
    const vuetify = createVuetify(factory())
    expect(vuetify.theme.current.value.dark).toBe(false)
    expect(vuetify.theme.current.value.colors['on-primary']).toBeTruthy()
    await vuetify.theme.change('dark')
    expect(vuetify.theme.current.value.dark).toBe(true)
    expect(vuetify.theme.current.value.colors['on-primary']).toBeTruthy()
  })

  it('applies brand colors, mode overrides and partial component defaults', () => {
    const original = factory()
    const theme = factory({
      colors: { primary: '#123456', secondary: '#654321' },
      dark: { colors: { primary: '#ABCDEF' }, variables: { 'border-opacity': 0.3 } },
      defaults: { VBtn: { rounded: 'lg' } },
    })
    expect(theme.theme.themes.light.colors?.primary).toBe('#123456')
    expect(theme.theme.themes.dark.colors?.primary).toBe('#ABCDEF')
    expect(theme.theme.themes.dark.colors?.secondary).toBe('#654321')
    expect(theme.theme.themes.dark.variables?.['border-opacity']).toBe(0.3)
    expect(theme.defaults.VBtn).toEqual({ ...original.defaults.VBtn, rounded: 'lg' })
    expect(factory()).toEqual(original)
  })

  it('keeps subsequent instances independent and preserves function props', () => {
    const theme = factory()
    theme.theme.themes.light.colors!.primary = '#000000'
    theme.defaults.VBtn!.rounded = 'pill'
    expect(factory().theme.themes.light.colors?.primary).not.toBe('#000000')
    const callback = () => 'item'
    expect(
      factory({ defaults: { VSelect: { itemTitle: callback } } }).defaults.VSelect?.itemTitle,
    ).toBe(callback)
  })
})

describe('System rules', () => {
  it.each([
    [createAtlasLookAndFeel, 0],
    [createMeridianLookAndFeel, 8],
    [createHarborLookAndFeel, 24],
    [createAtlassianLookAndFeel, 6],
  ] as const)('keeps one radius across the surfaces of %s', (factory, radius) => {
    const { defaults } = factory()
    for (const name of [
      'VBtn',
      'VCard',
      'VTextField',
      'VSelect',
      'VChip',
      'VAvatar',
      'VAlert',
      'VProgressLinear',
      'VListItem',
    ]) {
      expect(defaults[name]?.rounded, name).toBe(radius)
    }
    expect(defaults.VCard?.elevation).toBe(0)
    expect(defaults.VBtn?.ripple).toBe(false)
  })

  it('gives hospitality and editor surfaces different shapes and density', () => {
    const airbnb = createAirbnbLookAndFeel().defaults
    const jetbrains = createJetBrainsLookAndFeel().defaults
    expect(airbnb.VCard?.rounded).toBe(24)
    expect(jetbrains.VCard?.rounded).toBe(2)
    expect(airbnb.VAppBar?.height).toBe(88)
    expect(jetbrains.VAppBar?.height).toBe(48)
    expect(airbnb.VBtn?.height).toBe(48)
    expect(jetbrains.VBtn?.height).toBe(30)
    expect(airbnb.VChip?.rounded).toBe('pill')
    expect(jetbrains.VChip?.size).toBe('x-small')
  })

  it('allows component-specific customization without leaking to other components', () => {
    const theme = createAtlasLookAndFeel({ defaults: { VBtn: { rounded: 12 } } })
    expect(theme.defaults.VBtn?.rounded).toBe(12)
    expect(theme.defaults.VCard?.rounded).toBe(0)
    expect(createAtlasLookAndFeel().defaults.VBtn?.rounded).toBe(0)
  })
})

describe('Native system preference', () => {
  it('follows OS changes while system mode is selected', async () => {
    const media = Object.assign(new EventTarget(), { matches: true })
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => media),
    )
    const app = createApp({ render: () => null })
    const vuetify = createPlatformTheme({
      blueprint: createAtlassianLookAndFeel(),
      theme: { defaultTheme: 'system' },
    })
    try {
      app.use(vuetify)
      app.mount(document.createElement('div'))
      expect(vuetify.theme.current.value.dark).toBe(true)
      media.matches = false
      media.dispatchEvent(new Event('change'))
      expect(vuetify.theme.current.value.dark).toBe(false)
      await vuetify.theme.change('dark')
      expect(vuetify.theme.current.value.dark).toBe(true)
      await vuetify.theme.change('system')
      expect(vuetify.theme.current.value.dark).toBe(false)
    } finally {
      app.unmount()
      vi.unstubAllGlobals()
    }
  })
})
