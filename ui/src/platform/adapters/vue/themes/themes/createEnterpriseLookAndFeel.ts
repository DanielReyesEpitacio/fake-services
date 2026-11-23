import type {
  EnterprisePreset,
  EnterpriseTheme,
  EnterpriseThemeOptions,
  ThemeMode,
} from './types.ts'

/** Builds independent configurations without mutating presets or caller options. */
export function createEnterpriseLookAndFeel(
  preset: EnterprisePreset,
  options: EnterpriseThemeOptions = {},
): EnterpriseTheme {
  const buildMode = (mode: Exclude<ThemeMode, 'system'>) => ({
    dark: mode === 'dark',
    colors: {
      ...preset[mode].colors,
      ...options.colors,
      ...options[mode]?.colors,
    },
    variables: { ...preset[mode].variables, ...options[mode]?.variables },
  })

  const defaults = Object.fromEntries(
    [...new Set([...Object.keys(preset.defaults), ...Object.keys(options.defaults ?? {})])].map(
      (component) => [
        component,
        {
          ...preset.defaults[component],
          ...options.defaults?.[component],
        },
      ],
    ),
  )

  return {
    theme: {
      themes: { light: buildMode('light'), dark: buildMode('dark') },
    },
    defaults,
  }
}
