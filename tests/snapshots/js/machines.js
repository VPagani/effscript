import { Effect, Stream, Schema } from "effect"
import { Machine } from "@typeonce/effect-machine"

export const CheckoutState = Machine.state({ fields: { attempts: Schema.Finite }, states: {
  Cart: {},
  Payment: { states: {
    Entering: {},
    Charging: { fields: { card: Schema.String } }
  } },
  Done: { type: "final" },
  Cancelled: { type: "final" }
} })
export const CheckoutEvents = Machine.events({
  Start: {},
  Submit: { card: Schema.String },
  Paid: {},
  Fail: { reason: Schema.String },
  Cancel: {}
})
export const Checkout = Machine.make({
  root: CheckoutState,
  input: Schema.Finite,
  events: CheckoutEvents,
  effects: {
    charge: (card) => Effect.succeed(card.length)
  },
  streams: {
    ticks: Stream.make(1, 2, 3)
  }
}).handle({
  root: ({ input }) => ({ attempts: input }),
  initial: { target: "Cart" },
  states: {
    Cart: {
      on: {
        Start: { target: "Payment" },
        Cancel: { target: "Cancelled" }
      }
    },
    Payment: {
      initial: { target: "Payment.Entering" },
      on: {
        Cancel: { target: "Cancelled" }
      },
      states: {
        Entering: {
          on: {
            Submit: { target: "Payment.Charging", data: ({ event }) => ({ card: event.card }), guard: ({ event }) => event.card.length > 0 }
          }
        },
        Charging: {
          on: {
            Fail: { update: { root: ({ root }) => ({ ...root, attempts: root.attempts + 1 }) } }
          },
          invoke: [
            { src: "charge", input: ({ state }) => state.card, onDone: { target: "Done" } },
            { src: "ticks", onElement: { stay: true }, onDone: { stay: true } }
          ]
        }
      }
    },
    Done: {},
    Cancelled: {}
  }
})

export const PlayerState = Machine.state({ type: "parallel", states: {
  Playback: { states: {
    Paused: {},
    Playing: {}
  } },
  Volume: { states: {
    Audible: {},
    Muted: {}
  } }
} })
export const PlayerEvents = Machine.events({
  Toggle: {},
  Mute: {}
})
export const Player = Machine.make({
  root: PlayerState,
  events: PlayerEvents
}).handle({
  states: {
    Playback: {
      initial: { target: "Playback.Paused" },
      states: {
        Paused: {
          on: {
            Toggle: { target: "Playback.Playing" }
          }
        },
        Playing: {
          on: {
            Toggle: { target: "Playback.Paused" }
          }
        }
      }
    },
    Volume: {
      initial: { target: "Volume.Audible" },
      states: {
        Audible: {
          on: {
            Mute: { target: "Volume.Muted" }
          }
        },
        Muted: {
          on: {
            Mute: { target: "Volume.Audible" }
          }
        }
      }
    }
  }
})
