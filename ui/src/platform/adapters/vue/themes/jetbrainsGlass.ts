import './themes/jetbrainsGlassBase.css'
import './themes/jetbrainsGlass.css'
import { jetbrainsGlassPreset } from './themes/jetbrainsGlassPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent JetBrainsGlass entry point; imports only this profile's resources. */
export function createJetBrainsGlassLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(jetbrainsGlassPreset, options)
}
