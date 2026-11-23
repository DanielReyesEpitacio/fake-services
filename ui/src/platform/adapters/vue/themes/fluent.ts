import './themes/fluent.css'
import { fluentPreset } from './themes/fluentPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent Fluent entry point; imports only this profile's resources. */
export function createFluentLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(fluentPreset, options)
}
