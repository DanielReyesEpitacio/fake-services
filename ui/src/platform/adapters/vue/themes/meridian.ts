import { meridianPreset } from './themes/meridianPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent Meridian entry point; imports only this profile's resources. */
export function createMeridianLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(meridianPreset, options)
}
