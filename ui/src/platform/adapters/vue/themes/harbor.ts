import { harborPreset } from './themes/harborPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent Harbor entry point; imports only this profile's resources. */
export function createHarborLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(harborPreset, options)
}
