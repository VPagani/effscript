//! `effect` methods of an object literal that implements a service are traced as
//! `Service.method`, when the object is written with one of Effect's own patterns.

use efs_codegen::Options;

fn compile(src: &str, opts: &Options) -> String {
    let out = efs_codegen::compile(src, opts);
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    out.code
}

/// The span names of the `effect` methods and declarations in `src`, in order.
fn names_with(src: &str, opts: &Options) -> Vec<String> {
    let code = compile(src, opts);
    code.split("Effect.fn(\"").skip(1).map(|s| s[..s.find('"').unwrap()].to_string()).collect()
}

fn names(src: &str) -> Vec<String> {
    names_with(src, &Options::default())
}

#[test]
fn layer_effect_names_the_returned_object() {
    let src = "\
export const CacheLive = Layer.effect(Cache, effect {
  const store = new Map<string, string>();
  const helper = () => ({ effect inner() { return 1; } });
  const other = { effect notReturned() { return 2; } };
  if store.size > 0 {
    return { effect early() { return 3; } } satisfies Partial<Cache>;
  }
  return {
    effect get(key: string) {
      const nested = { effect deeper() { return 4; } };
      return store.get(key);
    },
  };
});
";
    assert_eq!(names(src), ["inner", "notReturned", "Cache.early", "Cache.get", "deeper"]);
}

#[test]
fn layer_succeed_and_sync_name_their_object() {
    let src = "\
export const A = Layer.succeed(Http.Client, { effect get(url: string) { return url; } });
export const B = Layer.sync(Clock, () => ({ effect now() { return 1; } }));
export const C = Layer.sync(Random, () => {
  const seed = 1;
  return { effect next() { return seed; } };
});
";
    assert_eq!(names(src), ["Http.Client.get", "Clock.now", "Random.next"]);
}

#[test]
fn service_of_names_its_object_anywhere() {
    let src = "\
export const make = effect {
  const db = use Db;
  return Users.of({ effect find(id: string) { return id; } });
};
export const T = Layer.effect(Users, effect {
  return Users.of({ effect all() { return []; } });
});
";
    assert_eq!(names(src), ["Users.find", "Users.all"]);
}

#[test]
fn effect_module_imports_count() {
    let src = "\
import { Layer as L } from \"effect\";
export const A = L.succeed(S, { effect m() { return 1; } });
";
    assert_eq!(names(src), ["S.m"]);
    let src = "\
import * as Layer from \"effect/Layer\";
export const A = Layer.succeed(S, { effect m() { return 1; } });
";
    assert_eq!(names(src), ["S.m"]);
    let src = "\
import { Layer } from \"effect\";
export const A = Layer.succeed(S, { effect m() { return 1; } });
";
    assert_eq!(names_with(src, &Options { prelude: false, ..Default::default() }), ["S.m"]);
}

#[test]
fn a_layer_that_is_not_effects_is_ignored() {
    // Another module's `Layer`.
    let src = "\
import { Layer } from \"./my-layer.js\";
export const A = Layer.succeed(S, { effect m() { return 1; } });
";
    assert_eq!(names(src), ["m"]);
    // A local binding, at the top level or in a function.
    let src = "\
const Layer = { succeed: (s: unknown, x: unknown) => x };
export const A = Layer.succeed(S, { effect m() { return 1; } });
";
    assert_eq!(names(src), ["m"]);
    let src = "\
export const make = (Layer: Mock) => Layer.succeed(S, { effect m() { return 1; } });
";
    assert_eq!(names(src), ["m"]);
    // The prelude turned off, with no import.
    let src = "export const A = Layer.succeed(S, { effect m() { return 1; } });\n";
    assert_eq!(names_with(src, &Options { prelude: false, ..Default::default() }), ["m"]);
}

#[test]
fn other_shapes_keep_the_method_name() {
    let src = "\
export const A = Layer.succeed(makeService(), { effect a() { return 1; } });
export const B = Layer.effect(S, Effect.succeed({ effect b() { return 1; } }));
export const C = Layer.effect(S, effect {
  return build({ effect c() { return 1; } });
});
export const D = Layer.sync(S, (x: number) => ({ effect d() { return x; } }));
export const E = Layer.succeed(S, makeImpl({ effect e() { return 1; } }));
export const F = { effect f() { return 1; } };
export const G = Layer.effect(S, effect {
  for run n of Stream.make(1) {
    return { effect g() { return n; } };
  }
  return {};
});
effect function h() {
  return 1;
}
";
    assert_eq!(names(src), ["a", "b", "c", "d", "e", "f", "g", "h"]);
}

#[test]
fn the_rest_of_the_call_is_unchanged() {
    let code = compile(
        "export const ClockLive = Layer.succeed(Clock, {\n  effect now() {\n    return 1;\n  },\n});\n",
        &Options::default(),
    );
    assert!(
        code.contains(
            "export const ClockLive = Layer.succeed(Clock, {\n  now: Effect.fn(\"Clock.now\")(function* () {"
        ),
        "{code}"
    );
}
