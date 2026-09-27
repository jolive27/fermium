//! `fermium`: the command-line tool (spec §B4, fermium-cli), with the subcommands, messages, help texts and exit
//! codes of Fermium 1.5's `fermium/cli.py`: run, check, fmt, build, doctor, jupyter, repl, lsp (and `parse`,
//! a developer tool). `fermium` alone starts the REPL; `fermium prog.fm` runs the program.
use std::process::ExitCode;

mod check;
mod doctor;
mod aot;
mod run;

use fermium_syntax::{sexpr, Diagnostic, Diagnostics};

const COMMANDS: &str = "{run,check,fmt,build,doctor,jupyter,repl,lsp}";

fn top_usage() -> String {
    format!("usage: fermium [-h] [--version]\n               {COMMANDS} ...")
}

fn help() -> String {
    format!("{}\n\nFermium: a programming language for physicists.\n\npositional arguments:\n  {COMMANDS}\n\
    \x20   run                 run a .fm program\n\
    \x20   check               check a program's units without running it\n\
    \x20   fmt                 convert a program between ASCII and symbols\n\
    \x20   build               compile a program into a standalone executable\n\
    \x20   doctor              check that Fermium is installed correctly\n\
    \x20   jupyter             set up the Jupyter kernel: fermium jupyter install\n\
    \x20   repl                start the interactive prompt (same as plain 'fermium')\n\
    \x20   lsp                 run the language server (used by editors, speaks LSP\n\
    \x20                       on stdin/stdout)\n\n\
    options:\n  -h, --help            show this help message and exit\n\
    \x20 --version             show program's version number and exit", top_usage())
}

/// argparse's error: the usage line, then `prog: error: msg`, exit code 2.
fn arg_error(usage: &str, prog: &str, msg: &str) -> ExitCode {
    eprintln!("{usage}\n{prog}: error: {msg}");
    ExitCode::from(2)
}

/// A subcommand's help (argparse's layout).
struct Sub {
    usage: &'static str,
    help: &'static str,
}

const RUN: Sub = Sub {
    usage: "usage: fermium run [-h] [--time] [--emit-llvm] [--interp] [--backend {auto,llvm,interp}]\n                   \
            [--base-dir DIR] file",
    help: "\npositional arguments:\n  file\n\noptions:\n  -h, --help            show this help message and exit\n  \
           --time                show how long each stage took\n  \
           --emit-llvm           print the generated LLVM IR instead of running\n  \
           --interp              run with the reference interpreter instead of compiling\n                        \
           (for checking; same as --backend interp)\n  \
           --backend {auto,llvm,interp}\n                        \
           the back end (default: auto, or $FERMIUM_BACKEND)\n  \
           --base-dir DIR        the folder that imports and data files are read from\n                        \
           (default: the program's folder)",
};
const CHECK: Sub = Sub {
    usage: "usage: fermium check [-h] file",
    help: "\npositional arguments:\n  file\n\noptions:\n  -h, --help  show this help message and exit",
};
const FMT: Sub = Sub {
    usage: "usage: fermium fmt [-h] [--pretty | --ascii] [--fix] [-w] file",
    help: "\npositional arguments:\n  file\n\noptions:\n  -h, --help   show this help message and exit\n  \
           --pretty     ASCII -> symbols (pi -> π, sqrt -> √, ^2 -> ²)\n  --ascii      symbols -> ASCII\n  \
           --fix        rewrite unit/variable collisions as bracketed units: 0.1 m ->\n               \
           0.1 [m] (keeps the old meaning)\n  -w, --write  rewrite the file instead of printing",
};
const BUILD: Sub = Sub {
    usage: "usage: fermium build [-h] [-o OUTPUT] file",
    help: "\npositional arguments:\n  file\n\noptions:\n  -h, --help            show this help message and exit\n  \
           -o OUTPUT, --output OUTPUT\n                        name of the executable (default: the program's name)",
};
const DOCTOR: Sub = Sub {
    usage: "usage: fermium doctor [-h]",
    help: "\noptions:\n  -h, --help  show this help message and exit",
};
const JUPYTER: Sub = Sub {
    usage: "usage: fermium jupyter [-h] [--sys-prefix] {install}",
    help: "\npositional arguments:\n  {install}\n\noptions:\n  -h, --help    show this help message and exit\n  \
           --sys-prefix  install into this Python environment, not for the user",
};
const REPL: Sub = Sub { usage: "usage: fermium repl [-h]", help: "\noptions:\n  -h, --help  show this help message and exit" };
const LSP: Sub = Sub { usage: "usage: fermium lsp [-h]", help: "\noptions:\n  -h, --help  show this help message and exit" };

