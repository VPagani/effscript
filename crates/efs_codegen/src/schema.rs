//! Translating EffScript types into Effect `Schema` values.

use efs_syntax::ast::*;

use crate::lower::Gen;

fn primitive(name: &str) -> Option<&'static str> {
    Some(match name {
        "String" | "string" => "String",
        // Finite numbers: Effect recommends rejecting NaN and Infinity by default.
        "Number" | "number" => "Finite",
        "Boolean" | "boolean" => "Boolean",
        "BigInt" | "bigint" => "BigInt",
        "Unknown" | "unknown" => "Unknown",
        "Any" | "any" => "Any",
        "Void" | "void" => "Void",
        "Never" | "never" => "Never",
        "Undefined" | "undefined" => "Undefined",
        "Null" => "Null",
        "Object" | "object" => "Object",
        "Symbol" | "symbol" => "Symbol",
        "Date" => "Date",
        "Int" => "Int",
        "Finite" => "Finite",
        _ => return None,
    })
}

fn is_literal(t: &Type) -> bool {
    matches!(&t.kind, TypeKind::Lit(l) if l != "null")
}

fn is_null(t: &Type) -> bool {
    matches!(&t.kind, TypeKind::Lit(l) if l == "null")
        || matches!(&t.kind, TypeKind::Ref { name, args: None } if name.len() == 1 && name[0].name == "Null")
}

fn is_undefined(t: &Type) -> bool {
    matches!(&t.kind, TypeKind::Ref { name, args: None } if name.len() == 1 && matches!(name[0].name.as_str(), "undefined" | "Undefined"))
}

fn brand_name(t: &Type) -> Option<&Type> {
    match &t.kind {
        TypeKind::Ref { name, args: Some(args) } if name.len() == 1 && name[0].name == "Brand" && args.len() == 1 => {
            Some(&args[0])
        }
        _ => None,
    }
}

