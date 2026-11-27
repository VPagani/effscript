# The EffScript language

EffScript is to Effect what JSX was to React: a thin layer of syntax over a library you
already use. Every construct compiles to plain, idiomatic TypeScript that calls
Effect 4, `@typeonce/effect-machine` or `@effect/ui`. Nothing runs at runtime
that you could not have written by hand.

For how EffScript works alongside TypeScript code, see
[TypeScript interop](typescript.md).

EffScript adds syntax only where TypeScript forces boilerplate that hides what
the code means (generators for `async`-style effects, classes for services and
errors, schema builders), not to rename an Effect API that already reads well.
That is why there is no syntax for layers: `Layer.effect(Cache, effect { ... })`
says what it does.

EffScript is **not** a superset of TypeScript. It keeps the parts of TypeScript that
read well (expressions, types, imports) and leaves out what Effect code does not
need (classes, `async`, decorators, `var`, `this`). Bindings work as in
JavaScript: `const` cannot be reassigned, `let` can.

- [Basics](#basics)
- [Effects](#effects): `effect function`, `effect` arrows, `effect { }`, `run`, `use`, `fail`, `defer`, `for run`
- [Pipelines, layers and errors](#pipelines-layers-and-errors): `|>`, `with`, `catch`
- [Matching](#matching): `match`
- [Services and layers](#services-and-layers): `service`, and layers with Effect's `Layer` API
- [Errors, schemas and enums](#errors-schemas-and-enums): `error`, `schema`, `enum`
- [The prelude](#prelude): `Effect`, `Schema`, `Stream`, ... without imports
- [Type sugar](#type-sugar)
- [State machines](#state-machines): `machine` (effect-machine)
- [Components and markup](#components-and-markup): `component` (Effect UI)
- [Not supported](#not-supported)

## Basics

Statements end at the end of a line; `;` is optional, as in TypeScript. A
line that starts with `.`, `|>`, `catch`, `with` or a binary operator
(except `<`, which may start markup) continues the previous expression.
Unlike TypeScript, a line that starts with `(`, `[` or a backtick always
starts a new statement, which avoids the classic semicolon-free pitfall
where `foo` followed by `(bar)` becomes `foo(bar)`. Because of that, leaving
semicolons out is always safe.

`efs fmt` prints semicolons by default, following Prettier: after statements
and line-style declarations, but not after a declaration or statement that ends
in its own block (`function`, `if`, `for`, `while`, `service`, `machine`,
`component`, ...), so `const f = () => { ... };` has one and `function f() { }`
does not. Set `"fmt": { "semicolons": false }` in `efs.json` to drop them. See
[`efs.json`](cli.md#efsjson).

| EffScript | TypeScript |
| --- | --- |
| `const x = 1` | `const x = 1` |
| `let n = 0` | `let n = 0` |
| `function f(x) { ... }` | `function f(x) { ... }` |
| `(x) => x + 1`, `function (x) { ... }` | arrow functions |
| `a == b`, `a != b` | `a === b`, `a !== b` |
| `if c { a } else { b }` (as an expression) | `(c ? a : b)` |
| `for x of xs { }`, `for let x of xs { }` | `for (const x of xs) { }`, `for (let x of xs) { }` |
| `import { X } from "./x.efs"` | `import { X } from "./x.js"` |
| `import("./x.efs")` | `import("./x.js")` |

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
result, and `use` gets a service.

```efs
export effect function transfer(from: AccountId, to: AccountId, amount: number) {
  const accounts = use Accounts;
  const source = run accounts.get(from);
  if source.balance < amount {
    fail new InsufficientFunds({ needed: amount, available: source.balance });
  }
  run accounts.withdraw(from, amount);
  run accounts.deposit(to, amount);
  defer Console.log("transfer finished");
  return source;
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
| `effect m(params) { }` in an object | `m: Effect.fn("m")(function* (params) { })`; `"S.m"` in a service implementation (see [trace names](#trace-names)) |
| `effect { ... }` | `Effect.gen(function* () { ... })` |
| `const x = run e` | `const x = yield* e` |
| `const s = use Service`, `use Http.Client` | `const s = yield* Service`, `yield* Http.Client` |
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
`use`, `fail` and `defer` outside an effect body are compile errors. Plain
functions and arrows nested inside an effect body are not effect bodies.

### `use` and `run`

`use Service` gets a **dependency**: the service that a `with` further out
provides. `run effect` **performs work**: it runs an effect and gives you its
result. Both compile to `yield*`, because in Effect a service key can be
yielded, but they mean different things, and EffScript keeps them apart:

```efs
export effect function onboard(id: UserId) {
  const repo = use UserRepo;      // a dependency, provided by `with UserRepoLive`
  const user = run repo.find(id); // work
  return user;
}
```

- `use` takes a name or a dotted name (`use UserRepo`, `use Http.Client`);
  anything else is a syntax error that suggests `run`.
- `run` on a service is an error: "`UserRepo` is a service; get it with
  `use UserRepo`". `use` on something that is not a service (an effect, a
  layer, a function) is an error too.
- Services are told apart by type: an Effect service key (`service`,
  `Context.Service`, `Context.Reference`, built-ins such as `Clock.Clock`) is a
  service, everything else is not. For names declared in the same file the
  compiler knows what they are, so every tool reports the error, including the
  bundler plugins. `efs check` and the language server also catch imported
  services and effects, including ones written in plain TypeScript, by their
  types (codes `run-service` and `use-non-service`).
- `use` is a keyword only when a name follows it on the same line, so
  `use(x)`, `const use = 1` and `x.use` still work.

## Pipelines, layers and errors

```efs
export const greeting = currentUser()
  |> Effect.map((user) => `Hello, ${user.name}!`)
  |> Effect.timeout("1 second")
  catch {
    UserNotFound(e) => Effect.succeed(`nobody called ${e.id}`)
    TimeoutError => Effect.succeed("too slow")
  };

export const main = greeting with AppLive;
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
const area = (shape: Shape): number =>
  match shape {
    Circle({ radius }) => Math.PI * radius ** 2
    Square({ side }) => side * side
    Point => 0
  };
```

When every arm is a tag, `match` compiles to `Match.valueTags(shape, { ... })`,
which TypeScript checks for exhaustiveness. Arms can also be literals and a
final `_` wildcard, which compile to a `Match.value(x).pipe(Match.tag(...),
Match.when(...), Match.orElse(...))` chain (`Match.exhaustive` without `_`).

## Services and layers

`service` declares a service: its name and the shape of its implementation.

```efs
service UserRepo {
  find(id: UserId): Effect<User, UserNotFound>;
  all: Effect<readonly User[]>;
}
```

| EffScript | TypeScript |
| --- | --- |
| `service Name { member: T }` | `class Name extends Context.Service<Name, { readonly member: T }>()("Name") {}` |
| `service Name("app/Name") { }` | the same with an explicit key |
| `use Name` (in an effect body) | `yield* Name`: gets the service, see [`use` and `run`](#use-and-run) |
| method members `m(a: A): R` | `readonly m: (a: A) => R` |

Implementations are **layers**, written with Effect's
[`Layer`](https://effect.website/docs/requirements-management/layers/) module
(in scope through the [prelude](#prelude)). `Layer.succeed` takes the
implementation as a value; `Layer.effect` builds it with an effect, which an
`effect { }` block writes with `use`, `run` and `defer`:

```efs
export const UserRepoTest = Layer.succeed(UserRepo, {
  find: (id) => Effect.fail(new UserNotFound({ id })),
  all: Effect.succeed([]),
});

export const UserRepoLive = Layer.effect(UserRepo, effect {
  const sql = use Sql;
  defer Console.log("UserRepo closed");
  return {
    effect find(id: UserId) {
      const row = run sql.one(id);
      if row == undefined {
        fail new UserNotFound({ id });
      }
      return row;
    },
    all: sql.all,
  };
});
```

Layers are ordinary values, so they are combined with the `Layer` API too and
provided with [`with`](#pipelines-layers-and-errors):

```efs
export const RepoLive = UserRepoLive |> Layer.provide(SqlLive);
export const AppLive = Layer.mergeAll(RepoLive, SessionLive);

export const main = program with AppLive;
```

Everything else Effect offers for layers (`Layer.unwrap`, `Layer.effectDiscard`,
layers that take parameters, memoization) works as in TypeScript; see Effect's
[layer documentation](https://effect.website/docs/requirements-management/layers/).

### Trace names

An `effect` method in an object literal compiles to
`Effect.fn("method")`. When the object is a service's implementation, written
in one of these shapes, the span is named after the service instead,
`Effect.fn("UserRepo.find")`:

| Shape | The object |
| --- | --- |
| `Layer.effect(S, effect { ... })` | the object literals the block `return`s (not returns in nested functions) |
| `Layer.succeed(S, { ... })` | the object literal |
| `Layer.sync(S, () => ({ ... }))`, `Layer.sync(S, () => { ...; return { ... }; })` | the object literal the function returns |
| `S.of({ ... })` (anywhere) | the object literal |

`S` is a name or a dotted name, used as written (`UserRepo`, `Http.Client`),
and `Layer` must be Effect's (the prelude, or imported from `"effect"`), not a
local binding of that name. The rule is syntactic, so every tool applies it,
including the bundler plugins.

## Errors, schemas and enums

```efs
error UserNotFound { id: UserId }
error Unauthorized

schema UserId = String & Brand<"UserId">;

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
schema LedgerEntry = Deposited | Withdrew;

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

## Prelude

The core modules of `effect` are in scope in every file without an import:

`Effect`, `Layer`, `Stream`, `Schema`, `Console`, `Schedule`, `Duration`,
`Config`, `Option`, `Result`, `Exit`, `Cause`, `Scope`, `Fiber`, `Ref`,
`Queue`, `Clock`, `Random`, `DateTime`, `Match`

```efs
export effect function greet(name: string) {
  run Console.log(`Hello, ${name}!`);
}

export const retried = greet("Ada") |> Effect.retry(Schedule.recurs(2));
```

compiles to

```ts
import { Effect, Console, pipe, Schedule } from "effect"

export const greet = Effect.fn("greet")(function* (name: string) {
  yield* Console.log(`Hello, ${name}!`)
})

export const retried = pipe(greet("Ada"), Effect.retry(Schedule.recurs(2)))
```

- A prelude name is imported only when the file uses it, as a value or as a
  type (including the [type sugar](#type-sugar) `Effect<A, E, R>`). The names
  join an existing `import { ... } from "effect"`, or get an import of their
  own, so the output is ordinary TypeScript with explicit imports.
- Your own names always win, without an error or a warning: a file that
  declares or imports a name (`service Clock { }`,
  `import { Effect } from "./my-effect.js"`) uses its own, and a parameter or
  local `const` with a prelude name hides the prelude where it is in scope.
- In JavaScript output, a name used only as a type is not imported.
- Other modules (`Data`, `Context`, `Brand`, `Chunk`, `HashMap`, ...) and
  companion libraries are imported as usual. Modules named like JavaScript
  globals (`Array`, `String`, `Number`, `Boolean`, `Function`, `Record`,
  `Symbol`, `Promise`, ...) are left out so that they do not hide the globals.
- The list follows the supported Effect major (see
  [Library versions](compatibility.md)).

To turn the prelude off, set `"prelude": false` in
[`efs.json`](cli.md#efsjson), or pass `prelude: false` to the
[compiler](bundlers.md#the-compiler-package) and the bundler plugins.

## Type sugar

In type positions, the Effect modules whose namespace has a type of the same
name can be used directly when they come from the [prelude](#prelude) or are
imported from `"effect"`: `Effect<A, E, R>` means `Effect.Effect<A, E, R>`,
and likewise `Stream`, `Layer`, `Option`, `Exit`, `Cause`, `Scope`,
`Schedule`, `Duration`, `Fiber`, `Queue`, `Ref`, `Schema`, `Result`, `Config`
and `DateTime`, plus `Brand`, `Context`, `Chunk`, `HashMap` and others once
imported.

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

  initial state Idle {
    on Search if event.query.length > 0 -> Loading { query: event.query };
  }
  state Loading(query: String) {
    on Reset -> Idle;
    invoke load(state.query) {
      done -> Ready { items: output };
      failure -> Failed { message: error.message };
    }
    after "5 seconds" -> Failed { message: "timed out" };
  }
  state Ready(items: String[]) {
    on Reset -> Idle;
  }
  state Failed(message: String) {
    on Reset -> Idle;
  }
}

machine Counter {
  root { count: Number = 0 }
  events {
    Increment { by: Number }
  }
  on Increment update root { count: root.count + event.by };
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
| `initial state Name(field: T = value)` | `initial: { target: "Name", data: { field: value } }`: the state entered first, with its field defaults as its data (without `initial`, the first child is entered) |
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
  prop label: String;
  let count: Number = 0;
  let history: Number[] = [];
  event Reset;

  update ClickedIncrement(amount: Number) {
    count += amount;
    history = [...history, count];
  }

  update ClickedReset {
    count = 0;
    emit Reset();
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
| `style`, `disabled` | the method of the same name |
| `value` on `input`, `textarea`, `select`; `checked` on `input`; `selected` on `option`; `modal` on `dialog` | the method of the same name (elsewhere, an ordinary attribute) |
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

## Not supported

- Classes, `async`/`await`, generators, decorators, `this`, regular expression
  literals and tagged templates. Use services, schemas and Effect instead, and
  `new RegExp(...)` for patterns.
- `export default`.
- Guards in `match` arms; use `if` expressions in the arm.
- Generic `enum`s; use a `type` alias with `Data.TaggedEnum` directly.
- In machines: `history` and `choice` states, and initial data for parallel
  regions.
