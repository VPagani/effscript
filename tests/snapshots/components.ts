import { Effect, Schema } from "effect"
import { Html } from "@effect/ui/html"
import { Command, UiSchema } from "@effect/ui"

export const ToggleMessage = UiSchema.TaggedEnum({
  ClickedToggle: {}
})
export const ToggleEvents = UiSchema.TaggedEnum({
  Toggled: { on: Schema.Boolean },
  TurnedOn: {}
})
export const Toggle = Html.component({
  name: "Toggle",
  props: Schema.Struct({ label: Schema.String, initial: Schema.optional(Schema.Boolean) }),
  model: Schema.Struct({ on: Schema.Boolean }),
  message: ToggleMessage,
  events: ToggleEvents
}).make({
  init: ($) => {
    const { initial } = $.props
    return ({ on: initial ?? false })
  },
  update: {
    ClickedToggle: ($) => {
      let on = $.model.on
      const $events: Array<typeof ToggleEvents.Type> = []
      on = !on
      $events.push(ToggleEvents.Toggled({ on }))
      if (on) {
        $events.push(ToggleEvents.TurnedOn())
      }
      return $.evolve({ on }).emit(...$events)
    }
  },
  view: (h, $model) => {
    const { on } = $model
    const { label } = h.props
    return h.button.attr("type", "button").attr("aria-pressed", on ? "true" : "false").on("click", ToggleMessage.ClickedToggle()).children(label)
  }
})

const Search = Command.make("Search", { args: { query: Schema.String } }, ({ query }) => Effect.succeed(SearchBoxMessage.ReceivedResults({ found: [query, `${query}!`] })))

export const SearchBoxMessage = UiSchema.TaggedEnum({
  UpdatedQuery: { value: Schema.String },
  PressedKey: { key: Schema.String },
  ToggledStrict: { enabled: Schema.Boolean },
  SubmittedSearch: {},
  ReceivedResults: { found: Schema.Array(Schema.String) }
})
export const SearchBox = Html.component({
  name: "SearchBox",
  model: Schema.Struct({ query: Schema.String, results: Schema.Array(Schema.String), strict: Schema.Boolean, searches: Schema.Finite }),
  message: SearchBoxMessage
}).make({
  init: ($) => ({ query: "", results: [], strict: false, searches: 0 }),
  update: {
    UpdatedQuery: ($, { value }) => {
      let query = $.model.query
      let searches = $.model.searches
      query = value
      if (value.length > 2) {
        searches++
      }
      return $.evolve({ query, searches })
    },
    PressedKey: ($, { key }) => key === "Escape" ? $.evolve({ query: "" }) : $.model,
    ToggledStrict: ($, { enabled }) => {
      let strict = $.model.strict
      strict = enabled
      return $.evolve({ strict })
    },
    SubmittedSearch: ($) => {
      const { query } = $.model
      const $command1 = Search.make({ query })
      return $.run($command1)
    },
    ReceivedResults: ($, { found }) => {
      let results = $.model.results
      results = found
      return $.evolve({ results })
    }
  },
  view: (h, $model) => {
    const { query, results, searches } = $model
    return h.section.class("search", query.length > 0 && "active").children(
      h.form.on("submit", SearchBoxMessage.SubmittedSearch()).children(
        h.input.value(query).on("input", ({ value }) => SearchBoxMessage.UpdatedQuery({ value })).on("keydown", ({ key }) => key === "Escape" ? SearchBoxMessage.PressedKey({ key }) : undefined, { preventDefault: true })
      ),
      h.ul.children(results.map((r) => h.li.children(r))),
      Toggle(h, { key: "strict", onEmit: (event) => event._tag === "Toggled" ? ((e) => SearchBoxMessage.ToggledStrict({ enabled: e.on }))(event) : undefined, props: { label: "Strict" } }),
      h.p.children("Found ", String(results.length), " results for \"", query, "\" after ", String(searches), " searches.")
    )
  }
})