impl Gen<'_> {
    pub(crate) fn schema_decl(&mut self, item: &Item, s: &SchemaDecl) {
        let name = s.name.name.clone();
        let export = if item.export { "export " } else { "" };
        match &s.kind {
            SchemaKind::Class(fields) => {
                self.w.s(&format!("{export}class "));
                self.w.v(s.name.span);
                self.w.s(" extends ");
                self.rtm("Schema", "Class");
                self.w.s("<");
                self.w.v(s.name.span);
                // The brand makes the class nominal, so structurally equal objects are not instances.
                self.w.s(&format!(", {{ readonly brand: unique symbol }}>({name:?})("));
                self.schema_struct_fields(fields);
                self.w.s(") {}");
                return;
            }
            SchemaKind::Struct(fields) => {
                self.w.s(&format!("{export}const "));
                self.w.v(s.name.span);
                self.w.s(" = ");
                self.rtm("Schema", "Struct");
                self.w.s("(");
                self.schema_struct_fields(fields);
                self.w.s(")");
            }
            SchemaKind::Tagged(fields) => {
                self.w.s(&format!("{export}const "));
                self.w.v(s.name.span);
                self.w.s(" = ");
                self.rtm("Schema", "TaggedStruct");
                self.w.s(&format!("({name:?}, "));
                self.schema_struct_fields(fields);
                self.w.s(")");
            }
            SchemaKind::Alias(t) => {
                self.w.s(&format!("{export}const "));
                self.w.v(s.name.span);
                self.w.s(" = ");
                self.schema_of(t);
            }
        }
        self.w.nl();
        self.w.s(&format!("{export}type "));
        self.w.v(s.name.span);
        self.w.s(" = typeof ");
        self.w.v(s.name.span);
        self.w.s(".Type");
    }

    /// `{ field: Schema, ... }`
    pub(crate) fn schema_struct_fields(&mut self, fields: &[SchemaField]) {
        if fields.is_empty() {
            self.w.s("{}");
            return;
        }
        self.w.s("{");
        self.w.indent();
        for (i, f) in fields.iter().enumerate() {
            self.w.push_node(f.span);
            if i > 0 {
                self.w.s(",");
            }
            self.w.nl();
            self.w.v(f.name.span);
            self.w.s(": ");
            self.schema_field_value(f);
            self.w.pop_node();
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}");
    }

    pub(crate) fn schema_field_value(&mut self, f: &SchemaField) {
        if f.optional {
            self.rtm("Schema", "optional");
            self.w.s("(");
        }
        match &f.value {
            SchemaFieldValue::Type(t) => self.schema_of(t),
            SchemaFieldValue::Expr(e) => self.expr(e),
        }
        if f.optional {
            self.w.s(")");
        }
    }

    /// Writes the Schema that describes type `t`.
    pub(crate) fn schema_of(&mut self, t: &Type) {
        self.w.push_node(t.span);
        self.schema_of_inner(t);
        self.w.pop_node();
    }

    fn schema_list(&mut self, items: &[&Type]) {
        self.w.s("[");
        for (i, m) in items.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            self.schema_of(m);
        }
        self.w.s("]");
    }

    fn schema_of_inner(&mut self, t: &Type) {
        match &t.kind {
            TypeKind::Ref { name, args } => {
                if name.len() == 1
                    && args.is_none()
                    && let Some(p) = primitive(&name[0].name)
                {
                    self.rtm("Schema", p);
                    return;
                }
                if name.len() == 1
                    && let Some(args) = args
                {
                    let combinator = match (name[0].name.as_str(), args.len()) {
                        ("Array" | "ReadonlyArray", 1) => Some("Array"),
                        ("Record", 2) => Some("Record"),
                        ("Option", 1) => Some("Option"),
                        ("Set" | "ReadonlySet", 1) => Some("ReadonlySet"),
                        ("Map" | "ReadonlyMap", 2) => Some("ReadonlyMap"),
                        _ => None,
                    };
                    if let Some(c) = combinator {
                        self.rtm("Schema", c);
                        self.w.s("(");
                        for (i, a) in args.iter().enumerate() {
                            if i > 0 {
                                self.w.s(", ");
                            }
                            self.schema_of(a);
                        }
                        self.w.s(")");
                        return;
                    }
                }
                // A schema value such as `UserId` or `Schema.Trim`, or a generic schema factory.
                for (i, n) in name.iter().enumerate() {
                    if i > 0 {
                        self.w.s(".");
                    }
                    self.w.v(n.span);
                }
                if let Some(args) = args {
                    self.w.s("(");
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            self.w.s(", ");
                        }
                        self.schema_of(a);
                    }
                    self.w.s(")");
                }
            }
            TypeKind::Lit(l) if l == "null" => self.rtm("Schema", "Null"),
            TypeKind::Lit(_) => {
                self.rtm("Schema", "Literal");
                self.w.s("(");
                self.w.v(t.span);
                self.w.s(")");
            }
            TypeKind::Array(inner) | TypeKind::Readonly(inner)
                if matches!(t.kind, TypeKind::Array(_)) || matches!(inner.kind, TypeKind::Array(_)) =>
            {
                let elem = match &t.kind {
                    TypeKind::Array(e) => e,
                    TypeKind::Readonly(a) => match &a.kind {
                        TypeKind::Array(e) => e,
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                };
                self.rtm("Schema", "Array");
                self.w.s("(");
                self.schema_of(elem);
                self.w.s(")");
            }
            TypeKind::Readonly(inner) | TypeKind::Paren(inner) => self.schema_of(inner),
            TypeKind::Tuple(elems) => {
                self.rtm("Schema", "Tuple");
                self.w.s("([");
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        self.w.s(", ");
                    }
                    if e.optional {
                        self.rtm("Schema", "optionalKey");
                        self.w.s("(");
                        self.schema_of(&e.ty);
                        self.w.s(")");
                    } else {
                        self.schema_of(&e.ty);
                    }
                }
                self.w.s("])");
            }
            TypeKind::Object(members) => {
                self.rtm("Schema", "Struct");
                self.w.s("({ ");
                for (i, m) in members.iter().enumerate() {
                    if i > 0 {
                        self.w.s(", ");
                    }
                    self.w.push_node(m.span);
                    self.prop_key(&m.name);
                    self.w.s(": ");
                    match &m.kind {
                        TypeMemberKind::Prop(pt) => {
                            if m.optional {
                                self.rtm("Schema", "optional");
                                self.w.s("(");
                                self.schema_of(pt);
                                self.w.s(")");
                            } else {
                                self.schema_of(pt);
                            }
                        }
                        _ => {
                            self.error(m.span, "only properties can be described by a schema");
                            self.rtm("Schema", "Never");
                        }
                    }
                    self.w.pop_node();
                }
                self.w.s(" })");
            }
            TypeKind::Union(ms) => {
                let has_null = ms.iter().any(is_null);
                let has_undefined = ms.iter().any(is_undefined);
                let rest: Vec<&Type> = ms.iter().filter(|m| !is_null(m) && !is_undefined(m)).collect();
                let wrapper = match (has_null, has_undefined) {
                    (true, true) => Some("NullishOr"),
                    (true, false) => Some("NullOr"),
                    (false, true) => Some("UndefinedOr"),
                    _ => None,
                };
                if let Some(wr) = wrapper {
                    self.rtm("Schema", wr);
                    self.w.s("(");
                }
                if rest.len() == 1 {
                    self.schema_of(rest[0]);
                } else if !rest.is_empty() && rest.iter().all(|m| is_literal(m)) {
                    self.rtm("Schema", "Literals");
                    self.w.s("([");
                    for (i, m) in rest.iter().enumerate() {
                        if i > 0 {
                            self.w.s(", ");
                        }
                        self.w.v(m.span);
                    }
                    self.w.s("])");
                } else if rest.is_empty() {
                    self.rtm("Schema", "Never");
                } else {
                    self.rtm("Schema", "Union");
                    self.w.s("(");
                    self.schema_list(&rest);
                    self.w.s(")");
                }
                if wrapper.is_some() {
                    self.w.s(")");
                }
            }
            TypeKind::Intersection(ms) => {
                let brands: Vec<&Type> = ms.iter().filter_map(brand_name).collect();
                let base: Vec<&Type> = ms.iter().filter(|m| brand_name(m).is_none()).collect();
                if base.len() != 1 {
                    self.error(t.span, "a schema intersection must be one schema and `Brand<\"Name\">` markers");
                    self.rtm("Schema", "Never");
                    return;
                }
                self.schema_of(base[0]);
                if !brands.is_empty() {
                    self.w.s(".pipe(");
                    for (i, b) in brands.iter().enumerate() {
                        if i > 0 {
                            self.w.s(", ");
                        }
                        self.rtm("Schema", "brand");
                        self.w.s("(");
                        self.w.v(b.span);
                        self.w.s(")");
                    }
                    self.w.s(")");
                }
            }
            TypeKind::Typeof(_)
            | TypeKind::Keyof(_)
            | TypeKind::Indexed { .. }
            | TypeKind::Fn { .. }
            | TypeKind::Template(_)
            | TypeKind::Array(_) => {
                self.error(t.span, "this type cannot be described by a schema; use `field = SchemaExpression` instead");
                self.rtm("Schema", "Never");
            }
        }
    }
}
