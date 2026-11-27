//! `efs`: build, check, format, lint and serve EffScript projects.

mod config;
mod init;
mod report;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use efs_syntax::Severity;

use crate::report::Reporter;

#[derive(Parser)]
#[command(name = "efs", version, about = "EffScript: a readable syntax for Effect, compiled to TypeScript")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile .efs files to TypeScript.
    Build {
        /// Files or directories (default: the current directory).
        paths: Vec<PathBuf>,
        /// Write output under this directory instead of next to each source file.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Also write a `.ts.map` source map for each file.
        #[arg(long)]
        source_map: bool,
        /// Extension that replaces `.efs` in relative imports (default `.js`).
        #[arg(long)]
        import_extension: Option<String>,
        /// Rebuild when files change.
        #[arg(long, short)]
        watch: bool,
        /// Output language: `ts` (default) or `js`.
        #[arg(long, value_name = "LANG")]
        emit: Option<String>,
        /// With `--emit js`, also write `.d.ts` declarations (types inferred by tsrs).
        #[arg(long)]
        declaration: bool,
    },
    /// Type check a project with tsrs (the Rust port of the TypeScript checker).
    Check {
        /// A file or directory inside the project (default: the current directory).
        path: Option<PathBuf>,
        /// Only report diagnostics in .efs files.
        #[arg(long)]
        efs_only: bool,
        /// Print diagnostics as JSON lines.
        #[arg(long)]
        json: bool,
        /// Also report Effect Language Service diagnostics from effect-tsgo (`@effect/tsgo`).
        /// Writes the build output next to each source file, as `efs build` does.
        #[arg(long)]
        effect: bool,
    },
    /// Format .efs files.
    Fmt {
        paths: Vec<PathBuf>,
        /// Report files that are not formatted instead of rewriting them.
        #[arg(long)]
        check: bool,
        /// Format standard input to standard output.
        #[arg(long)]
        stdin: bool,
    },
    /// Lint .efs files.
    Lint {
        paths: Vec<PathBuf>,
        /// Exit with an error when there are warnings.
        #[arg(long)]
        deny_warnings: bool,
        /// Turn off a rule (repeatable).
        #[arg(long = "disable", value_name = "RULE")]
        disabled: Vec<String>,
        /// List the available rules.
        #[arg(long)]
        rules: bool,
    },
    /// Print the TypeScript generated for a file.
    Emit {
        file: PathBuf,
        /// Print the span map (TypeScript content-mapper format) instead of the code.
        #[arg(long)]
        span_map: bool,
        /// Print a Source Map v3 instead of the code.
        #[arg(long)]
        source_map: bool,
        /// Print JavaScript instead of TypeScript.
        #[arg(long)]
        js: bool,
    },
    /// Print the type at a position: `efs type-at src/main.efs:12:8`.
    TypeAt { location: String },
    /// Run the language server on stdio.
    Lsp,
    /// Serve the TypeScript 7.1 content-mapper protocol on stdio (used by tsgo/tsrs).
    Mapper,
    /// Create a new EffScript project.
    Init {
        /// Directory to create the project in (default: the current directory).
        dir: Option<PathBuf>,
    },
}

const SKIP_DIRS: &[&str] = &["node_modules", "target", "dist", "build", ".git"];

