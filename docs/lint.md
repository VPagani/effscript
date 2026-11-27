# Lint rules

`efs lint` checks the syntax tree, so it runs instantly and needs no type
information. The same rules appear in the editor as you type.

| Rule | Default | |
| --- | --- | --- |
| `discarded-effect` | warning | An effect built as a statement in an effect body is never run (`Console.log(x)` without `run`). |
| `throw-in-effect` | warning | `throw` in an effect body becomes a defect; use `fail`. |
| `prefer-fail` | hint | `return run Effect.fail(e)` is `fail e`. |
| `unused-binding` | warning | A local binding is never used. |
| `prefer-const` | warning | A `let` that is never reassigned should be `const`. |
| `pascal-case` | warning | Services, errors, schemas, machines, components, states, events and messages are PascalCase. |
| `unknown-event` | error | A machine handles an event it does not declare. |
| `unreachable-state` | warning | No transition enters a machine state. |
| `message-naming` | warning | Effect UI messages are past-tense facts (`ClickedSave`), not commands (`SetName`). |
| `strict-equality` | hint | `==` already compiles to `===`. |
| `multiple-with` | warning | Chained `with A with B` provides layers one at a time; provide them together. |
| `run-effect-inside-effect` | warning | `Effect.run*` inside an effect body starts a separate program; use `run`. |
| `missing-concurrency-option` | hint | `Effect.all` and `Effect.forEach` are sequential unless `concurrency` is set. |
| `new-promise-in-effect` | hint | `new Promise` in an effect body; use `Effect.callback` or `Effect.tryPromise`. |
| `global-console-in-effect`, `global-date-in-effect`, `global-fetch-in-effect`, `global-random-in-effect`, `global-timers-in-effect`, `process-env-in-effect` | hint | Globals used directly in an effect body that have Effect services: `Console`, `Clock`/`DateTime`, `HttpClient`, `Random`, `Effect.sleep`/`Schedule`, `Config`. |

The rules about globals only apply directly in effect bodies, not in plain
callbacks: `Effect.sync(() => Date.now())` is the right way to use them.

Silence a rule for one line with a comment on the line above:

```efs
// efs-ignore: unused-binding
const draft = run load();
```

`// efs-ignore` alone silences every rule for that line. To turn a rule off
for a project, list it under `"lint": { "disabled": [...] }` in `efs.json`.

For type-aware checks (unhandled errors, layer mistakes and about 150 more),
run `efs check --effect`, which adds the Effect Language Service's
diagnostics. See [TypeScript interop](typescript.md#effect-language-service).
