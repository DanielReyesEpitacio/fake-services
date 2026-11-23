import './themes/airbnb.css'
import { airbnbPreset } from './themes/airbnbPreset.ts'
import { createEnterpriseLookAndFeel } from './themes/createEnterpriseLookAndFeel.ts'
import type { EnterpriseThemeOptions } from './themes/types.ts'

/** Independent Airbnb entry point; imports only this profile's resources. */
export function createAirbnbLookAndFeel(options: EnterpriseThemeOptions = {}) {
  return createEnterpriseLookAndFeel(airbnbPreset, options)
}
