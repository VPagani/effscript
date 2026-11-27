import { Data, Context, Effect, Layer, Console, pipe, Schedule, Stream } from "effect"

export class NotFound extends Data.TaggedError("NotFound")<{
  readonly id: string
}> {}

export class Timeout extends Data.TaggedError("Timeout")<{}> {}

class Clock extends Context.Service<Clock, {
  readonly now: Effect.Effect<number>
  readonly sleep: (ms: number) => Effect.Effect<void>
}>()("app/Clock") {}

class Cache extends Context.Service<Cache, {
  readonly get: (key: string) => Effect.Effect<string, NotFound>
  readonly set: (key: string, value: string) => Effect.Effect<void>
}>()("Cache") {}

const ClockLive = Layer.succeed(Clock, {
  now: Effect.sync(() => Date.now()),
  sleep: (ms) => Effect.sleep(ms)
})

const CacheLive = Layer.effect(Cache, Effect.gen(function* () {
  const store = new Map<string, string>()
  const _clock = yield* Clock
  yield* Effect.addFinalizer(() => Console.log("cache closed"))
  return {
    get: Effect.fn("Cache.get")(function* (key: string) {
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

export const lookup: (key: string) => Effect.Effect<string, NotFound, Cache> = Effect.fn("lookup")(function* (key: string) {
  const cache = yield* Cache
  return yield* cache.get(key)
})

export const resilient = Effect.catchTags(pipe(lookup("a"), Effect.retry(Schedule.exponential("10 millis")), Effect.timeout("1 second")), {
  NotFound: (e) => Effect.succeed(`missing ${e.id}`),
  TimeoutError: () => Effect.succeed("slow")
})

export const recovered = Effect.catch(lookup("b"), (e) => Effect.succeed(String(e)))

export const anonymous = Effect.fnUntraced(function* (n: number) {
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

export const traced = Effect.fnUntraced(function* (n: number) {
  return yield* Effect.succeed(n)
})

export const doubled = Effect.fnUntraced(function* (n: number) {
  return yield* Effect.succeed(n * 2)
})

function run(task: string): string {
  return `ran ${task}`
}

export const ran = run("build")

const use = (n: number): number => n + 1

export const usedTwice = Effect.gen(function* () {
  return use(use(1))
})

export const sumBoth = Effect.gen(function* () {
  const a = yield* doubled(1)
  const b = (yield* traced(2)) + a
  yield* Stream.runForEach(Stream.make(a, b), (tick) => Console.log(tick))
  return b
})

export const backoff = (Schedule: number): number => Schedule * 2

export const shadowed = Effect.gen(function* () {
  const Console = {
    log: (message: string) => Effect.log(message)
  }
  yield* Console.log("shadowed")
})
