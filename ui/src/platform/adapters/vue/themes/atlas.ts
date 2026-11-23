import { atlasPreset } from './themes/atlasPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent Atlas entry point; imports only this profile's resources. */
export function createAtlasLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(atlasPreset, options)
}
