//! The EffScript formatter. It prints the syntax tree in a canonical style while respecting
//! a few layout choices from the source: whether a bracketed construct spans several
//! lines, where pipelines break, and blank lines between statements. Comments are
//! never dropped.

use efs_syntax::ast::*;
use efs_syntax::lexer::Comment;
use efs_syntax::{Diagnostic, Span};

/// Formatter options.
#[derive(Clone, Debug)]
pub struct FormatOptions {
    /// Print `;` after statements and line-style declarations (the default), or leave them out.
    pub semicolons: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions { semicolons: true }
    }
}

/// Formats EffScript source with the default options. Files with syntax errors are not formatted.
pub fn format(src: &str) -> Result<String, Vec<Diagnostic>> {
    format_with(src, &FormatOptions::default())
}

/// Formats EffScript source. Files with syntax errors are not formatted.
pub fn format_with(src: &str, opts: &FormatOptions) -> Result<String, Vec<Diagnostic>> {
    let parsed = efs_syntax::parse(src);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    let mut p =
        Printer { src, out: String::new(), indent: 0, comments: parsed.comments, next: 0, semi: opts.semicolons };
    p.module(&parsed.module);
    Ok(p.out)
}

struct Printer<'s> {
    src: &'s str,
    out: String,
    indent: usize,
    comments: Vec<Comment>,
    next: usize,
    semi: bool,
}

