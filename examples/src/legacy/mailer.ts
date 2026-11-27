// Plain TypeScript Effect code, written without EffScript.
import { Context, Data, Effect, Layer } from "effect"

export class MailError extends Data.TaggedError("MailError")<{
  readonly reason: string
}> {}

export class Mailer extends Context.Service<Mailer, {
  readonly send: (to: string, body: string) => Effect.Effect<void, MailError>
}>()("Mailer") {}

export const MailerConsole = Layer.succeed(Mailer, {
  send: (to, body) =>
    to.includes("@")
      ? Effect.log(`mail to ${to}: ${body}`)
      : Effect.fail(new MailError({ reason: `invalid address ${to}` }))
})

export const welcomeText = (name: string): string => `Welcome aboard, ${name}!`
