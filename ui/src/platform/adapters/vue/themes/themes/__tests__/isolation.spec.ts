// @vitest-environment node
import { describe, expect, it } from 'vitest'
import { fileURLToPath } from 'node:url'
import { build } from 'vite'

const profiles = [
  ['atlas', []],
  ['meridian', []],
  ['harbor', []],
  ['atlassian', []],
  ['airbnb', ['airbnb.css']],
  ['jetbrains', ['jetbrains.css']],
  ['fluent', ['fluent.css']],
  ['jetbrainsGlass', ['jetbrainsGlassBase.css', 'jetbrainsGlass.css']],
  ['cupertinoGlass', ['cupertinoGlass.css']],
] as const

// Check the actual bundler graph, not just the source import declarations.
describe('Look and feel resource isolation', () => {
  it.each(profiles)(
    '%s only loads its own resources',
    async (profile, expectedCss) => {
      const modules = new Set<string>()
      await build({
        configFile: false,
        logLevel: 'silent',
        plugins: [
          {
            name: 'inspect-theme-resources',
            generateBundle() {
              for (const id of this.getModuleIds()) modules.add(id.split('\\').join('/'))
            },
          },
        ],
        build: {
          write: false,
          minify: false,
          lib: {
            entry: fileURLToPath(new URL(`../../${profile}.ts`, import.meta.url)),
            formats: ['es'],
          },
          rolldownOptions: { external: ['vue', /^vuetify(?:\/|$)/] },
        },
      })
      const loaded = [...modules]
      const profileCss = loaded
        .filter((id) => id.includes('/adapters/vue/themes/themes/') && id.endsWith('.css'))
        .map((id) => id.slice(id.lastIndexOf('/') + 1))
        .sort()
      expect(profileCss).toEqual([...expectedCss].sort())
      expect(loaded.some((id) => id.includes('@fontsource-variable/manrope'))).toBe(
        profile === 'cupertinoGlass',
      )
      expect(loaded.some((id) => id.includes('@phosphor-icons/vue'))).toBe(
        profile === 'cupertinoGlass',
      )
      const presets = loaded
        .filter((id) => id.endsWith('Preset.ts'))
        .map((id) => id.slice(id.lastIndexOf('/') + 1))
        .sort()
      expect(presets).toEqual([`${profile}Preset.ts`])
    },
    30000,
  )
})
