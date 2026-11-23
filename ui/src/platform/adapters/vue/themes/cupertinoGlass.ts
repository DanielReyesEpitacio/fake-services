import './themes/cupertinoGlass.css'
import '@fontsource-variable/manrope'
import { createCupertinoIcons } from './themes/cupertinoIcons.ts'
import { cupertinoGlassPreset } from './themes/cupertinoGlassPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent CupertinoGlass entry point; imports only this profile's resources. */
export function createCupertinoGlassLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return {
    ...createEnterpriseLookAndFeel(cupertinoGlassPreset, options),
    icons: createCupertinoIcons(),
  }
}