fn collect(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_file() {
            out.push(p.to_path_buf());
            return;
        }
        let Ok(entries) = std::fs::read_dir(p) else { return };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                    walk(&path, out);
                }
            } else if name.ends_with(".efs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    let defaults = [PathBuf::from(".")];
    for p in if paths.is_empty() { &defaults[..] } else { paths } {
        walk(p, &mut out);
    }
    out
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut r = Reporter::new();
    match cli.command {
        Command::Build { paths, out_dir, source_map, import_extension, watch, emit, declaration } => {
            let mut cfg = config::Config::load(paths.first().map(PathBuf::as_path).unwrap_or(Path::new(".")));
            if let Some(e) = import_extension {
                cfg.import_extension = e;
            }
            if out_dir.is_some() {
                cfg.out_dir = out_dir;
            }
            cfg.source_map |= source_map;
            cfg.declaration |= declaration;
            match emit.as_deref() {
                Some("js") => cfg.emit_js = true,
                Some("ts") => cfg.emit_js = false,
                Some(other) => {
                    r.error_line(&format!("unknown `--emit {other}`; expected `ts` or `js`"));
                    return r.exit_code(false);
                }
                None => {}
            }
            if cfg.declaration && !cfg.emit_js {
                r.error_line("`--declaration` needs `--emit js`; TypeScript output already carries its types");
                return r.exit_code(false);
            }
            if watch {
                return build_watch(&paths, &cfg, &mut r);
            }
            build(&paths, &cfg, &mut r);
        }
        Command::Check { path, efs_only, json, effect } => {
            let start = path.unwrap_or_else(|| PathBuf::from("."));
            let cfg = config::Config::load(&start);
            let mut ws = efs_check::Workspace::discover(&start);
            ws.codegen = cfg.codegen();
            let mut session = efs_check::Session::new(ws);
            let started = std::time::Instant::now();
            let mut diags = efs_check::compat::check_versions(&session.workspace.root);
            diags.extend(session.efs_diagnostics());
            let local: Vec<(Option<PathBuf>, u32)> =
                diags.iter().filter_map(|d| Some((d.file.clone(), d.line_col()?.0))).collect();
            for d in session.type_diagnostics(efs_only) {
                // Skip type errors on lines that already have an EffScript error.
                let line = d.line_col().map(|(l, _)| l);
                if line.is_some_and(|l| local.iter().any(|(f, ll)| *f == d.file && *ll == l)) {
                    continue;
                }
                diags.push(d);
            }
            if effect {
                let root = session.workspace.root.clone();
                match efs_check::effect_tsgo::find(&root) {
                    Some(exe) => match session.effect_diagnostics(&exe) {
                        Ok(found) => diags.extend(found.into_iter().filter(|d| !efs_only || d.file.as_ref().is_some_and(|f| f.extension().is_some_and(|e| e == "efs")))),
                        Err(e) => r.error_line(&e),
                    },
                    None => r.error_line(
                        "`--effect` needs effect-tsgo: install it with `pnpm add -D @effect/tsgo tsrs`, or set EFS_EFFECT_TSGO",
                    ),
                }
            }
            for d in &diags {
                if json {
                    r.json(d);
                } else {
                    r.check_diagnostic(d);
                }
            }
            if !json {
                let files = session.compile().len();
                r.summary(&format!(
                    "checked {files} EffScript file{} in {:.2?}",
                    if files == 1 { "" } else { "s" },
                    started.elapsed()
                ));
            }
        }
        Command::Fmt { paths, check, stdin } => {
            if stdin {
                let mut src = String::new();
                std::io::stdin().read_to_string(&mut src).ok();
                let opts = config::Config::load(Path::new(".")).fmt_options();
                match efs_fmt::format_with(&src, &opts) {
                    Ok(out) => print!("{out}"),
                    Err(diags) => {
                        for d in diags {
                            r.diagnostic(Path::new("<stdin>"), &src, &d);
                        }
                    }
                }
                return r.exit_code(false);
            }
            let mut changed = 0;
            for file in collect(&paths) {
                let Ok(src) = std::fs::read_to_string(&file) else { continue };
                let opts = config::Config::load(&file).fmt_options();
                match efs_fmt::format_with(&src, &opts) {
                    Ok(out) if out != src => {
                        changed += 1;
                        if check {
                            r.error_line(&format!("{} is not formatted", file.display()));
                        } else if let Err(e) = std::fs::write(&file, out) {
                            r.error_line(&format!("{}: {e}", file.display()));
                        }
                    }
                    Ok(_) => {}
                    Err(diags) => {
                        for d in diags {
                            r.diagnostic(&file, &src, &d);
                        }
                    }
                }
            }
            if !check && changed > 0 {
                r.summary(&format!("formatted {changed} file{}", if changed == 1 { "" } else { "s" }));
            }
        }
        Command::Lint { paths, deny_warnings, disabled, rules } => {
            if rules {
                for rule in efs_lint::RULES {
                    println!(
                        "{:<20} {:<8} {}",
                        rule.name,
                        format!("{:?}", rule.severity).to_lowercase(),
                        rule.description
                    );
                }
                return ExitCode::SUCCESS;
            }
            let mut cfg = config::Config::load(paths.first().map(PathBuf::as_path).unwrap_or(Path::new(".")));
            cfg.lint_disabled.extend(disabled);
            let lint_cfg = efs_lint::Config { disabled: cfg.lint_disabled.clone() };
            for file in collect(&paths) {
                let Ok(src) = std::fs::read_to_string(&file) else { continue };
                let parsed = efs_syntax::parse(&src);
                for d in &parsed.diagnostics {
                    r.diagnostic(&file, &src, d);
                }
                if parsed.diagnostics.is_empty() {
                    for d in efs_lint::lint(&src, &parsed.module, &lint_cfg) {
                        r.diagnostic(&file, &src, &d);
                    }
                }
            }
            return r.exit_code(deny_warnings);
        }
        Command::Emit { file, span_map, source_map, js } => {
            let Ok(src) = std::fs::read_to_string(&file) else {
                r.error_line(&format!("cannot read {}", file.display()));
                return r.exit_code(false);
            };
            let cfg = config::Config::load(&file);
            let mut opts = cfg.codegen();
            if js {
                opts.target = efs_codegen::Target::JavaScript;
            }
            let out = efs_codegen::compile(&src, &opts);
            for d in &out.diagnostics {
                r.diagnostic(&file, &src, d);
            }
            if span_map {
                println!("{}", out.span_map_json());
            } else if source_map {
                let ts = efs_check::output_path(&file);
                println!("{}", efs_codegen::sourcemap::source_map(&out, &src, &file_name(&file), &file_name(&ts)));
            } else {
                print!("{}", out.code);
            }
        }
        Command::TypeAt { location } => {
            let mut parts = location.rsplitn(3, ':');
            let (Some(col), Some(line), Some(file)) = (parts.next(), parts.next(), parts.next()) else {
                r.error_line("expected FILE:LINE:COLUMN");
                return r.exit_code(false);
            };
            let (Ok(line), Ok(col)) = (line.parse::<u32>(), col.parse::<u32>()) else {
                r.error_line("expected FILE:LINE:COLUMN");
                return r.exit_code(false);
            };
            let path = std::fs::canonicalize(file).unwrap_or_else(|_| PathBuf::from(file));
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            let offset = efs_syntax::LineIndex::new(&src).offset(line.saturating_sub(1), col.saturating_sub(1));
            let mut ws = efs_check::Workspace::discover(&path);
            ws.codegen = config::Config::load(&path).codegen();
            let mut session = efs_check::Session::new(ws);
            match session.hover(&path, offset) {
                Some(t) => println!("{t}"),
                None => r.error_line("no type information at this position"),
            }
        }
        Command::Lsp => {
            if let Err(e) = efs_lsp::Server::run_stdio() {
                r.error_line(&format!("language server error: {e}"));
            }
        }
        Command::Mapper => {
            if let Err(e) = efs_mapper::Server::default().run_stdio() {
                r.error_line(&format!("content mapper error: {e}"));
            }
        }
        Command::Init { dir } => {
            let dir = dir.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = init::init(&dir) {
                r.error_line(&e);
            } else {
                println!(
                    "Created an EffScript project in {}.\nNext: pnpm install, then `efs check` and `efs build`.",
                    dir.display()
                );
            }
        }
    }
    r.exit_code(false)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Compiles every file. Returns the number of files written.
/// Where the output for `file` goes, with extension `ext`.
fn target_for(file: &Path, root: &Path, cfg: &config::Config, ext: &str) -> PathBuf {
    match &cfg.out_dir {
        Some(dir) => {
            let abs = std::fs::canonicalize(file).unwrap_or(file.to_path_buf());
            let rel = abs.strip_prefix(root).unwrap_or(&abs);
            dir.join(rel).with_extension(ext)
        }
        None => file.with_extension(ext),
    }
}

/// Writes `text` to `target` unless unchanged or hand-written. Returns whether it was written.
fn write_output(file: &Path, target: &Path, text: &str, r: &mut Reporter) -> bool {
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if efs_check::is_handwritten(target) {
        r.error_line(&format!(
            "{}: not overwriting {}, which was not generated by efs; rename one of them",
            file.display(),
            target.display()
        ));
        return false;
    }
    if std::fs::read_to_string(target).is_ok_and(|old| old == text) {
        return false;
    }
    if let Err(e) = std::fs::write(target, text) {
        r.error_line(&format!("{}: {e}", target.display()));
        return false;
    }
    true
}

/// Compiles every file. Returns the number of files written.
fn build(paths: &[PathBuf], cfg: &config::Config, r: &mut Reporter) -> usize {
    let mut opts = cfg.codegen();
    let ext = if cfg.emit_js { "js" } else { "ts" };
    if cfg.emit_js {
        opts.target = efs_codegen::Target::JavaScript;
    }
    let root = std::fs::canonicalize(paths.first().map(PathBuf::as_path).unwrap_or(Path::new("."))).unwrap_or_default();
    let root = if root.is_file() { root.parent().map(Path::to_path_buf).unwrap_or_default() } else { root };
    let mut written = 0;
    let mut built = Vec::new();
    for file in collect(paths) {
        let Ok(src) = std::fs::read_to_string(&file) else { continue };
        let out = efs_codegen::compile(&src, &opts);
        let has_errors = out.diagnostics.iter().any(|d| d.severity == Severity::Error);
        for d in &out.diagnostics {
            r.diagnostic(&file, &src, d);
        }
        if has_errors {
            continue;
        }
        let target = target_for(&file, &root, cfg, ext);
        let mut code = efs_check::generated_header(&file_name(&file));
        let header_lines = 1;
        code.push_str(&out.code);
        if cfg.source_map {
            let map_path = PathBuf::from(format!("{}.map", target.display()));
            let source_rel = relative(map_path.parent().unwrap_or(Path::new(".")), &file);
            let mut shifted = out.clone();
            shifted.code = format!("{}{}", "\n".repeat(header_lines), out.code);
            for s in &mut shifted.segments {
                s.gen_start += header_lines as u32;
            }
            let map = efs_codegen::sourcemap::source_map(&shifted, &src, &source_rel, &file_name(&target));
            code.push_str(&format!("//# sourceMappingURL={}\n", file_name(&map_path)));
            if let Some(parent) = map_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&map_path, map) {
                r.error_line(&format!("{}: {e}", map_path.display()));
            }
        }
        if write_output(&file, &target, &code, r) {
            written += 1;
        }
        built.push(std::fs::canonicalize(&file).unwrap_or(file));
    }
    if cfg.declaration && cfg.emit_js && !built.is_empty() {
        let mut ws = efs_check::Workspace::discover(&root);
        ws.codegen = cfg.codegen();
        let mut session = efs_check::Session::new(ws);
        for (file, dts) in session.declarations() {
            if !built.contains(&file) {
                continue;
            }
            let target = target_for(&file, &root, cfg, "d.ts");
            let text = efs_check::generated_header(&file_name(&file)) + &dts;
            if write_output(&file, &target, &text, r) {
                written += 1;
            }
        }
    }
    written
}

fn relative(from_dir: &Path, to: &Path) -> String {
    let from = std::fs::canonicalize(from_dir).unwrap_or(from_dir.to_path_buf());
    let to = std::fs::canonicalize(to).unwrap_or(to.to_path_buf());
    let fc: Vec<_> = from.components().collect();
    let tc: Vec<_> = to.components().collect();
    let common = fc.iter().zip(&tc).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); fc.len() - common];
    parts.extend(tc[common..].iter().map(|c| c.as_os_str().to_string_lossy().to_string()));
    parts.join("/")
}

fn build_watch(paths: &[PathBuf], cfg: &config::Config, r: &mut Reporter) -> ExitCode {
    let stamp = |files: &[PathBuf]| -> Vec<(PathBuf, Option<std::time::SystemTime>)> {
        files.iter().map(|f| (f.clone(), std::fs::metadata(f).and_then(|m| m.modified()).ok())).collect()
    };
    let mut last = Vec::new();
    loop {
        let files = collect(paths);
        let now = stamp(&files);
        if now != last {
            let n = build(paths, cfg, r);
            r.summary(&format!("built {n} file{}; watching for changes", if n == 1 { "" } else { "s" }));
            r.reset();
            last = now;
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}
