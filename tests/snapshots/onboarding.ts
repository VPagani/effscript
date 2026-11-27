import { Effect, pipe } from "effect"
import { Mailer, MailerConsole, welcomeText } from "./legacy/mailer.js"
import { currentUser, AppLive } from "./users.js"

export const onboard = Effect.fn("onboard")(function* () {
  const user = yield* currentUser()
  const mailer = yield* Mailer
  yield* mailer.send(user.email ?? "nobody", welcomeText(user.name))
  return user.id
})

export const onboarding = Effect.provide(Effect.provide(Effect.catchTags(onboard(), {
  MailError: (e) => pipe(Effect.logWarning(e.reason), Effect.as("not sent")),
  UserNotFound: () => Effect.succeed("no user"),
  Unauthorized: () => Effect.succeed("signed out"),
  TimeoutError: () => Effect.succeed("timed out")
}), MailerConsole), AppLive)
