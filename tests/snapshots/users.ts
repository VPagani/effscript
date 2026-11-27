import { Effect, Layer, Console, Schedule, Stream, Context, pipe } from "effect"
import { User, UserId, UserNotFound, Unauthorized } from "./domain.js"

export class UserRepo extends Context.Service<UserRepo, {
  readonly find: (id: UserId) => Effect.Effect<User, UserNotFound>
  readonly all: Effect.Effect<readonly User[]>
}>()("UserRepo") {}

export class Session extends Context.Service<Session, {
  readonly current: Effect.Effect<UserId, Unauthorized>
}>()("Session") {}

export const UserRepoLive = Layer.effect(UserRepo, Effect.gen(function* () {
  const users = new Map<UserId, User>()
  yield* Console.log("UserRepo ready")
  return {
    find: Effect.fn("UserRepo.find")(function* (id: UserId) {
      const user = users.get(id)
      if (user === undefined) {
        return yield* Effect.fail(new UserNotFound({ id }))
      }
      return user
    }),
    all: Effect.sync(() => [...users.values()])
  }
}))

export const SessionTest = Layer.succeed(Session, { current: Effect.succeed(UserId.make("u-1")) })

export const currentUser = Effect.fn("currentUser")(function* () {
  const session = yield* Session
  const repo = yield* UserRepo
  const id = yield* session.current
  const user = yield* pipe(repo.find(id), Effect.retry(Schedule.recurs(2)), Effect.timeout("1 second"))
  yield* Effect.log(`loaded ${user.name}`)
  return user
})

export const greeting = Effect.catchTags(pipe(currentUser(), Effect.map((user) => `Hello, ${user.name}!`)), {
  UserNotFound: (e) => Effect.succeed(`nobody called ${e.id}`),
  Unauthorized: () => Effect.succeed("please sign in")
})

export const AppLive = Layer.mergeAll(UserRepoLive, SessionTest)

export const main = Effect.provide(greeting, AppLive)

export const program = Effect.gen(function* () {
  const message = yield* main
  yield* Console.log(message)
  yield* Effect.addFinalizer(() => Console.log("done"))
  yield* Stream.runForEach(pipe(Stream.make(1, 2, 3), Stream.map((n) => n * 2)), (n) => Console.log(n))
})
