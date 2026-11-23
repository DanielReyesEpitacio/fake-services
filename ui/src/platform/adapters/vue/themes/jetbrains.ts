import './themes/jetbrains.css'
import { jetbrainsPreset } from './themes/jetbrainsPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent JetBrains entry point; imports only this profile's resources. */
export function createJetBrainsLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(jetbrainsPreset, options)
}
