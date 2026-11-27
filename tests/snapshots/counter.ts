import { Schema } from "effect"
import { Html } from "@effect/ui/html"
import { UiSchema } from "@effect/ui"

export const CounterMessage = UiSchema.TaggedEnum({
  ClickedIncrement: { amount: Schema.Number },
  ClickedReset: {}
})
export const CounterEvents = UiSchema.TaggedEnum({
  Reset: {}
})
export const Counter = Html.component({
  name: "Counter",
  props: Schema.Struct({ label: Schema.String }),
  model: Schema.Struct({ count: Schema.Number, history: Schema.Array(Schema.Number) }),
  message: CounterMessage,
  events: CounterEvents
}).make({
  init: ($) => ({ count: 0, history: [] }),
  update: {
    ClickedIncrement: ($, { amount }) => {
      let count = $.model.count
      let history = $.model.history
      count += amount
      history = [...history, count]
      return $.evolve({ count, history })
    },
    ClickedReset: ($) => {
      let count = $.model.count
      count = 0
      const $event1 = CounterEvents.Reset()
      return $.evolve({ count }).emit($event1)
    }
  },
  view: (h, $model) => {
    const { count, history } = $model
    const { label } = h.props
    return h.div.class("counter").children(
      h.h2.children(label),
      h.p.class("count", count > 9 && "big").children("Count: ", String(count)),
      h.button.attr("type", "button").on("click", CounterMessage.ClickedIncrement({ amount: 1 })).children("Add one"),
      h.button.attr("type", "button").disabled(count === 0).on("click", CounterMessage.ClickedReset()).children("Reset"),
      h.small.children("History: ", history.join(", "))
    )
  }
})

export const DashboardMessage = UiSchema.TaggedEnum({
  ResetLikes: {}
})
export const Dashboard = Html.component({
  name: "Dashboard",
  model: Schema.Struct({ resets: Schema.Number }),
  message: DashboardMessage
}).make({
  init: ($) => ({ resets: 0 }),
  update: {
    ResetLikes: ($) => {
      let resets = $.model.resets
      resets += 1
      return $.evolve({ resets })
    }
  },
  view: (h, $model) => {
    const { resets } = $model
    return h.main.children(
      Counter(h, { onEmit: (event) => event._tag === "Reset" ? DashboardMessage.ResetLikes() : undefined, props: { label: "Likes" } }),
      h.p.children("Reset ", String(resets), " times")
    )
  }
})
