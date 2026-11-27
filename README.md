# EffScript

**A readable syntax for [Effect](https://effect.website/).** EffScript is to
Effect what JSX was to React: a small language that compiles to the
TypeScript you would write by hand, minus the incantations. If you know
`async function` and `await`, you already know `effect function` and `run`.

```efs
import { Effect, Schedule } from "effect";

error UserNotFound { id: UserId }

service UserRepo {
  find(id: UserId): Effect<User, UserNotFound>;
}

export effect function greet(id: UserId) {
  const repo = use UserRepo;
  const user = run repo.find(id)
    |> Effect.retry(Schedule.recurs(2))
    |> Effect.timeout("1 second");
  run Effect.log(`Hello, ${user.name}`);
  return user;
}

export const main = greet(UserId.make("u-1"))
  catch {
    UserNotFound(e) => Effect.log(`no user ${e.id}`)
    TimeoutError => Effect.log("too slow")
  }
  with UserRepoLive;
```

- **Plain output.** It compiles to ordinary Effect code (`Effect.fn`,
  `Context.Service`, `Data.TaggedError`, `Effect.catchTags`), as TypeScript or
  JavaScript with source maps. There is no runtime.
- **The whole ecosystem.** Services, layers, errors, schemas, `match`,
  streams, [effect-machine](https://github.com/typeonce-dev/effect-machine)
  state machines and [Effect UI](https://ui.effect.website/) components with
  JSX-like markup.
- **Real types.** Type checked by
  [tsrs](https://github.com/oxc-project/tsrs), the Rust port of the
  TypeScript 7 checker, with errors at EffScript positions. EffScript and
  TypeScript files import each other freely.
- **Tooling included.** `efs` builds, checks, formats and lints, and runs a
  language server. Plugins for Vite, Rollup, Rolldown, esbuild, Bun and
  webpack.

## Quick start

```bash
cargo install --path crates/efs_cli
```

```bash
efs init hello && cd hello && pnpm install && efs check && efs build
```

## Documentation

- [Getting started](docs/getting-started.md)
- [Language guide](docs/language.md): every construct and what it compiles to
- [The `efs` command](docs/cli.md) and [lint rules](docs/lint.md)
- [TypeScript interop](docs/typescript.md), including effect-tsgo
- [Bundlers](docs/bundlers.md) and [editors](docs/editors.md)
- [Library versions](docs/compatibility.md)
- [Architecture](docs/architecture.md) and [contributing](CONTRIBUTING.md)

## Status

An early release for Effect 4. Machines and components follow
effect-machine and Effect UI, which are pre-1.0, so their syntax may change
with them. See [Not supported](docs/language.md#not-supported).

## License

MIT
