import { atlassianPreset } from './themes/atlassianPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent Atlassian entry point; imports only this profile's resources. */
export function createAtlassianLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(atlassianPreset, options)
}
