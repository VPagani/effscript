// Plain TypeScript importing EffScript output.
import { Effect } from "effect"
import { onboarding } from "../onboarding.js"
import { area, Shape } from "../domain.js"

export const report: Effect.Effect<string> = onboarding.pipe(
  Effect.map((result) => `onboarding: ${result}, unit circle area: ${area(Shape.Circle({ radius: 1 })).toFixed(2)}`)
)
