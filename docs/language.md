# The EffScript language

EffScript is to Effect what JSX was to React: a thin layer of syntax over a library you
already use. Every construct compiles to plain, idiomatic TypeScript that calls
Effect 4, `@typeonce/effect-machine` or `@effect/ui`. Nothing runs at runtime
that you could not have written by hand.

EffScript is **not** a superset of TypeScript. It keeps the parts of TypeScript that
read well (expressions, types, imports) and leaves out what Effect code does not
need (classes, `async`, decorators, `var`, `this`). Bindings work as in
JavaScript: `const` cannot be reassigned, `let` can.

- [Basics](#basics)
- [Effects](#effects): `effect function`, `effect` arrows, `effect { }`, `run`, `fail`, `defer`, `for run`
- [Pipelines, layers and errors](#pipelines-layers-and-errors): `|>`, `with`, `catch`
- [Matching](#matching): `match`
- [Services and layers](#services-and-layers): `service`, `layer`
- [Errors, schemas and enums](#errors-schemas-and-enums): `error`, `schema`, `enum`
- [Type sugar](#type-sugar)
- [State machines](#state-machines): `machine` (effect-machine)
- [Components and markup](#components-and-markup): `component` (Effect UI)
- [Interop with TypeScript](#interop-with-typescript)
- [Not supported](#not-supported)

## Basics

Statements end at the end of a line; `;` is optional, as in TypeScript. A
line that starts with `.`, `|>`, `catch`, `with` or a binary operator
(except `<`, which may start markup) continues the previous expression.
Unlike TypeScript, a line that starts with `(`, `[` or a backtick always
starts a new statement, which avoids the classic semicolon-free pitfall
where `foo` followed by `(bar)` becomes `foo(bar)`.

| EffScript | TypeScript |
| --- | --- |
| `const x = 1` | `const x = 1` |
| `let n = 0` | `let n = 0` |
| `function add(a: number, b: number): number = a + b` | `function add(a: number, b: number): number { return a + b }` |
| `function f(x) { ... }` | `function f(x) { ... }` |
| `(x) => x + 1`, `function (x) { ... }` | arrow functions |
| `a == b`, `a != b` | `a === b`, `a !== b` |
| `if c { a } else { b }` (as an expression) | `(c ? a : b)` |
| `for x of xs { }`, `for let x of xs { }` | `for (const x of xs) { }`, `for (let x of xs) { }` |
| `import { X } from "./x.efs"` | `import { X } from "./x.js"` |

Everything else (objects, arrays, spread, destructuring, optional chaining,
`??`, template strings, `as`, `satisfies`, `!`, `new`, `typeof`) is written as
in TypeScript. `export` works on every declaration.

## Effects

Effects work like `async`/`await`, with `effect` in place of `async` and
`run` in place of `await`:

| JavaScript | EffScript | Compiles to |
| --- | --- | --- |
| `async function f() { }` | `effect function f() { }` | `const f = Effect.fn("f")(function* () { })` |
| `async (x) => { }` | `effect (x) => { }` | `Effect.fnUntraced(function* (x) { })` |
| `await p` | `run e` | `yield* e` |
| `for await (const x of it) { }` | `for run x of stream { }` | `yield* Stream.runForEach(stream, (x) => Effect.gen(...))` |

An **effect body** is the inside of an `effect function`, an `effect` arrow
or an `effect { }` block. There, `run` runs an effect and gives you its
result.

```efs
export effect function transfer(from: AccountId, to: AccountId, amount: number) {
  const accounts = run Accounts
  const source = run accounts.get(from)
  if source.balance < amount {
    fail new InsufficientFunds({ needed: amount, available: source.balance })
  }
  run accounts.withdraw(from, amount)
  run accounts.deposit(to, amount)
  defer Console.log("transfer finished")
  return source
}
```

compiles to

```ts
export const transfer = Effect.fn("transfer")(function* (from: AccountId, to: AccountId, amount: number) {
  const accounts = yield* Accounts
  const source = yield* accounts.get(from)
  if (source.balance < amount) {
    return yield* Effect.fail(new InsufficientFunds({ needed: amount, available: source.balance }))
  }
  yield* accounts.withdraw(from, amount)
  yield* accounts.deposit(to, amount)
  yield* Effect.addFinalizer(() => Console.log("transfer finished"))
  return source
})
```

| EffScript | TypeScript |
| --- | --- |
| `effect function name(params) { ... }` | `const name = Effect.fn("name")(function* (params) { ... })` |
| `effect function name(params): Effect<A, E, R> { }` | the same, with the function type annotated |
| `effect function (params) { ... }`, `effect (params) => ...` | `Effect.fnUntraced(function* (params) { ... })` |
| `effect { ... }` | `Effect.gen(function* () { ... })` |
| `const x = run e` | `const x = yield* e` |
| `run e` (statement), `(run e) + 1` (in an expression) | `yield* e`, `(yield* e) + 1` |
| `fail e` | `return yield* Effect.fail(e)`; `return yield* e` when `e` is `new E(...)` of an `error` declared in the file |
| `defer e` | `yield* Effect.addFinalizer(() => e)` |
| `for run x of stream { ... }` | `yield* Stream.runForEach(stream, (x) => Effect.gen(function* () { ... }))` |

`run` looks like `await`, but it applies to the whole expression on its
right, the way `yield*` does, so that pipelines work:
`run repo.find(id) |> Effect.retry(policy)` runs the retried effect. When a
`run` is one operand of a larger expression, put it in parentheses:
`(run a) + (run b)`.

Like `await`, `run` is a keyword only inside effect bodies; elsewhere it is an
ordinary name, so `run("build")` can call your own `run` function. `run`,
`fail` and `defer` outside an effect body are compile errors. Plain functions
and arrows nested inside an effect body are not effect bodies.

## Pipelines, layers and errors

```efs
export const greeting = currentUser()
  |> Effect.map((user) => `Hello, ${user.name}!`)
  |> Effect.timeout("1 second")
  catch {
    UserNotFound(e) => Effect.succeed(`nobody called ${e.id}`)
    TimeoutError => Effect.succeed("too slow")
  }

export const main = greeting with AppLive
```

| EffScript | TypeScript |
| --- | --- |
| `a \|> f \|> g` | `pipe(a, f, g)` |
| `effect with layer` | `Effect.provide(effect, layer)` |
| `e catch { Tag(err) => h, Other => h2 }` | `Effect.catchTags(e, { Tag: (err) => h, Other: () => h2 })` |
| `e catch err => h` | `Effect.catch(e, (err) => h)` |

`with` binds loosest of all, then `catch` and `|>` (left to right), then the
usual operators. The handlers of `catch` are expressions; to sequence several
effects in one, use `effect { ... }`.

## Matching

```efs
function area(shape: Shape): number =
  match shape {
    Circle({ radius }) => Math.PI * radius ** 2
    Square({ side }) => side * side
    Point => 0
  }
```

When every arm is a tag, `match` compiles to `Match.valueTags(shape, { ... })`,
which TypeScript checks for exhaustiveness. Arms can also be literals and a
final `_` wildcard, which compile to a `Match.value(x).pipe(Match.tag(...),
Match.when(...), Match.orElse(...))` chain (`Match.exhaustive` without `_`).

## Services and layers

```efs
service UserRepo {
  find(id: UserId): Effect<User, UserNotFound>
  all: Effect<readonly User[]>
}

layer UserRepoLive for UserRepo {
  const sql = run Sql
  return {
    effect find(id: UserId) {
      const row = run sql.one(id)
      if row == undefined {
        fail new UserNotFound({ id })
      }
      return row
    },
    all: sql.all,
  }
}

layer UserRepoTest for UserRepo = {
  find: (id) => Effect.fail(new UserNotFound({ id })),
  all: Effect.succeed([]),
}
```

| EffScript | TypeScript |
| --- | --- |
| `service Name { member: T }` | `class Name extends Context.Service<Name, { readonly member: T }>()("Name") {}` |
| `service Name("app/Name") { }` | the same with an explicit key |
| method members `m(a: A): R` | `readonly m: (a: A) => R` |
| `layer L for S { effect body }` | `const L = Layer.effect(S, Effect.gen(function* () { ... }))` |
| `layer L for S = value` | `const L = Layer.succeed(S, value)` |
| `effect m(params) { }` in an object | `m: Effect.fn("S.m")(function* (params) { })` |

## Errors, schemas and enums

```efs
error UserNotFound { id: UserId }
error Unauthorized

schema UserId = String & Brand<"UserId">

schema User {
  id: UserId
  name: String
  email?: String
  roles: ("admin" | "member")[]
  manager: UserId | null
  slug = Schema.String.pipe(Schema.check(Schema.isLowercased()))
}

schema class Account { owner: UserId, balance: Number }
tagged schema Deposited { amount: Number }
schema LedgerEntry = Deposited | Withdrew

enum Shape {
  Circle { radius: number }
  Square { side: number }
  Point
}
```

| EffScript | TypeScript |
| --- | --- |
| `error E { f: T }` | `class E extends Data.TaggedError("E")<{ readonly f: T }> {}` |
| `schema S { ... }` | `const S = Schema.Struct({ ... }); type S = typeof S.Type` |
| `schema class S { ... }` | `class S extends Schema.Class<S, { readonly brand: unique symbol }>("S")({ ... }) {}`, a nominal class |
| `tagged schema S { ... }` | `const S = Schema.TaggedStruct("S", { ... })` and its type |
| `schema S = T` | `const S = <schema of T>` and its type |
| `enum E { V { f: T } }` | `type E = Data.TaggedEnum<{ V: { readonly f: T } }>; const E = Data.taggedEnum<E>()` |

Schema fields are written as **types** and translated to schemas:

| Type | Schema |
| --- | --- |
| `String`, `Boolean`, `BigInt`, `Date`, `Unknown`, ... (or lowercase `string`, ...) | `Schema.String`, `Schema.Boolean`, ... |
| `Number`, `number` | `Schema.Finite`: rejects `NaN` and `Infinity`, as Effect recommends. Write `field = Schema.Number` to accept them. |
| `"a"`, `1`, `true` | `Schema.Literal("a")`, ... |
| `"a" \| "b"` | `Schema.Literals(["a", "b"])` |
| `A \| B` | `Schema.Union([A, B])` |
| `T \| null`, `T \| undefined` | `Schema.NullOr(T)`, `Schema.UndefinedOr(T)` |
| `T[]`, `readonly T[]`, `Array<T>` | `Schema.Array(T)` |
| `Record<K, V>`, `Option<T>`, `Set<T>`, `Map<K, V>` | `Schema.Record`, `Schema.Option`, `Schema.ReadonlySet`, `Schema.ReadonlyMap` |
| `[A, B?]` | `Schema.Tuple([A, Schema.optionalKey(B)])` |
| `{ a: A, b?: B }` | `Schema.Struct({ a: A, b: Schema.optional(B) })` |
| `T & Brand<"X">` | `T.pipe(Schema.brand("X"))` |
| `Name`, `Schema.Trim` | used as written (any schema value) |
| `field?: T` | `Schema.optional(T)` |
| `field = expression` | the expression, as written |

## Type sugar

In type positions, the Effect modules whose namespace has a type of the same
name can be used directly when they are imported from `"effect"`:
`Effect<A, E, R>` means `Effect.Effect<A, E, R>`, and likewise `Stream`,
`Layer`, `Option`, `Exit`, `Cause`, `Scope`, `Schedule`, `Duration`, `Fiber`,
`Queue`, `Ref`, `Brand`, `Context`, `Schema`, `Result`, `Config` and others.

## State machines

`machine` declares a statechart for
[`@typeonce/effect-machine`](https://github.com/typeonce-dev/effect-machine).
Each decision sits next to the state that owns it.

```efs
machine Search {
  events {
    Search { query: String }
    Reset
  }
  effects {
    load = (query: string) => searchItems(query)
  }

  initial Idle

  state Idle {
    on Search if event.query.length > 0 -> Loading { query: event.query }
  }
  state Loading(query: String) {
    on Reset -> Idle
    invoke load(state.query) {
      done -> Ready { items: output }
      failure -> Failed { message: error.message }
    }
    after "5 seconds" -> Failed { message: "timed out" }
  }
  state Ready(items: String[]) {
    on Reset -> Idle
  }
  state Failed(message: String) {
    on Reset -> Idle
  }
}

machine Counter {
  root { count: Number = 0 }
  events {
    Increment { by: Number }
  }
  on Increment update root { count: root.count + event.by }
}
```

This produces `SearchState` (`Machine.state`), `SearchEvents`
(`Machine.events`) and `Search` (`Machine.make({ ... }).handle({ ... })`).
Send events with `SearchEvents.Search({ query })`.

| EffScript | effect-machine |
| --- | --- |
| `state Name(field: Type)` | a state with `fields` |
| `final state Name`, `parallel state Name { }` | `type: "final"`, `type: "parallel"` |
| `root { f: T = init }` | root `fields`, and `root: (ctx) => ({ f: init })` |
| `input T` | `input: <schema of T>`; root defaults can read `input` |
| `initial Target { data }` | `initial: { target, data }` (default: the first child) |
| `on Event -> Target { data }` | `on: { Event: { target: "Target", data: (ctx) => ({ ... }) } }` |
| `on Event if cond -> ...` | adds `guard: (ctx) => cond` |
| `on Event -> stay` | `{ stay: true }` |
| `on Event update root { f: v }` | `{ update: { root: ({ root }) => ({ ...root, f: v }) } }` |
| `invoke src(input) { done -> ...; failure -> ...; element -> ... }` | `invoke: { src, input, onDone, onFailure, onElement }` |
| `after "5 seconds" -> Target` | a generated timer plus an invocation of it |
| `effects { }`, `streams { }`, `timers { }` | the registries passed to `make` |

Inside data, guards and inputs you can use `event`, `state`, `root`,
`output`, `error`, `element`, `input`, `ancestors` and `snapshot`; EffScript
destructures exactly the ones you use. Targets may be written as a state's
name when it is unique, or as a dotted path (`Payment.Charging`).

## Components and markup

`component` declares an [Effect UI](https://ui.effect.website/) component
(`Html.component({ ... }).make({ ... })`), and markup compiles to its element
builder. A component is a list of declarations:

```efs
component Counter {
  prop label: String
  let count: Number = 0
  let history: Number[] = []
  event Reset

  update ClickedIncrement(amount: Number) {
    count += amount
    history = [...history, count]
  }

  update ClickedReset {
    count = 0
    emit Reset()
  }

  view =>
    <div class="counter">
      <h2>{label}</h2>
      <p>Count: {String(count)}</p>
      <button type="button" on:click={ClickedIncrement({ amount: 1 })}>Add one</button>
      <button type="button" disabled={count == 0} on:click={ClickedReset}>Reset</button>
    </div>
}
```

| Declaration | Meaning |
| --- | --- |
| `prop label: String` | A prop, passed in by the parent. Read-only, like `const`. `prop x?: T` is optional. |
| `let count: Number = 0` | A field of the component's model, with its initial value. The value may read props. |
| `event Saved(note: Note)` | An event the component sends to its parent. |
| `update Clicked(amount: Number) { ... }` | Declares the message `Clicked` with its fields and handles it. |
| `update Clicked => expr` | Handles a message with an expression that returns the next model or step. |
| `view => markup` | The view. |
| `name = expr` | Passed to `make` as written (`subscriptions`, `queries`, `init`, ...). |

**Updates.** In an update block, the model fields are variables: assign them
and the update returns `$.evolve(...)` with the fields you assigned.
`emit Event(...)` sends an event to the parent and `run command` issues an
Effect UI command; both are added to the step the update returns:

```ts
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
},
```

The message's fields (`amount`) are parameters. Assignments take effect when
the update ends, and reading a field after assigning it sees the new value.
`emit` can appear anywhere in the update; `run` must be a statement at the top
of the update, not inside an `if` or loop. Use the `=> expr` form for anything
else, such as `$.combine(...)`.

**Names.** Props, model fields, messages and events are used by their bare
names everywhere in the component: `label`, `count`, `ClickedReset()`,
`Reset()`. Props cannot be assigned, the view cannot assign anything (it is
pure), and mutating in place (`history.push(x)`) is a type error because the
model is read-only.

**Generated values.** Messages become `CounterMessage =
UiSchema.TaggedEnum({ ... })` and events `CounterEvents`, so code in other
files can construct them, for example a command that returns
`CounterMessage.ReceivedResults(...)`.

| Markup | Builder |
| --- | --- |
| `<div>...</div>` | `h.div.children(...)` |
| `class="a"`, `class={[a, cond && "b"]}` | `.class("a")`, `.class(a, cond && "b")` |
| `style`, `value`, `checked`, `selected`, `disabled`, `modal` | the method of the same name |
| `on:click={Clicked({ n: 1 })}`, `on:input={({ value }) => Typed({ value })}` | `.on("click", ...)` with a message or a function of the payload |
| `on:click={ClickedReset}` | `.on("click", ClickedReset())`: a message without fields can be named bare |
| `on:keydown\|preventDefault={f}` | `.on("keydown", f, { preventDefault: true })` |
| `{...attrs}` | `.attrs(attrs)` |
| any other `name="v"` / `name={v}` / `name` | `.attr("name", v)` / `.attr("name", "")` |
| `<Child key="k" label="x" />` | `Child(h, { key: "k", props: { label: "x" } })` |
| `<Child on:Saved={SavedNote} />` | handles the child's `Saved` event with your `SavedNote` message |
| `<Child on:Toggled={(e) => Toggled({ on: e.on })} />` | a function of the event's payload |
| text | trimmed like JSX and passed as strings |

A message constructor only builds a value, so `on:click={ClickedReset()}`
does not run anything when the view renders; the runtime dispatches the
message when the event happens. `on:Event` handlers on a child compile to its
`onEmit`, ignoring the events you don't handle.

Children must be strings or elements, as Effect UI requires: write
`{String(n)}` for numbers.

## Interop with TypeScript

EffScript and TypeScript modules import each other in both directions.
Code written in plain TypeScript (services, layers, errors, schemas, effects)
is used from EffScript exactly as it is, with its real types:

```ts
// legacy/mailer.ts: plain TypeScript
export class MailError extends Data.TaggedError("MailError")<{ readonly reason: string }> {}

export class Mailer extends Context.Service<Mailer, {
  readonly send: (to: string, body: string) => Effect.Effect<void, MailError>
}>()("Mailer") {}

export const MailerConsole = Layer.succeed(Mailer, { send: (to, body) => Effect.log(`${to}: ${body}`) })
```

```efs
// onboarding.efs
import { Mailer, MailerConsole } from "./legacy/mailer.js"

export effect function onboard(to: string) {
  const mailer = run Mailer
  run mailer.send(to, "Welcome!")
}

export const onboarding = onboard("ada@example.com")
  catch {
    MailError(e) => Effect.logWarning(e.reason)
  }
  with MailerConsole
```

```ts
// report.ts: plain TypeScript again
import { onboarding } from "./onboarding.js"
```

An `.efs` file and a hand-written `.ts` file with the same name would
collide, because the build writes `name.ts`. `efs build` refuses to overwrite
a `.ts` file it did not generate, and `efs check` reports the conflict.

EffScript modules import and export like TypeScript modules. `import ... from
"./x.efs"` becomes `./x.js` by default (set `importExtension` in `efs.json`),
which TypeScript resolves to the generated `x.ts`.

There are two ways to type check:

1. **`efs check`** runs [tsrs](https://github.com/oxc-project/tsrs), the Rust
   port of the TypeScript 7 checker, in process. Every `.efs` file is compiled
   in memory and overlaid on the file system as the `.ts` file `efs build`
   would write, so TypeScript files can import EffScript modules and the other way
   round. Errors are reported at their EffScript source position.
2. **TypeScript content mappers** (TypeScript 7.1): list the EffScript package in
   `tsconfig.json` and the TypeScript compiler and language server read `.efs`
   files directly, through `efs mapper`:

   ```json
   {
     "contentMappers": [
       { "package": "effscript", "extensions": [".efs"], "options": { "importExtension": ".efs" } }
     ]
   }
   ```

   Run `tsgo` or `tsrs` with `--runExternalCode`. To emit JavaScript through
   `tsc` this way, also enable `rewriteRelativeImportExtensions`.

### Effect Language Service

[effect-tsgo](https://github.com/Effect-TS/tsgo) (`@effect/tsgo`) is the
Effect team's language service for TypeScript: about 150 Effect-specific
diagnostics, with quick fixes and refactors. EffScript generates code that
passes it, and works with it in two ways:

- `efs check --effect` runs the project's `effect-tsgo` over the build output
  (written next to each `.efs` file, as `efs build` does) and reports each
  Effect diagnostic at its EffScript position. It enables the
  `@effect/language-service` plugin for the run if your tsconfig does not.

  ```bash
  pnpm add -D @effect/tsgo tsrs
  ```

  ```bash
  efs check --effect
  ```

- With the content mapper, run your editor or CLI through `effect-tsgo` (for
  example `effect-tsgo tsrs --noEmit --runExternalCode`) and its
  diagnostics apply to `.efs` files directly, like TypeScript's own.

`efs lint` has its own versions of the rules that fit EffScript's syntax, so
the most common ones show up as you type without effect-tsgo.

## Not supported

- Classes, `async`/`await`, generators, decorators, `this`, regular expression
  literals and tagged templates. Use services, schemas and Effect instead, and
  `new RegExp(...)` for patterns.
- `export default`.
- Guards in `match` arms; use `if` expressions in the arm.
- Generic `enum`s; use a `type` alias with `Data.TaggedEnum` directly.
- In machines: `history` and `choice` states, and initial data for parallel
  regions.
