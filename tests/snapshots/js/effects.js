import { Effect, Layer, Schedule, Stream, Console, Data, Context, pipe } from "effect"

export class NotFound extends Data.TaggedError("NotFound") {}

export class Timeout extends Data.TaggedError("Timeout") {}

class Clock extends Context.Service()("app/Clock") {}

class Cache extends Context.Service()("Cache") {}

const ClockLive = Layer.succeed(Clock, {
  now: Effect.sync(() => Date.now()),
  sleep: (ms) => Effect.sleep(ms)
})

const CacheLive = Layer.effect(Cache, Effect.gen(function* () {
  const store = new Map()
  const _clock = yield* Clock
  yield* Effect.addFinalizer(() => Console.log("cache closed"))
  return {
    get: Effect.fn("Cache.get")(function* (key) {
      const value = store.get(key)
      if (value === undefined) {
        return yield* new NotFound({ id: key })
      }
      return value
    }),
    set: (key, value) => Effect.sync(() => {
      store.set(key, value)
    })
  }
}))

export const lookup = Effect.fn("lookup")(function* (key) {
  const cache = yield* Cache
  return yield* cache.get(key)
})

export const resilient = Effect.catchTags(pipe(lookup("a"), Effect.retry(Schedule.exponential("10 millis")), Effect.timeout("1 second")), {
  NotFound: (e) => Effect.succeed(`missing ${e.id}`),
  TimeoutError: () => Effect.succeed("slow")
})

export const recovered = Effect.catch(lookup("b"), (e) => Effect.succeed(String(e)))

export const anonymous = Effect.fnUntraced(function* (n) {
  yield* Effect.log(n)
  return n * 2
})

export const total = Effect.gen(function* () {
  const a = yield* Effect.succeed(1)
  const b = (yield* Effect.succeed(2)) + a
  yield* Stream.runForEach(Stream.range(1, 3), (n) => Console.log(n + b))
  return a + b
})

export const provided = Effect.provide(lookup("c"), Layer.provide(CacheLive, ClockLive))

Effect.runFork(total)

export const traced = Effect.fnUntraced(function* (n) {
  return yield* Effect.succeed(n)
})

export const doubled = Effect.fnUntraced(function* (n) {
  return yield* Effect.succeed(n * 2)
})

function run(task) {
  return `ran ${task}`
}

export const ran = run("build")

export const sumBoth = Effect.gen(function* () {
  const a = yield* doubled(1)
  const b = (yield* traced(2)) + a
  yield* Stream.runForEach(Stream.make(a, b), (tick) => Console.log(tick))
  return b
})