impl Sub {
    fn prog(&self) -> String {
        self.usage.trim_start_matches("usage: ").split(" [").next().unwrap_or("fermium").to_string()
    }
    /// -h/--help anywhere in the arguments: print the help (exit 0).
    fn wants_help(&self, args: &[String]) -> bool {
        if args.iter().any(|a| a == "-h" || a == "--help") {
            { use std::io::Write; let _ = writeln!(std::io::stdout(), "{}\n{}", self.usage, self.help); }
            return true;
        }
        false
    }
    fn missing(&self, what: &str) -> ExitCode {
        arg_error(self.usage, &self.prog(), &format!("the following arguments are required: {what}"))
    }
}

/// Unrecognized arguments are reported with the top-level usage, as argparse does.
fn unrecognized(args: &[&String]) -> ExitCode {
    let s: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    arg_error(&top_usage(), "fermium", &format!("unrecognized arguments: {}", s.join(" ")))
}

/// Positional FILE and no options (check).
fn one_file<'a>(sub: &Sub, args: &'a [String]) -> Result<&'a str, ExitCode> {
    let (opts, pos): (Vec<&String>, Vec<&String>) = args.iter().partition(|a| a.starts_with('-') && a.len() > 1);
    if pos.is_empty() {
        return Err(sub.missing("file"));
    }
    let extra: Vec<&String> = opts.into_iter().chain(pos[1..].iter().copied()).collect();
    if !extra.is_empty() {
        return Err(unrecognized(&extra));
    }
    Ok(pos[0].as_str())
}

fn no_args(sub: &Sub, args: &[String]) -> Result<(), ExitCode> {
    if sub.wants_help(args) {
        return Err(ExitCode::SUCCESS);
    }
    if !args.is_empty() {
        return Err(unrecognized(&args.iter().collect::<Vec<_>>()));
    }
    Ok(())
}

fn cmd_run(args: &[String]) -> ExitCode {
    if RUN.wants_help(args) {
        return ExitCode::SUCCESS;
    }
    let mut o = run::RunOptions { backend: run::BackendChoice::from_env(), ..Default::default() };
    let mut file = None;
    let mut extra = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--time" => o.time = true,
            "--emit-llvm" => o.emit_llvm = true,
            "--interp" => o.backend = run::BackendChoice::Interp,
            "--base-dir" => match it.next() {
                Some(d) => o.base_dir = Some(d.clone()),
                None => return arg_error(RUN.usage, "fermium run", "argument --base-dir: expected one argument"),
            },
            "--backend" => match it.next().map(|s| (s, run::BackendChoice::parse(s))) {
                Some((_, Some(b))) => o.backend = b,
                Some((s, None)) => {
                    return arg_error(RUN.usage, "fermium run",
                                     &format!("argument --backend: invalid choice: '{s}' (choose from 'auto', \
                                               'llvm', 'interp')"))
                }
                None => return arg_error(RUN.usage, "fermium run", "argument --backend: expected one argument"),
            },
            s if s.starts_with('-') && s.len() > 1 => extra.push(a),
            _ if file.is_none() => file = Some(a.clone()),
            _ => extra.push(a),
        }
    }
    let Some(file) = file else { return RUN.missing("file") };
    if !extra.is_empty() {
        return unrecognized(&extra);
    }
    run::run_file(&file, o)
}

