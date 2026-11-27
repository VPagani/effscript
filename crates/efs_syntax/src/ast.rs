//! The EffScript syntax tree. Every node carries the span of the source it came from.

use crate::span::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

// ---------------------------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub export: bool,
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Import(Import),
    /// `export { a, b } from "m"`, `export * from "m"`, `export { a }`: kept as written.
    ExportList(ExportList),
    Let(LetDecl),
    Fn(FnDecl),
    Type(TypeAlias),
    Service(Service),
    Layer(LayerDecl),
    Error(ErrorDecl),
    Schema(SchemaDecl),
    Enum(EnumDecl),
    Machine(Machine),
    Component(Component),
    Stmt(Stmt),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub type_only: bool,
    pub default: Option<Ident>,
    pub namespace: Option<Ident>,
    pub named: Vec<ImportSpec>,
    /// Whether braces were written, even if empty.
    pub has_braces: bool,
    pub source: StrLit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportSpec {
    pub type_only: bool,
    pub name: Ident,
    pub alias: Option<Ident>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportList {
    pub star: bool,
    pub specs: Vec<ImportSpec>,
    pub source: Option<StrLit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrLit {
    /// The raw text including quotes.
    pub raw: String,
    pub span: Span,
}

impl StrLit {
    /// The unquoted value (escape sequences are kept as written).
    pub fn value(&self) -> &str {
        &self.raw[1..self.raw.len().saturating_sub(1).max(1)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LetKind {
    /// `const`: an immutable binding.
    Const,
    /// `let`: a mutable binding.
    Let,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LetDecl {
    pub kind: LetKind,
    pub pattern: Pattern,
    pub ty: Option<Type>,
    pub init: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDecl {
    pub name: Ident,
    pub func: Func,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Func {
    /// `effect function` or `effect (x) => ...`: the body runs as an Effect generator and may use `run`.
    pub effect: bool,
    /// Written as an arrow function.
    pub arrow: bool,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: FuncBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FuncBody {
    Expr(Box<Expr>),
    Block(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub pattern: Pattern,
    pub optional: bool,
    pub rest: bool,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeParam {
    pub name: Ident,
    pub constraint: Option<Type>,
    pub default: Option<Type>,
    pub is_const: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeAlias {
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub ty: Type,
}

/// `service Name { member: Type }`
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub name: Ident,
    /// Optional explicit key: `service Name("app/Name") { ... }`.
    pub key: Option<StrLit>,
    pub members: Vec<TypeMember>,
}

/// `layer Name for Service { effect body }` or `layer Name for Service = value`.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDecl {
    pub name: Ident,
    pub service: Expr,
    pub body: LayerBody,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerBody {
    Effect(Block),
    Value(Expr),
}

/// `error Name { field: Type }`
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorDecl {
    pub name: Ident,
    pub fields: Vec<TypeMember>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDecl {
    pub name: Ident,
    pub kind: SchemaKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SchemaKind {
    /// `schema User { ... }`
    Struct(Vec<SchemaField>),
    /// `schema class User { ... }`
    Class(Vec<SchemaField>),
    /// `tagged schema Circle { ... }`
    Tagged(Vec<SchemaField>),
    /// `schema Id = String & Brand<"Id">`
    Alias(Type),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchemaField {
    pub name: Ident,
    pub optional: bool,
    pub value: SchemaFieldValue,
    /// A default initial value, used by machines and components: `count: Number = 0`.
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SchemaFieldValue {
    /// A type, translated to the Schema that describes it.
    Type(Type),
    /// `name = expr`: a schema expression used as written.
    Expr(Expr),
}

/// `enum Shape { Circle { radius: number } Square { side: number } }`
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: Ident,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: Ident,
    pub fields: Vec<SchemaField>,
    pub span: Span,
}

// ---------------------------------------------------------------------------------------------
// Machines (@typeonce/effect-machine)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    pub name: Ident,
    pub parallel: bool,
    pub input: Option<Type>,
    pub root_fields: Vec<SchemaField>,
    pub events: Vec<Variant>,
    pub sources: Vec<MachineSource>,
    pub body: StateBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Effects,
    Streams,
    Timers,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MachineSource {
    pub kind: SourceKind,
    pub name: Ident,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct StateBody {
    pub initial: Option<Transition>,
    pub states: Vec<StateDecl>,
    pub ons: Vec<On>,
    pub invokes: Vec<Invoke>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateDecl {
    pub name: Ident,
    pub is_final: bool,
    pub parallel: bool,
    pub fields: Vec<SchemaField>,
    pub body: StateBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct On {
    pub event: Ident,
    pub guard: Option<Expr>,
    pub transition: Transition,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    /// `-> Target.Path { data }`
    Target { path: Vec<Ident>, data: Option<Expr>, span: Span },
    /// `-> stay`
    Stay(Span),
    /// `update root { field: value }`
    Update { owner: Ident, data: Expr, span: Span },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Invoke {
    /// The registered source name, or a timer synthesized from `after`.
    pub src: InvokeSrc,
    pub input: Option<Expr>,
    pub handlers: Vec<(Ident, Transition)>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InvokeSrc {
    Named(Ident),
    After(Expr),
}

// ---------------------------------------------------------------------------------------------
// Components (@effect/ui)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub name: Ident,
    /// Members in source order.
    pub members: Vec<Member>,
}

impl Component {
    pub fn props(&self) -> impl Iterator<Item = &SchemaField> {
        self.members.iter().filter_map(|m| if let MemberKind::Prop(f) = &m.kind { Some(f) } else { None })
    }
    /// Model fields (`let`).
    pub fn fields(&self) -> impl Iterator<Item = &SchemaField> {
        self.members.iter().filter_map(|m| if let MemberKind::Let(f) = &m.kind { Some(f) } else { None })
    }
    pub fn events(&self) -> impl Iterator<Item = &Variant> {
        self.members.iter().filter_map(|m| if let MemberKind::Event(v) = &m.kind { Some(v) } else { None })
    }
    pub fn updates(&self) -> impl Iterator<Item = &UpdateArm> {
        self.members.iter().filter_map(|m| if let MemberKind::Update(u) = &m.kind { Some(u) } else { None })
    }
    pub fn view(&self) -> Option<&Expr> {
        self.members.iter().find_map(|m| if let MemberKind::View(v) = &m.kind { Some(v) } else { None })
    }
    pub fn options(&self) -> impl Iterator<Item = (&Ident, &Expr)> {
        self.members.iter().filter_map(|m| if let MemberKind::Option(k, v) = &m.kind { Some((k, v)) } else { None })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub kind: MemberKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemberKind {
    /// `prop label: String`
    Prop(SchemaField),
    /// `let count: Number = 0`: a field of the component's model.
    Let(SchemaField),
    /// `event Saved(note: Note)`: a message the component sends to its parent.
    Event(Variant),
    /// `update ClickedAdd(amount: Number) { ... }`: declares a message and handles it.
    Update(UpdateArm),
    /// `view => markup`
    View(Expr),
    /// Any other `name = expr`, passed to `make` as written.
    Option(Ident, Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateArm {
    pub message: Ident,
    /// The message's payload fields, also bound as parameters.
    pub fields: Vec<SchemaField>,
    pub body: UpdateBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateBody {
    /// `{ count += 1 }`: assignments to model fields, `emit` and `run`.
    Block(Block),
    /// `=> expr`: returns the next model or step.
    Expr(Expr),
}

// ---------------------------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Let(LetDecl),
    Fn(FnDecl),
    Expr(Expr),
    Return(Option<Expr>),
    If(IfStmt),
    /// `for x of xs { }` or, with `bind`, `for run x of stream { }`.
    For {
        kind: LetKind,
        pattern: Pattern,
        iter: Expr,
        body: Block,
        bind: bool,
    },
    While {
        cond: Expr,
        body: Block,
    },
    Break,
    Continue,
    /// `fail error`: fails the current effect.
    Fail(Expr),
    /// `defer effect`: runs the effect when the current scope closes.
    Defer(Expr),
    /// `emit Event(...)`: in a component update, sends an event to the parent.
    Emit(Expr),
    Throw(Expr),
    Block(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then: Block,
    pub else_: Option<Box<Stmt>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    Ident(Ident),
    Object {
        props: Vec<PatternProp>,
        rest: Option<Ident>,
    },
    Array {
        elems: Vec<Option<Pattern>>,
        rest: Option<Box<Pattern>>,
    },
    /// An array element with a default: `[a = 1]`.
    Default {
        pattern: Box<Pattern>,
        default: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternProp {
    pub key: Ident,
    pub value: Option<Pattern>,
    pub default: Option<Expr>,
}

// ---------------------------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Ident(String),
    /// Number, string, `true`, `false`, `null`: emitted as written.
    Lit(String),
    Template(Vec<TemplatePart>),
    Array(Vec<ArrayElem>),
    Object(Vec<Prop>),
    Paren(Box<Expr>),
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    Update {
        op: String,
        prefix: bool,
        expr: Box<Expr>,
    },
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Assign {
        op: String,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Cond {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        type_args: Option<Vec<Type>>,
        args: Vec<ArrayElem>,
        optional: bool,
    },
    Member {
        object: Box<Expr>,
        name: Ident,
        optional: bool,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        optional: bool,
    },
    New {
        callee: Box<Expr>,
        type_args: Option<Vec<Type>>,
        args: Vec<ArrayElem>,
    },
    Func(Box<Func>),
    /// `a |> f |> g`
    Pipe(Vec<Expr>),
    /// `effect with layer`: provides a layer to an effect.
    With {
        expr: Box<Expr>,
        layer: Box<Expr>,
    },
    /// `run effect`: runs an effect inside an effect body (`yield*`).
    Bind(Box<Expr>),
    /// `effect catch { Tag(e) => handler }` or `effect catch e => handler`.
    Catch {
        expr: Box<Expr>,
        handler: CatchHandler,
    },
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
    },
    /// `effect { ... }`
    EffectBlock(Block),
    Markup(Box<Element>),
    As {
        expr: Box<Expr>,
        ty: Type,
    },
    Satisfies {
        expr: Box<Expr>,
        ty: Type,
    },
    NonNull(Box<Expr>),
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplatePart {
    /// Raw template text, including the delimiters that end it (`` ` ``, `${`, `}`).
    Str(Span),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArrayElem {
    Expr(Expr),
    Spread(Expr),
    Hole,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    KeyValue {
        key: PropKey,
        value: Expr,
    },
    Shorthand(Ident),
    Spread(Expr),
    /// `name(params) { }` or `effect name(params) { }`.
    Method {
        key: PropKey,
        func: Func,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropKey {
    Ident(Ident),
    Str(StrLit),
    Num(String, Span),
    Computed(Box<Expr>),
}

impl PropKey {
    pub fn span(&self) -> Span {
        match self {
            PropKey::Ident(i) => i.span,
            PropKey::Str(s) => s.span,
            PropKey::Num(_, s) => *s,
            PropKey::Computed(e) => e.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CatchHandler {
    Tags(Vec<CatchArm>),
    All { binding: Pattern, body: Box<Expr> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatchArm {
    pub tag: Ident,
    pub binding: Option<Pattern>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MatchPattern {
    Tag { tag: Ident, binding: Option<Pattern> },
    Lit(String, Span),
    Wildcard(Span),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// `div`, `Counter`, `Kit.Button`.
    pub name: Vec<Ident>,
    pub attrs: Vec<Attr>,
    pub children: Vec<Child>,
    pub self_closing: bool,
    pub span: Span,
}

impl Element {
    pub fn is_component(&self) -> bool {
        self.name.len() > 1 || self.name[0].name.starts_with(|c: char| c.is_uppercase())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Attr {
    Named { name: Ident, value: Option<AttrValue>, span: Span },
    Spread(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrValue {
    Str(StrLit),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Child {
    Text(String, Span),
    Expr(Expr),
    Element(Element),
}

// ---------------------------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    Ref {
        name: Vec<Ident>,
        args: Option<Vec<Type>>,
    },
    /// String, number, boolean, `null` literals.
    Lit(String),
    Array(Box<Type>),
    Tuple(Vec<TupleElem>),
    Object(Vec<TypeMember>),
    Union(Vec<Type>),
    Intersection(Vec<Type>),
    Fn {
        type_params: Vec<TypeParam>,
        params: Vec<Param>,
        ret: Box<Type>,
    },
    Paren(Box<Type>),
    Typeof(Vec<Ident>),
    Keyof(Box<Type>),
    Readonly(Box<Type>),
    Indexed {
        object: Box<Type>,
        index: Box<Type>,
    },
    /// A template literal type, kept as written.
    Template(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TupleElem {
    pub name: Option<Ident>,
    pub optional: bool,
    pub rest: bool,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeMember {
    pub name: PropKey,
    pub readonly: bool,
    pub optional: bool,
    pub kind: TypeMemberKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeMemberKind {
    Prop(Type),
    Method {
        type_params: Vec<TypeParam>,
        params: Vec<Param>,
        ret: Option<Type>,
    },
    /// `[key: string]: T`
    Index {
        param: Ident,
        key: Type,
        value: Type,
    },
}
