import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { h } from 'vue'
import { VIcon } from 'vuetify/components'
import { aliases } from 'vuetify/iconsets/mdi'
import { createPlatformTheme } from '../../createPlatformTheme.ts'
import { createCupertinoGlassLookAndFeel } from '../../cupertinoGlass.ts'
import { createAtlasLookAndFeel } from '../../atlas.ts'

vi.hoisted(() => {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => Object.assign(new EventTarget(), { matches: false })),
  )
})

function renderIcons(names: string[]) {
  return mount(
    {
      render: () =>
        h(
          'div',
          names.map((icon) => h(VIcon, { icon, size: 28 })),
        ),
    },
    {
      global: { plugins: [createPlatformTheme({ blueprint: createCupertinoGlassLookAndFeel() })] },
    },
  )
}

describe('Cupertino native icon integration', () => {
  it('renders every built-in Vuetify alias without an empty icon', () => {
    const wrapper = renderIcons(Object.keys(aliases).map((name) => `$${name}`))
    const icons = wrapper.findAll('.v-icon')
    expect(icons).toHaveLength(Object.keys(aliases).length)
    for (const icon of icons) {
      expect(icon.find('svg').exists() || icon.classes().includes('mdi')).toBe(true)
    }
    wrapper.unmount()
  })

  it('renders existing app names as scalable SVGs and preserves Vuetify sizing', () => {
    const wrapper = renderIcons([
      'mdi-home',
      'mdi-email-outline',
      'mdi-lock-outline',
      'mdi-eye-outline',
      '$menu',
      '$checkboxOn',
      '$dropdown',
    ])
    for (const icon of wrapper.findAll('.v-icon')) {
      expect(icon.find('svg').attributes('viewBox')).toBe('0 0 256 256')
      expect(icon.attributes('style')).toContain('28px')
      expect(icon.find('svg').attributes('aria-hidden')).toBe('true')
    }
    wrapper.unmount()
  })

  it('keeps unknown MDI names and explicit MDI requests working', () => {
    const wrapper = renderIcons(['mdi-abacus', 'mdi:mdi-home'])
    expect(wrapper.find('.mdi-abacus').exists()).toBe(true)
    expect(wrapper.find('.mdi-home').exists()).toBe(true)
    expect(wrapper.findAll('svg')).toHaveLength(0)
    wrapper.unmount()
  })

  it('scopes icon configuration to Cupertino and creates independent alias maps', () => {
    expect(createAtlasLookAndFeel().icons).toBeUndefined()
    const first = createCupertinoGlassLookAndFeel()
    first.icons.aliases!.close = 'mdi-abacus'
    expect(createCupertinoGlassLookAndFeel().icons.aliases!.close).toBe(aliases.close)
  })
})