fn cmd_build(args: &[String]) -> ExitCode {
    if BUILD.wants_help(args) {
        return ExitCode::SUCCESS;
    }
    let mut output: Option<String> = None;
    let pos: Vec<&String> = {
        let mut v = vec![];
        let mut it = args.iter();
        while let Some(a) = it.next() {
            if a == "-o" || a == "--output" {
                output = it.next().cloned();
            } else if let Some(o) = a.strip_prefix("--output=") {
                output = Some(o.to_string());
            } else if !a.starts_with('-') {
                v.push(a);
            }
        }
        v
    };
    if pos.is_empty() {
        return BUILD.missing("file");
    }
    aot::build_file(pos[0], output.as_deref())
}

fn cmd_jupyter(args: &[String]) -> ExitCode {
    if JUPYTER.wants_help(args) {
        return ExitCode::SUCCESS;
    }
    // `fermium jupyter kernel -f FILE` is what kernel.json asks Jupyter to start (not in the help)
    if args.first().map(String::as_str) == Some("kernel") {
        let file = args.iter().position(|a| a == "-f").and_then(|i| args.get(i + 1));
        let Some(file) = file else {
            return arg_error("usage: fermium jupyter kernel -f CONNECTION_FILE", "fermium jupyter",
                             "the following arguments are required: -f");
        };
        return ExitCode::from(fermium_jupyter::kernel_command(file) as u8);
    }
    let mut sys_prefix = false;
    let mut prefix = None;
    let (mut pos, mut extra) = (vec![], vec![]);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sys-prefix" => sys_prefix = true,
            // --prefix DIR: install into DIR/share/jupyter (as v1's install(prefix=...); used by the tests)
            "--prefix" => match it.next() {
                Some(d) => prefix = Some(d.clone()),
                None => return arg_error(JUPYTER.usage, "fermium jupyter", "argument --prefix: expected one argument"),
            },
            s if s.starts_with('-') => extra.push(a),
            _ => pos.push(a),
        }
    }
    match pos.first().map(|s| s.as_str()) {
        None => return JUPYTER.missing("action"),
        Some("install") => {}
        Some(other) => {
            return arg_error(JUPYTER.usage, "fermium jupyter",
                             &format!("argument action: invalid choice: '{other}' (choose from 'install')"))
        }
    }
    extra.extend(pos[1..].iter().copied());
    if !extra.is_empty() {
        return unrecognized(&extra);
    }
    ExitCode::from(fermium_jupyter::install_command(sys_prefix, prefix.as_deref()) as u8)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a.ends_with(".fm")) {
        args.insert(0, "run".into());
    }
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        None => ExitCode::from(repl_main() as u8),
        Some("-h" | "--help") => {
            { use std::io::Write; let _ = writeln!(std::io::stdout(), "{}", help()); } // no panic when piped into head
            ExitCode::SUCCESS
        }
        Some("--version") => {
            println!("fermium {} (Rust)", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("run") => cmd_run(rest),
        Some("check") => {
            if CHECK.wants_help(rest) {
                return ExitCode::SUCCESS;
            }
            match one_file(&CHECK, rest) {
                Ok(f) => check::cmd_check(f),
                Err(c) => c,
            }
        }
        Some("fmt") => {
            if FMT.wants_help(rest) {
                return ExitCode::SUCCESS;
            }
            if !rest.iter().any(|a| !a.starts_with('-') || a == "-") {
                return arg_error("usage: fermium fmt [-h] [--pretty | --ascii] [--fix] [-w] file", "fermium fmt",
                                 "the following arguments are required: file");
            }
            ExitCode::from(fermium_fmt::cli::cmd_fmt(rest))
        }
        Some("build") => cmd_build(rest),
        Some("doctor") => match no_args(&DOCTOR, rest) {
            Ok(()) => doctor::doctor(),
            Err(c) => c,
        },
        Some("jupyter") => cmd_jupyter(rest),
        Some("repl") => match no_args(&REPL, rest) {
            Ok(()) => ExitCode::from(repl_main() as u8),
            Err(c) => c,
        },
        Some("lsp") => match no_args(&LSP, rest) {
            Ok(()) => ExitCode::from(fermium_lsp::serve() as u8),
            Err(c) => c,
        },
        Some("parse") => cmd_parse(rest),
        Some(a) if a.starts_with('-') => unrecognized(&[&args[0]]),
        Some(a) => arg_error(&top_usage(), "fermium",
                             &format!("argument cmd: invalid choice: '{a}' (choose from 'run', 'check', 'fmt', \
                                       'build', 'doctor', 'jupyter', 'repl', 'lsp')")),
    }
}

