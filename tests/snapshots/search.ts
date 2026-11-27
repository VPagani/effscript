import { Effect, Schema, pipe } from "effect"
import { Machine } from "@typeonce/effect-machine"

function searchItems(query: string): Effect.Effect<string[]> {
  return Effect.succeed([query, `${query}!`])
}

export const SearchState = Machine.state({ states: {
  Idle: {},
  Loading: { fields: { query: Schema.String } },
  Ready: { fields: { items: Schema.Array(Schema.String) } },
  Failed: { fields: { message: Schema.String } }
} })
export const SearchEvents = Machine.events({
  Search: { query: Schema.String },
  Reset: {}
})
export const Search = Machine.make({
  root: SearchState,
  events: SearchEvents,
  effects: {
    load: (query: string) => searchItems(query)
  },
  timers: {
    loadingAfter2: "5 seconds"
  }
}).handle({
  initial: { target: "Idle" },
  states: {
    Idle: {
      on: {
        Search: { target: "Loading", data: ({ event }) => ({ query: event.query }), guard: ({ event }) => event.query.length > 0 }
      }
    },
    Loading: {
      on: {
        Reset: { target: "Idle" }
      },
      invoke: [
        { src: "load", input: ({ state }) => state.query, onDone: { target: "Ready", data: ({ output }) => ({ items: output }) } },
        { src: "loadingAfter2", onDone: { target: "Failed", data: { message: "timed out" } } }
      ]
    },
    Ready: {
      on: {
        Reset: { target: "Idle" }
      }
    },
    Failed: {
      on: {
        Reset: { target: "Idle" }
      }
    }
  }
})

export const CounterState = Machine.state({ fields: { count: Schema.Finite } })
export const CounterEvents = Machine.events({
  Increment: { by: Schema.Finite },
  Decrement: {}
})
export const Counter = Machine.make({
  root: CounterState,
  events: CounterEvents
}).handle({
  root: () => ({ count: 0 }),
  on: {
    Increment: { update: { root: ({ root, event }) => ({ ...root, count: root.count + event.by }) } },
    Decrement: { update: { root: ({ root }) => ({ ...root, count: root.count - 1 }) } }
  }
})

export const startCounter = pipe(Effect.gen(function* () {
  const ref = yield* Machine.start(Counter)
  yield* ref.send(CounterEvents.Increment({ by: 2 }))
}), Effect.scoped)