impl Printer<'_> {
    fn w(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Prints the optional `;` terminator, if enabled.
    fn semi(&mut self) {
        if self.semi {
            self.w(";");
        }
    }

    fn newline(&mut self) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        self.out.push('\n');
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
    }

    fn blank_line(&mut self) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if !self.out.ends_with("\n\n") && !self.out.is_empty() {
            self.out.push('\n');
        }
        self.newline();
    }

    fn multiline(&self, span: Span) -> bool {
        span.text(self.src).contains('\n')
    }

    fn has_newline(&self, from: u32, to: u32) -> bool {
        from < to && self.src[from as usize..to as usize].contains('\n')
    }

    fn blank_between(&self, from: u32, to: u32) -> bool {
        if from >= to {
            return false;
        }
        let segs: Vec<&str> = self.src[from as usize..to as usize].split('\n').collect();
        segs.len() >= 3 && segs[1..segs.len() - 1].iter().any(|l| l.trim().is_empty())
    }

    fn comment_text(&self, c: &Comment) -> String {
        let t = c.span.text(self.src);
        if c.block { t.to_string() } else { t.trim_end().to_string() }
    }

    /// Prints comments that start before `pos`, each on its own line.
    fn leading(&mut self, pos: u32) {
        while let Some(c) = self.comments.get(self.next).cloned() {
            if c.span.start >= pos {
                break;
            }
            self.next += 1;
            let text = self.comment_text(&c);
            self.w(&text);
            let next_start = self.comments.get(self.next).map_or(pos, |n| n.span.start.min(pos));
            if self.blank_between(c.span.end, next_start) {
                self.blank_line();
            } else {
                self.newline();
            }
        }
    }

    /// Prints a comment that follows `end` on the same source line.
    fn trailing(&mut self, end: u32) {
        while let Some(c) = self.comments.get(self.next).cloned() {
            if c.span.start < end || self.has_newline(end, c.span.start) || c.own_line {
                break;
            }
            self.next += 1;
            let text = self.comment_text(&c);
            self.w(" ");
            self.w(&text);
        }
    }

    /// Starts the next entry of a sequence: a new line, a blank line if the source had one.
    fn seq(&mut self, prev_end: Option<u32>, start: u32) {
        if let Some(prev) = prev_end {
            let first_comment = self.comments.get(self.next).map(|c| c.span.start).filter(|&s| s < start && s >= prev);
            let until = first_comment.unwrap_or(start);
            if self.blank_between(prev, until) {
                self.blank_line();
            } else {
                self.newline();
            }
        }
        self.leading(start);
    }

    fn module(&mut self, m: &Module) {
        let mut prev = None;
        for item in &m.items {
            if prev.is_none() {
                self.leading(item.span.start);
            } else {
                self.seq(prev, item.span.start);
            }
            self.item(item);
            self.trailing(item.span.end);
            prev = Some(item.span.end);
        }
        // Any comments left over (at the end of the file or inside dropped spans).
        if self.next < self.comments.len() {
            if !self.out.is_empty() {
                let last = prev.unwrap_or(0);
                let start = self.comments[self.next].span.start;
                if self.blank_between(last, start) {
                    self.blank_line();
                } else {
                    self.newline();
                }
            }
            self.leading(u32::MAX);
        }
        while self.out.ends_with(['\n', ' ']) {
            self.out.pop();
        }
        self.out.push('\n');
    }

    fn ident(&mut self, id: &Ident) {
        self.w(&id.name);
    }

    fn raw(&mut self, span: Span) {
        let t = span.text(self.src).to_string();
        self.w(&t);
    }

    // -----------------------------------------------------------------------------------------
    // Items
    // -----------------------------------------------------------------------------------------

    fn item(&mut self, item: &Item) {
        if item.export {
            self.w("export ");
        }
        match &item.kind {
            ItemKind::Import(imp) => {
                self.w("import ");
                if imp.type_only {
                    self.w("type ");
                }
                let mut any = false;
                if let Some(d) = &imp.default {
                    self.ident(d);
                    any = true;
                }
                if let Some(n) = &imp.namespace {
                    if any {
                        self.w(", ");
                    }
                    self.w("* as ");
                    self.ident(n);
                    any = true;
                }
                if imp.has_braces {
                    if any {
                        self.w(", ");
                    }
                    self.specs(&imp.named);
                    any = true;
                }
                if any {
                    self.w(" from ");
                }
                self.raw(imp.source.span);
                self.semi();
            }
            ItemKind::ExportList(list) => {
                if list.star {
                    self.w("*");
                } else {
                    self.specs(&list.specs);
                }
                if let Some(s) = &list.source {
                    self.w(" from ");
                    self.raw(s.span);
                }
                self.semi();
            }
            ItemKind::Let(l) => {
                self.let_decl(l);
                self.semi();
            }
            ItemKind::Fn(f) => {
                self.fn_decl(f);
                if fn_needs_semi(f) {
                    self.semi();
                }
            }
            ItemKind::Type(t) => {
                self.w("type ");
                self.ident(&t.name);
                self.type_params(&t.type_params);
                self.w(" = ");
                self.ty(&t.ty);
                self.semi();
            }
            ItemKind::Service(s) => {
                self.w("service ");
                self.ident(&s.name);
                if let Some(k) = &s.key {
                    self.w("(");
                    self.raw(k.span);
                    self.w(")");
                }
                self.w(" ");
                self.members(&s.members, item.span, true, true);
            }
            ItemKind::Layer(l) => {
                self.w("layer ");
                self.ident(&l.name);
                self.w(" for ");
                self.expr(&l.service);
                match &l.body {
                    LayerBody::Effect(b) => {
                        self.w(" ");
                        self.block(b);
                    }
                    LayerBody::Value(v) => {
                        self.w(" = ");
                        self.expr(v);
                        self.semi();
                    }
                }
            }
            ItemKind::Error(e) => {
                self.w("error ");
                self.ident(&e.name);
                if !e.fields.is_empty() {
                    self.w(" ");
                    self.members(&e.fields, item.span, false, false);
                }
            }
            ItemKind::Schema(s) => {
                match &s.kind {
                    SchemaKind::Tagged(_) => self.w("tagged schema "),
                    SchemaKind::Class(_) => self.w("schema class "),
                    _ => self.w("schema "),
                }
                self.ident(&s.name);
                match &s.kind {
                    SchemaKind::Alias(t) => {
                        self.w(" = ");
                        self.ty(t);
                        self.semi();
                    }
                    SchemaKind::Struct(f) | SchemaKind::Class(f) | SchemaKind::Tagged(f) => {
                        self.w(" ");
                        self.schema_fields(f, "{", "}", item.span);
                    }
                }
            }
            ItemKind::Enum(e) => {
                self.w("enum ");
                self.ident(&e.name);
                self.w(" ");
                self.variants(&e.variants, item.span);
            }
            ItemKind::Machine(m) => self.machine(m, item.span),
            ItemKind::Component(c) => self.component(c, item.span),
            ItemKind::Stmt(s) => self.stmt_semi(s),
        }
    }

    fn specs(&mut self, specs: &[ImportSpec]) {
        if specs.is_empty() {
            self.w("{}");
            return;
        }
        self.w("{ ");
        for (i, s) in specs.iter().enumerate() {
            if i > 0 {
                self.w(", ");
            }
            if s.type_only {
                self.w("type ");
            }
            self.ident(&s.name);
            if let Some(a) = &s.alias {
                self.w(" as ");
                self.ident(a);
            }
        }
        self.w(" }");
    }

    fn let_decl(&mut self, l: &LetDecl) {
        self.w(if l.kind == LetKind::Const { "const " } else { "let " });
        self.pattern(&l.pattern);
        if let Some(t) = &l.ty {
            self.w(": ");
            self.ty(t);
        }
        if let Some(init) = &l.init {
            self.w(" = ");
            self.expr(init);
        }
    }

    fn fn_decl(&mut self, f: &FnDecl) {
        if f.func.effect {
            self.w("effect ");
        }
        self.w("function ");
        self.ident(&f.name);
        self.func_sig_and_body(&f.func);
    }

    fn func_sig_and_body(&mut self, func: &Func) {
        self.type_params(&func.type_params);
        self.params(&func.params);
        if let Some(r) = &func.ret {
            self.w(": ");
            self.ty(r);
        }
        match &func.body {
            FuncBody::Block(b) => {
                self.w(" ");
                self.block(b);
            }
            FuncBody::Expr(e) => {
                self.w(" =");
                if self.has_newline(func.ret.as_ref().map_or(func.span.start, |r| r.span.end), e.span.start) {
                    self.indent += 1;
                    self.newline();
                    self.expr(e);
                    self.indent -= 1;
                } else {
                    self.w(" ");
                    self.expr(e);
                }
            }
        }
    }

    fn type_params(&mut self, tps: &[TypeParam]) {
        if tps.is_empty() {
            return;
        }
        self.w("<");
        for (i, tp) in tps.iter().enumerate() {
            if i > 0 {
                self.w(", ");
            }
            if tp.is_const {
                self.w("const ");
            }
            self.ident(&tp.name);
            if let Some(c) = &tp.constraint {
                self.w(" extends ");
                self.ty(c);
            }
            if let Some(d) = &tp.default {
                self.w(" = ");
                self.ty(d);
            }
        }
        self.w(">");
    }

    fn params(&mut self, params: &[Param]) {
        self.w("(");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.w(", ");
            }
            if p.rest {
                self.w("...");
            }
            self.pattern(&p.pattern);
            if p.optional {
                self.w("?");
            }
            if let Some(t) = &p.ty {
                self.w(": ");
                self.ty(t);
            }
            if let Some(d) = &p.default {
                self.w(" = ");
                self.expr(d);
            }
        }
        self.w(")");
    }

    fn pattern(&mut self, p: &Pattern) {
        match &p.kind {
            PatternKind::Ident(id) => self.ident(id),
            PatternKind::Object { props, rest } => {
                if props.is_empty() && rest.is_none() {
                    self.w("{}");
                    return;
                }
                self.w("{ ");
                for (i, prop) in props.iter().enumerate() {
                    if i > 0 {
                        self.w(", ");
                    }
                    self.ident(&prop.key);
                    if let Some(v) = &prop.value {
                        self.w(": ");
                        self.pattern(v);
                    }
                    if let Some(d) = &prop.default {
                        self.w(" = ");
                        self.expr(d);
                    }
                }
                if let Some(r) = rest {
                    if !props.is_empty() {
                        self.w(", ");
                    }
                    self.w("...");
                    self.ident(r);
                }
                self.w(" }");
            }
            PatternKind::Array { elems, rest } => {
                self.w("[");
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        self.w(", ");
                    }
                    if let Some(e) = e {
                        self.pattern(e);
                    }
                }
                if let Some(r) = rest {
                    if !elems.is_empty() {
                        self.w(", ");
                    }
                    self.w("...");
                    self.pattern(r);
                }
                if matches!(elems.last(), Some(None)) && rest.is_none() {
                    self.w(",");
                }
                self.w("]");
            }
            PatternKind::Default { pattern, default } => {
                self.pattern(pattern);
                self.w(" = ");
                self.expr(default);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Fields, members and variants
    // -----------------------------------------------------------------------------------------

    /// Prints a braced list either on one line or one entry per line, following the source.
    fn braced_list<T>(
        &mut self,
        items: &[T],
        span_of: impl Fn(&T) -> Span,
        open: &str,
        close: &str,
        multi: bool,
        mut print: impl FnMut(&mut Self, &T),
    ) {
        if items.is_empty() {
            self.w(open);
            self.w(close);
            return;
        }
        if !multi {
            self.w(open);
            if open == "{" {
                self.w(" ");
            }
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    self.w(", ");
                }
                print(self, it);
            }
            if open == "{" {
                self.w(" ");
            }
            self.w(close);
            return;
        }
        self.w(open);
        self.indent += 1;
        let mut prev: Option<u32> = None;
        for it in items {
            let span = span_of(it);
            self.newline();
            if prev.is_some() && self.blank_between(prev.unwrap(), span.start) {
                self.newline();
            }
            self.leading(span.start);
            print(self, it);
            self.trailing(span.end);
            prev = Some(span.end);
        }
        self.indent -= 1;
        self.newline();
        self.w(close);
    }

    fn members(&mut self, members: &[TypeMember], container: Span, force_multi: bool, terminate: bool) {
        let multi =
            force_multi && !members.is_empty() || self.list_is_multiline(container, members.first().map(|m| m.span));
        self.braced_list(
            members,
            |m| m.span,
            "{",
            "}",
            multi,
            |p, m| {
                p.member(m);
                if terminate && multi {
                    p.semi();
                }
            },
        );
    }

    /// Whether a list in `container` begins on a new line after its opening bracket.
    fn list_is_multiline(&self, container: Span, first: Option<Span>) -> bool {
        match first {
            Some(f) => {
                let before = &self.src[container.start as usize..f.start as usize];
                let open = before.rfind(['{', '(']).unwrap_or(0);
                before[open..].contains('\n')
            }
            None => false,
        }
    }

    fn member(&mut self, m: &TypeMember) {
        if m.readonly {
            self.w("readonly ");
        }
        match &m.kind {
            TypeMemberKind::Index { param, key, value } => {
                self.w("[");
                self.ident(param);
                self.w(": ");
                self.ty(key);
                self.w("]: ");
                self.ty(value);
            }
            TypeMemberKind::Prop(t) => {
                self.prop_key(&m.name);
                if m.optional {
                    self.w("?");
                }
                self.w(": ");
                self.ty(t);
            }
            TypeMemberKind::Method { type_params, params, ret } => {
                self.prop_key(&m.name);
                if m.optional {
                    self.w("?");
                }
                self.type_params(type_params);
                self.params(params);
                if let Some(r) = ret {
                    self.w(": ");
                    self.ty(r);
                }
            }
        }
    }

    fn schema_fields(&mut self, fields: &[SchemaField], open: &str, close: &str, container: Span) {
        let multi = self.list_is_multiline(container, fields.first().map(|f| f.span));
        self.braced_list(fields, |f| f.span, open, close, multi, |p, f| p.schema_field(f));
    }

    fn schema_field(&mut self, f: &SchemaField) {
        self.ident(&f.name);
        if f.optional {
            self.w("?");
        }
        match &f.value {
            SchemaFieldValue::Type(t) => {
                self.w(": ");
                self.ty(t);
            }
            SchemaFieldValue::Expr(e) => {
                self.w(" = ");
                self.expr(e);
            }
        }
        if let Some(d) = &f.default {
            self.w(" = ");
            self.expr(d);
        }
    }

    fn variants(&mut self, variants: &[Variant], container: Span) {
        let multi = self.list_is_multiline(container, variants.first().map(|v| v.span));
        self.braced_list(
            variants,
            |v| v.span,
            "{",
            "}",
            multi,
            |p, v| {
                p.ident(&v.name);
                if !v.fields.is_empty() {
                    p.w(" ");
                    p.schema_fields(&v.fields, "{", "}", v.span);
                }
            },
        );
    }

    // -----------------------------------------------------------------------------------------
    // Machines and components
    // -----------------------------------------------------------------------------------------

    fn section_open(&mut self, head: &str) {
        self.newline();
        self.w(head);
    }

    fn machine(&mut self, m: &Machine, span: Span) {
        self.w("machine ");
        self.ident(&m.name);
        if m.parallel {
            self.w(" parallel");
        }
        self.w(" {");
        self.indent += 1;
        let mut header_end: Option<u32> = None;
        if let Some(t) = &m.input {
            self.leading(t.span.start);
            self.section_open("input ");
            self.ty(t);
            header_end = Some(t.span.end);
        }
        if !m.root_fields.is_empty() {
            self.leading(m.root_fields[0].span.start);
            self.section_open("root ");
            self.schema_fields(&m.root_fields, "{", "}", span);
            header_end = Some(self.close_after(m.root_fields.last().unwrap().span.end));
        }
        if !m.events.is_empty() {
            self.leading(m.events[0].span.start);
            self.section_open("events ");
            let ev_container = Span::new(m.events[0].span.start.saturating_sub(1), m.events[0].span.start);
            let multi = m.events.len() > 1
                || self.has_newline(
                    self.src[..m.events[0].span.start as usize].rfind('{').unwrap_or(0) as u32,
                    m.events[0].span.start,
                );
            let _ = ev_container;
            self.braced_list(
                &m.events,
                |v| v.span,
                "{",
                "}",
                multi,
                |p, v| {
                    p.ident(&v.name);
                    if !v.fields.is_empty() {
                        p.w(" ");
                        p.schema_fields(&v.fields, "{", "}", v.span);
                    }
                },
            );
            header_end = Some(self.close_after(m.events.last().unwrap().span.end));
        }
        for (kind, label) in
            [(SourceKind::Effects, "effects"), (SourceKind::Streams, "streams"), (SourceKind::Timers, "timers")]
        {
            let entries: Vec<&MachineSource> = m.sources.iter().filter(|s| s.kind == kind).collect();
            if entries.is_empty() {
                continue;
            }
            self.leading(entries[0].name.span.start);
            self.section_open(label);
            self.w(" {");
            self.indent += 1;
            for s in entries {
                self.newline();
                self.leading(s.name.span.start);
                self.ident(&s.name);
                self.w(" = ");
                self.expr(&s.value);
                self.trailing(s.value.span.end);
            }
            self.indent -= 1;
            self.newline();
            self.w("}");
            header_end = Some(self.close_after(m.sources.iter().rfind(|s| s.kind == kind).unwrap().value.span.end));
        }
        self.state_body(&m.body, header_end);
        self.indent -= 1;
        self.leading(span.end);
        self.newline();
        self.w("}");
    }

    fn transition_span(t: &Transition) -> Span {
        match t {
            Transition::Target { span, .. } | Transition::Update { span, .. } => *span,
            Transition::Stay(s) => *s,
        }
    }

    fn state_body(&mut self, body: &StateBody, header_end: Option<u32>) {
        enum Entry<'a> {
            State(&'a StateDecl),
            On(&'a On),
            Invoke(&'a Invoke),
        }
        let mut entries: Vec<(Span, Entry)> = Vec::new();
        for s in &body.states {
            entries.push((s.span, Entry::State(s)));
        }
        for o in &body.ons {
            entries.push((o.span, Entry::On(o)));
        }
        for i in &body.invokes {
            entries.push((i.span, Entry::Invoke(i)));
        }
        entries.sort_by_key(|(s, _)| s.start);
        let mut prev: Option<u32> = header_end;
        for (span, e) in &entries {
            if prev.is_some_and(|p| self.blank_between(p, span.start)) {
                self.blank_line();
            } else {
                self.newline();
            }
            self.leading(span.start);
            match e {
                Entry::State(s) => self.state(s),
                Entry::On(o) => {
                    self.w("on ");
                    self.ident(&o.event);
                    if let Some(g) = &o.guard {
                        self.w(" if ");
                        self.expr(g);
                    }
                    self.w(" ");
                    self.transition(&o.transition);
                    self.semi();
                }
                Entry::Invoke(i) => self.invoke(i),
            }
            self.trailing(span.end);
            prev = Some(span.end);
        }
    }

    fn state(&mut self, s: &StateDecl) {
        if s.is_initial {
            self.w("initial ");
        }
        if s.is_final {
            self.w("final ");
        }
        if s.parallel {
            self.w("parallel ");
        }
        self.w("state ");
        self.ident(&s.name);
        if !s.fields.is_empty() {
            self.schema_fields(&s.fields, "(", ")", s.span);
        }
        let empty = s.body.states.is_empty() && s.body.ons.is_empty() && s.body.invokes.is_empty();
        if !empty {
            self.w(" {");
            self.indent += 1;
            self.state_body(&s.body, None);
            self.indent -= 1;
            self.leading(s.span.end);
            self.newline();
            self.w("}");
        } else if self.src[s.span.start as usize..s.span.end as usize].trim_end().ends_with('}') {
            self.w(" {}");
        }
    }

    fn transition_target(&mut self, t: &Transition) {
        match t {
            Transition::Target { path, data, .. } => {
                for (i, p) in path.iter().enumerate() {
                    if i > 0 {
                        self.w(".");
                    }
                    self.ident(p);
                }
                if let Some(d) = data {
                    self.w(" ");
                    self.expr(d);
                }
            }
            Transition::Stay(_) => self.w("stay"),
            Transition::Update { owner, data, .. } => {
                self.w("update ");
                self.ident(owner);
                self.w(" ");
                self.expr(data);
            }
        }
    }

    fn transition(&mut self, t: &Transition) {
        if !matches!(t, Transition::Update { .. }) {
            self.w("-> ");
        }
        self.transition_target(t);
    }

    fn invoke(&mut self, i: &Invoke) {
        match &i.src {
            InvokeSrc::After(delay) => {
                self.w("after ");
                self.expr(delay);
                self.w(" ");
                if let Some((_, t)) = i.handlers.first() {
                    self.transition(t);
                }
                self.semi();
            }
            InvokeSrc::Named(src) => {
                self.w("invoke ");
                self.ident(src);
                if let Some(input) = &i.input {
                    self.w("(");
                    self.expr(input);
                    self.w(")");
                }
                if !i.handlers.is_empty() {
                    self.w(" {");
                    self.indent += 1;
                    for (name, t) in &i.handlers {
                        self.newline();
                        self.leading(name.span.start);
                        self.ident(name);
                        self.w(" ");
                        self.transition(t);
                        self.semi();
                        self.trailing(Self::transition_span(t).end);
                    }
                    self.indent -= 1;
                    self.newline();
                    self.w("}");
                } else {
                    self.semi();
                }
            }
        }
    }

    fn component(&mut self, c: &Component, span: Span) {
        self.w("component ");
        self.ident(&c.name);
        self.w(" {");
        self.indent += 1;
        let mut prev_end: Option<u32> = None;
        for m in &c.members {
            if prev_end.is_some_and(|e| self.blank_between(e, m.span.start)) {
                self.blank_line();
            } else {
                self.newline();
            }
            self.leading(m.span.start);
            match &m.kind {
                MemberKind::Prop(f) | MemberKind::Let(f) => {
                    self.w(if matches!(m.kind, MemberKind::Prop(_)) { "prop " } else { "let " });
                    self.schema_field(f);
                    self.semi();
                }
                MemberKind::Event(v) => {
                    self.w("event ");
                    self.ident(&v.name);
                    if !v.fields.is_empty() {
                        self.schema_fields(&v.fields, "(", ")", v.span);
                    }
                    self.semi();
                }
                MemberKind::Update(u) => {
                    self.w("update ");
                    self.ident(&u.message);
                    if !u.fields.is_empty() {
                        self.schema_fields(&u.fields, "(", ")", u.span);
                    }
                    match &u.body {
                        UpdateBody::Block(b) => {
                            self.w(" ");
                            self.block(b);
                        }
                        UpdateBody::Expr(e) => {
                            self.w(" =>");
                            let after = u.fields.last().map_or(u.message.span.end, |f| f.span.end);
                            self.arrow_rhs(after, e);
                            self.semi();
                        }
                    }
                }
                MemberKind::View(v) => {
                    self.w("view =>");
                    self.arrow_rhs(m.span.start + 4, v);
                    if !matches!(v.kind, ExprKind::Markup(_)) {
                        self.semi();
                    }
                }
                MemberKind::Option(k, v) => {
                    self.ident(k);
                    self.w(" = ");
                    self.expr(v);
                    self.semi();
                }
            }
            self.trailing(m.span.end);
            prev_end = Some(m.span.end);
        }
        self.indent -= 1;
        self.leading(span.end);
        self.newline();
        self.w("}");
    }

    /// The position just after the `}` that closes a section ending at `pos`.
    fn close_after(&self, pos: u32) -> u32 {
        self.src[pos as usize..].find('}').map_or(pos, |i| pos + i as u32 + 1)
    }

    /// The right-hand side of `=>`: on the same line, or indented on the next if the source did so.
    fn arrow_rhs(&mut self, after: u32, e: &Expr) {
        if self.has_newline(after, e.span.start) {
            self.indent += 1;
            self.newline();
            self.expr(e);
            self.indent -= 1;
        } else {
            self.w(" ");
            self.expr(e);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------------------------

    fn block(&mut self, b: &Block) {
        if b.stmts.is_empty() && !self.comments.get(self.next).is_some_and(|c| c.span.start < b.span.end) {
            self.w("{}");
            return;
        }
        self.w("{");
        self.indent += 1;
        let mut prev = None;
        for s in &b.stmts {
            if prev.is_none() {
                self.newline();
                self.leading(s.span.start);
            } else {
                self.seq(prev, s.span.start);
            }
            self.stmt_semi(s);
            self.trailing(s.span.end);
            prev = Some(s.span.end);
        }
        // Comments before the closing brace.
        if self.comments.get(self.next).is_some_and(|c| c.span.start < b.span.end) {
            self.newline();
            self.leading(b.span.end);
            while self.out.ends_with([' ', '\n']) {
                self.out.pop();
            }
        }
        self.indent -= 1;
        self.newline();
        self.w("}");
    }

    /// A statement followed by its `;` where one is allowed.
    fn stmt_semi(&mut self, s: &Stmt) {
        self.stmt(s);
        let needs = match &s.kind {
            StmtKind::Fn(f) => fn_needs_semi(f),
            StmtKind::If(_) | StmtKind::For { .. } | StmtKind::While { .. } | StmtKind::Block(_) => false,
            _ => true,
        };
        if needs {
            self.semi();
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let(l) => self.let_decl(l),
            StmtKind::Fn(f) => self.fn_decl(f),
            StmtKind::Expr(e) => self.expr(e),
            StmtKind::Return(e) => {
                self.w("return");
                if let Some(e) = e {
                    self.w(" ");
                    self.expr(e);
                }
            }
            StmtKind::If(i) => self.if_stmt(i),
            StmtKind::For { kind, pattern, iter, body, bind } => {
                self.w("for ");
                if *bind {
                    self.w("run ");
                }
                if *kind == LetKind::Let {
                    self.w("let ");
                }
                self.pattern(pattern);
                self.w(" of ");
                self.expr(iter);
                self.w(" ");
                self.block(body);
            }
            StmtKind::While { cond, body } => {
                self.w("while ");
                self.expr(cond);
                self.w(" ");
                self.block(body);
            }
            StmtKind::Break => self.w("break"),
            StmtKind::Continue => self.w("continue"),
            StmtKind::Fail(e) => {
                self.w("fail ");
                self.expr(e);
            }
            StmtKind::Emit(e) => {
                self.w("emit ");
                self.expr(e);
            }
            StmtKind::Defer(e) => {
                self.w("defer ");
                self.expr(e);
            }
            StmtKind::Throw(e) => {
                self.w("throw ");
                self.expr(e);
            }
            StmtKind::Block(b) => self.block(b),
        }
    }

    fn if_stmt(&mut self, i: &IfStmt) {
        self.w("if ");
        self.expr(&i.cond);
        self.w(" ");
        self.block(&i.then);
        if let Some(e) = &i.else_ {
            self.w(" else ");
            match &e.kind {
                StmtKind::If(inner) => self.if_stmt(inner),
                StmtKind::Block(b) => self.block(b),
                _ => self.stmt_semi(e),
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------------------------

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(_) | ExprKind::Lit(_) => self.raw(e.span),
            ExprKind::Template(parts) => {
                for p in parts {
                    match p {
                        TemplatePart::Str(s) => self.raw(*s),
                        TemplatePart::Expr(x) => self.expr(x),
                    }
                }
            }
            ExprKind::Array(elems) => {
                let multi = self.list_is_multiline_br(e.span, '[', elems.first().and_then(elem_span));
                self.list(elems, "[", "]", multi);
            }
            ExprKind::Object(props) => self.object(props, e.span),
            ExprKind::Paren(x) => {
                self.w("(");
                self.expr(x);
                self.w(")");
            }
            ExprKind::Unary { op, expr } => {
                self.w(op);
                if op.chars().all(char::is_alphabetic) {
                    self.w(" ");
                }
                self.expr(expr);
            }
            ExprKind::Update { op, prefix, expr } => {
                if *prefix {
                    self.w(op);
                }
                self.expr(expr);
                if !*prefix {
                    self.w(op);
                }
            }
            ExprKind::Binary { op, left, right } => {
                self.expr(left);
                if self.has_newline(left.span.end, right.span.start) {
                    self.indent += 1;
                    self.newline();
                    self.w(op);
                    self.w(" ");
                    self.indent -= 1;
                } else {
                    self.w(&format!(" {op} "));
                }
                self.expr(right);
            }
            ExprKind::Assign { op, target, value } => {
                self.expr(target);
                self.w(&format!(" {op} "));
                self.expr(value);
            }
            ExprKind::Cond { cond, then, else_ } => {
                self.expr(cond);
                self.w(" ? ");
                self.expr(then);
                self.w(" : ");
                self.expr(else_);
            }
            ExprKind::Call { callee, type_args, args, optional } => {
                self.expr(callee);
                if *optional {
                    self.w("?.");
                }
                self.type_args(type_args);
                let multi = self.list_is_multiline_br(
                    Span::new(callee.span.end, e.span.end),
                    '(',
                    args.first().and_then(elem_span),
                );
                self.list(args, "(", ")", multi);
            }
            ExprKind::Member { object, name, optional } => {
                self.expr(object);
                if self.has_newline(object.span.end, name.span.start) {
                    self.indent += 1;
                    self.newline();
                    self.indent -= 1;
                }
                self.w(if *optional { "?." } else { "." });
                self.ident(name);
            }
            ExprKind::Index { object, index, optional } => {
                self.expr(object);
                self.w(if *optional { "?.[" } else { "[" });
                self.expr(index);
                self.w("]");
            }
            ExprKind::New { callee, type_args, args } => {
                self.w("new ");
                self.expr(callee);
                self.type_args(type_args);
                self.list(args, "(", ")", false);
            }
            ExprKind::Func(f) => self.func_expr(f),
            ExprKind::Pipe(parts) => {
                self.expr(&parts[0]);
                let mut indented = false;
                for w in parts.windows(2) {
                    if self.has_newline(w[0].span.end, w[1].span.start) {
                        if !indented {
                            self.indent += 1;
                            indented = true;
                        }
                        self.newline();
                        self.w("|> ");
                    } else {
                        self.w(" |> ");
                    }
                    self.expr(&w[1]);
                }
                if indented {
                    self.indent -= 1;
                }
            }
            ExprKind::With { expr, layer } => {
                self.expr(expr);
                if self.has_newline(expr.span.end, layer.span.start) {
                    self.indent += 1;
                    self.newline();
                    self.w("with ");
                    self.indent -= 1;
                } else {
                    self.w(" with ");
                }
                self.expr(layer);
            }
            ExprKind::Bind(kind, x) => {
                self.w(kind.keyword());
                self.w(" ");
                self.expr(x);
            }
            ExprKind::Catch { expr, handler } => {
                self.expr(expr);
                let handler_start = match handler {
                    CatchHandler::Tags(arms) => arms.first().map_or(e.span.end, |a| a.span.start),
                    CatchHandler::All { binding, .. } => binding.span.start,
                };
                let kw = self.src[expr.span.end as usize..handler_start as usize]
                    .find("catch")
                    .map_or(expr.span.end, |i| expr.span.end + i as u32);
                let broke = self.has_newline(expr.span.end, kw);
                if broke {
                    self.indent += 1;
                    self.newline();
                    self.w("catch ");
                } else {
                    self.w(" catch ");
                }
                match handler {
                    CatchHandler::Tags(arms) => {
                        self.w("{");
                        self.indent += 1;
                        for a in arms {
                            self.newline();
                            self.leading(a.span.start);
                            self.ident(&a.tag);
                            if let Some(b) = &a.binding {
                                self.w("(");
                                self.pattern(b);
                                self.w(")");
                            }
                            self.w(" =>");
                            self.arrow_rhs(a.tag.span.end, &a.body);
                            self.trailing(a.span.end);
                        }
                        self.indent -= 1;
                        self.newline();
                        self.w("}");
                    }
                    CatchHandler::All { binding, body } => {
                        self.pattern(binding);
                        self.w(" => ");
                        self.expr(body);
                    }
                }
                if broke {
                    self.indent -= 1;
                }
            }
            ExprKind::Match { expr, arms } => {
                self.w("match ");
                self.expr(expr);
                self.w(" {");
                self.indent += 1;
                for a in arms {
                    self.newline();
                    self.leading(a.span.start);
                    let pat_end = match &a.pattern {
                        MatchPattern::Tag { tag, binding } => {
                            self.ident(tag);
                            if let Some(b) = binding {
                                self.w("(");
                                self.pattern(b);
                                self.w(")");
                            }
                            binding.as_ref().map_or(tag.span.end, |b| b.span.end)
                        }
                        MatchPattern::Lit(_, s) => {
                            self.raw(*s);
                            s.end
                        }
                        MatchPattern::Wildcard(s) => {
                            self.w("_");
                            s.end
                        }
                    };
                    self.w(" =>");
                    self.arrow_rhs(pat_end, &a.body);
                    self.trailing(a.span.end);
                }
                self.indent -= 1;
                self.newline();
                self.w("}");
            }
            ExprKind::If { cond, then, else_ } => {
                self.w("if ");
                self.expr(cond);
                self.w(" { ");
                self.expr(then);
                self.w(" } else ");
                if matches!(else_.kind, ExprKind::If { .. }) {
                    self.expr(else_);
                } else {
                    self.w("{ ");
                    self.expr(else_);
                    self.w(" }");
                }
            }
            ExprKind::EffectBlock(b) => {
                self.w("effect ");
                self.block(b);
            }
            ExprKind::Markup(el) => self.element(el),
            ExprKind::As { expr, ty } => {
                self.expr(expr);
                self.w(" as ");
                self.ty(ty);
            }
            ExprKind::Satisfies { expr, ty } => {
                self.expr(expr);
                self.w(" satisfies ");
                self.ty(ty);
            }
            ExprKind::NonNull(x) => {
                self.expr(x);
                self.w("!");
            }
            ExprKind::Error => self.raw(e.span),
        }
    }

    fn list_is_multiline_br(&self, span: Span, open: char, first: Option<Span>) -> bool {
        let Some(f) = first else { return false };
        let before = &self.src[span.start as usize..f.start as usize];
        before.rfind(open).is_some_and(|i| before[i..].contains('\n'))
    }

    fn list(&mut self, elems: &[ArrayElem], open: &str, close: &str, multi: bool) {
        if !multi {
            self.w(open);
            for (i, a) in elems.iter().enumerate() {
                if i > 0 {
                    self.w(", ");
                }
                self.elem(a);
            }
            if matches!(elems.last(), Some(ArrayElem::Hole)) {
                self.w(",");
            }
            self.w(close);
            return;
        }
        self.w(open);
        self.indent += 1;
        for a in elems {
            self.newline();
            if let Some(s) = elem_span(a) {
                self.leading(s.start);
            }
            self.elem(a);
            self.w(",");
            if let Some(s) = elem_span(a) {
                self.trailing(s.end);
            }
        }
        self.indent -= 1;
        self.newline();
        self.w(close);
    }

    fn elem(&mut self, a: &ArrayElem) {
        match a {
            ArrayElem::Expr(x) => self.expr(x),
            ArrayElem::Spread(x) => {
                self.w("...");
                self.expr(x);
            }
            ArrayElem::Hole => {}
        }
    }

    fn type_args(&mut self, args: &Option<Vec<Type>>) {
        if let Some(args) = args {
            self.w("<");
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    self.w(", ");
                }
                self.ty(a);
            }
            self.w(">");
        }
    }

    fn func_expr(&mut self, f: &Func) {
        if f.effect {
            self.w("effect ");
        }
        if !f.arrow {
            self.w("function ");
            self.func_sig_and_body(f);
            return;
        }
        self.type_params(&f.type_params);
        self.params(&f.params);
        if let Some(r) = &f.ret {
            self.w(": ");
            self.ty(r);
        }
        self.w(" =>");
        match &f.body {
            FuncBody::Block(b) => {
                self.w(" ");
                self.block(b);
            }
            FuncBody::Expr(x) => {
                let arrow_end =
                    f.ret.as_ref().map_or(f.params.last().map_or(f.span.start, |p| p.span.end), |r| r.span.end);
                self.arrow_rhs(arrow_end + 1, x);
            }
        }
    }

    fn object(&mut self, props: &[Prop], span: Span) {
        if props.is_empty() {
            self.w("{}");
            return;
        }
        let first = prop_span(&props[0]);
        let multi = self.has_newline(span.start, first.start);
        if !multi {
            self.w("{ ");
            for (i, p) in props.iter().enumerate() {
                if i > 0 {
                    self.w(", ");
                }
                self.prop(p);
            }
            self.w(" }");
            return;
        }
        self.w("{");
        self.indent += 1;
        let mut prev: Option<u32> = None;
        for p in props {
            let s = prop_span(p);
            self.newline();
            if prev.is_some_and(|e| self.blank_between(e, s.start)) {
                self.newline();
            }
            self.leading(s.start);
            self.prop(p);
            self.w(",");
            self.trailing(s.end);
            prev = Some(s.end);
        }
        self.indent -= 1;
        self.newline();
        self.w("}");
    }

    fn prop(&mut self, p: &Prop) {
        match p {
            Prop::KeyValue { key, value } => {
                self.prop_key(key);
                self.w(": ");
                self.expr(value);
            }
            Prop::Shorthand(id) => self.ident(id),
            Prop::Spread(x) => {
                self.w("...");
                self.expr(x);
            }
            Prop::Method { key, func } => {
                if func.effect {
                    self.w("effect ");
                }
                self.prop_key(key);
                self.func_sig_and_body(func);
            }
        }
    }

    fn prop_key(&mut self, k: &PropKey) {
        match k {
            PropKey::Ident(i) => self.ident(i),
            PropKey::Str(s) => self.raw(s.span),
            PropKey::Num(_, s) => self.raw(*s),
            PropKey::Computed(e) => {
                self.w("[");
                self.expr(e);
                self.w("]");
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Markup
    // -----------------------------------------------------------------------------------------

    fn element(&mut self, el: &Element) {
        self.w("<");
        let name: Vec<&str> = el.name.iter().map(|i| i.name.as_str()).collect();
        let name = name.join(".");
        self.w(&name);
        let attr_span = |a: &Attr| match a {
            Attr::Named { span, .. } => *span,
            Attr::Spread(x) => x.span,
        };
        let multi_attrs =
            el.attrs.first().is_some_and(|a| self.has_newline(el.name.last().unwrap().span.end, attr_span(a).start));
        if multi_attrs {
            self.indent += 1;
        }
        for a in &el.attrs {
            if multi_attrs {
                self.newline();
            } else {
                self.w(" ");
            }
            match a {
                Attr::Named { name, value, .. } => {
                    self.w(&name.name);
                    match value {
                        Some(AttrValue::Str(s)) => {
                            self.w("=");
                            self.raw(s.span);
                        }
                        Some(AttrValue::Expr(x)) => {
                            self.w("={");
                            self.expr(x);
                            self.w("}");
                        }
                        None => {}
                    }
                }
                Attr::Spread(x) => {
                    self.w("{...");
                    self.expr(x);
                    self.w("}");
                }
            }
        }
        if multi_attrs {
            self.indent -= 1;
            self.newline();
        }
        if el.self_closing {
            self.w(if multi_attrs { "/>" } else { " />" });
            return;
        }
        self.w(">");
        let multi = self.multiline(el.span)
            && el.children.iter().any(|c| !matches!(c, Child::Text(t, _) if t.trim().is_empty()));
        if !multi {
            for c in &el.children {
                self.child_inline(c);
            }
        } else {
            self.indent += 1;
            // Children that share a source line stay together, exactly spaced.
            let mut prev_end: Option<u32> = None;
            for c in &el.children {
                let span = child_span(c);
                let starts_line = prev_end.is_none_or(|e| self.has_newline(e, span.start));
                match c {
                    Child::Text(raw, span) => {
                        let lines: Vec<&str> = raw.split('\n').collect();
                        for (i, line) in lines.iter().enumerate() {
                            let mut l = *line;
                            if i > 0 || starts_line {
                                l = l.trim_start();
                            }
                            if i + 1 < lines.len() {
                                l = l.trim_end();
                            }
                            if (i > 0 || starts_line) && !l.trim().is_empty() {
                                self.newline();
                            }
                            if i + 1 == lines.len() && lines.len() > 1 && l.trim().is_empty() {
                                continue;
                            }
                            if !(lines.len() > 1 && l.trim().is_empty()) {
                                self.w(l);
                            }
                        }
                        let _ = span;
                    }
                    other => {
                        if starts_line {
                            self.newline();
                        }
                        self.child_inline(other);
                    }
                }
                prev_end = Some(span.end);
            }
            self.indent -= 1;
            self.newline();
        }
        self.w("</");
        self.w(&name);
        self.w(">");
    }

    fn child_inline(&mut self, c: &Child) {
        match c {
            Child::Text(_, span) => self.raw(*span),
            Child::Expr(x) => {
                self.w("{");
                self.expr(x);
                self.w("}");
            }
            Child::Element(e) => self.element(e),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------------------------

    fn ty(&mut self, t: &Type) {
        match &t.kind {
            TypeKind::Ref { name, args } => {
                for (i, n) in name.iter().enumerate() {
                    if i > 0 {
                        self.w(".");
                    }
                    self.ident(n);
                }
                self.type_args(args);
            }
            TypeKind::Lit(_) | TypeKind::Template(_) => self.raw(t.span),
            TypeKind::Array(inner) => {
                self.ty(inner);
                self.w("[]");
            }
            TypeKind::Tuple(elems) => {
                self.w("[");
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        self.w(", ");
                    }
                    if e.rest {
                        self.w("...");
                    }
                    if let Some(n) = &e.name {
                        self.ident(n);
                        if e.optional {
                            self.w("?");
                        }
                        self.w(": ");
                        self.ty(&e.ty);
                    } else {
                        self.ty(&e.ty);
                        if e.optional {
                            self.w("?");
                        }
                    }
                }
                self.w("]");
            }
            TypeKind::Object(members) => self.members(members, t.span, false, false),
            TypeKind::Union(ms) | TypeKind::Intersection(ms) => {
                let sep = if matches!(t.kind, TypeKind::Union(_)) { " | " } else { " & " };
                for (i, m) in ms.iter().enumerate() {
                    if i > 0 {
                        self.w(sep);
                    }
                    self.ty(m);
                }
            }
            TypeKind::Fn { type_params, params, ret } => {
                self.type_params(type_params);
                self.params(params);
                self.w(" => ");
                self.ty(ret);
            }
            TypeKind::Paren(inner) => {
                self.w("(");
                self.ty(inner);
                self.w(")");
            }
            TypeKind::Typeof(name) => {
                self.w("typeof ");
                for (i, n) in name.iter().enumerate() {
                    if i > 0 {
                        self.w(".");
                    }
                    self.ident(n);
                }
            }
            TypeKind::Keyof(inner) => {
                self.w("keyof ");
                self.ty(inner);
            }
            TypeKind::Readonly(inner) => {
                self.w("readonly ");
                self.ty(inner);
            }
            TypeKind::Indexed { object, index } => {
                self.ty(object);
                self.w("[");
                self.ty(index);
                self.w("]");
            }
        }
    }
}

/// Function declarations end in their own block, except the `= expr` form.
fn fn_needs_semi(f: &FnDecl) -> bool {
    matches!(f.func.body, FuncBody::Expr(_))
}

fn elem_span(a: &ArrayElem) -> Option<Span> {
    match a {
        ArrayElem::Expr(x) | ArrayElem::Spread(x) => Some(x.span),
        ArrayElem::Hole => None,
    }
}

fn prop_span(p: &Prop) -> Span {
    match p {
        Prop::KeyValue { key, value } => key.span().to(value.span),
        Prop::Shorthand(id) => id.span,
        Prop::Spread(x) => x.span,
        Prop::Method { key, func } => key.span().to(func.span),
    }
}

fn child_span(c: &Child) -> Span {
    match c {
        Child::Text(_, s) => *s,
        Child::Expr(x) => x.span,
        Child::Element(e) => e.span,
    }
}