/// The REPL runs on a thread with a big stack, like `fermium run` (deep recursion gets a clear error).
fn repl_main() -> i32 {
    fermium_codegen::eval::STACK_LIMIT.store(400 << 20, std::sync::atomic::Ordering::Relaxed);
    std::thread::Builder::new()
        .stack_size(run::STACK)
        .spawn(fermium_repl::main)
        .ok()
        .and_then(|h| h.join().ok())
        .unwrap_or(101)
}

fn read(file: &str) -> Option<String> {
    fermium_fmt::cli::read_program(file).ok()
}

fn basename(file: &str) -> &str {
    std::path::Path::new(file).file_name().and_then(|s| s.to_str()).unwrap_or(file)
}

fn parse_usage() -> ExitCode {
    eprintln!("usage: fermium parse [--oracle|--tokens|--fix] FILE.fm");
    ExitCode::from(2)
}

/// `fermium parse FILE` (a developer tool): the tree as S-expressions; a parse error is printed as
/// `fermium check` prints it. `--oracle`: the full comparison format of rust/tools/parse_oracle.py (tree,
/// tokens, warnings). `--tokens`: only the lexer. `--fix`: fix mode (`fmt --fix`), the edits.
fn cmd_parse(args: &[String]) -> ExitCode {
    let mode = args.iter().find(|a| a.starts_with("--")).cloned().unwrap_or_default();
    let Some(file) = args.iter().find(|a| !a.starts_with("--")) else { return parse_usage() };
    let Some(src) = read(file) else { return ExitCode::from(2) };
    match mode.as_str() {
        "--tokens" => {
            let mut d = Diagnostics::new();
            let mut out = vec![];
            match fermium_syntax::tokenize(&src, &mut d) {
                Ok(toks) => {
                    out.push("TOKENS".to_string());
                    out.extend(toks.iter().map(sexpr::token));
                }
                Err(e) => out.push(sexpr::error(&Diagnostic { fix: vec![], ..e })),
            }
            out.push("WARNINGS".into());
            out.extend(d.warnings.iter().map(sexpr::warning));
            println!("{}", out.join("\n"));
            ExitCode::SUCCESS
        }
        "--oracle" => {
            print!("{}", fermium_syntax::oracle_text(&src));
            ExitCode::SUCCESS
        }
        "--fix" => {
            print!("{}", fermium_syntax::fix_text(&src));
            ExitCode::SUCCESS
        }
        "" => match fermium_syntax::parse(&src, &[]) {
            Ok((prog, diags)) => {
                println!("{}", sexpr::program(&prog));
                let mut ws: Vec<_> = diags.warnings.iter().collect();
                ws.sort_by_key(|w| (w.line.unwrap_or(0), w.col.unwrap_or(0)));
                for w in ws {
                    eprintln!("{}", w.format(Some(&src), None));
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{}", e.format(Some(&src), Some(basename(file))));
                ExitCode::from(1)
            }
        },
        _ => parse_usage(),
    }
}
