//! Removed syntax is reported with the code to write instead.

fn errors(src: &str) -> Vec<String> {
    efs_syntax::parse(src).diagnostics.into_iter().map(|d| d.message).collect()
}

#[test]
fn layer_declarations_point_to_the_layer_api() {
    let e = errors("export layer CacheLive for Cache {\n  const clock = use Clock;\n  return { get: clock.now };\n}\n");
    assert_eq!(
        e,
        vec!["`layer` declarations were removed; write `const CacheLive = Layer.effect(Cache, effect { ... })`"]
    );
    let e = errors("layer ClockLive for Clock = { now: Effect.succeed(1) };\n");
    assert_eq!(e, vec!["`layer` declarations were removed; write `const ClockLive = Layer.succeed(Clock, { ... })`"]);
    let e = errors("layer ClientLive for Http.Client = makeClient();\n");
    assert_eq!(
        e,
        vec!["`layer` declarations were removed; write `const ClientLive = Layer.succeed(Http.Client, value)`"]
    );
}

#[test]
fn removed_layers_still_bind_their_name() {
    use efs_syntax::ast::{ItemKind, PatternKind};
    let parsed = efs_syntax::parse("layer L for S { return {}; }\nconst x = L;\n");
    let ItemKind::Let(l) = &parsed.module.items[0].kind else { panic!("{:?}", parsed.module.items[0]) };
    assert!(matches!(&l.pattern.kind, PatternKind::Ident(id) if id.name == "L"));
    assert_eq!(parsed.module.items.len(), 2);
}

#[test]
fn layer_is_an_ordinary_name() {
    let src = "const layer = makeLayer();\nconst a = Mailer.layer;\nconst b = layer(layer);\nconst c = { layer };\nexport const main = program with layer;\n";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
}

#[test]
fn expression_bodies_point_to_arrows_and_blocks() {
    let e = errors("function add(a: number, b: number): number = a + b;\n");
    assert_eq!(
        e,
        vec!["`= expr` function bodies were removed; write `const add = (a: number, b: number): number => ...`"]
    );
    let e = errors("export function id<const T>(x: T): T = x;\n");
    assert_eq!(e, vec!["`= expr` function bodies were removed; write `const id = <const T>(x: T): T => ...`"]);
    let e = errors("effect function load(id: string) = repo.find(id);\n");
    assert_eq!(
        e,
        vec![
            "`= expr` function bodies were removed; write a block: `effect function load(id: string) { return ...; }`"
        ]
    );
    let e = errors("const f = function (x: number) = x;\n");
    assert_eq!(e, vec!["`= expr` function bodies were removed; write an arrow function: `(x: number) => ...`"]);
    let e = errors("const o = { m(x: number) = x, effect n() = Effect.void };\n");
    assert_eq!(
        e,
        vec![
            "`= expr` function bodies were removed; write `m: (x: number) => ...` or a block: `m(x: number) { return ...; }`",
            "`= expr` function bodies were removed; write a block: `effect n() { return ...; }`",
        ]
    );
}

#[test]
fn expression_bodies_are_read_as_returns() {
    use efs_syntax::ast::{FuncBody, ItemKind, StmtKind};
    let parsed = efs_syntax::parse("function f(x: number): number =\n  x + 1;\nconst y = f(1);\n");
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.module.items.len(), 2);
    let ItemKind::Fn(f) = &parsed.module.items[0].kind else { panic!() };
    let FuncBody::Block(b) = &f.func.body else { panic!() };
    assert!(matches!(b.stmts[..], [efs_syntax::ast::Stmt { kind: StmtKind::Return(Some(_)), .. }]));
}

#[test]
fn other_equals_forms_stay() {
    let src = "type A = number;\nschema Id = String & Brand<\"Id\">;\nconst f = (x: number) => x;\ncomponent C {\n  init = Effect.void\n  view => h.div\n}\n";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
}
