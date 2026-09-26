//! `fermium fmt FILE [--pretty|--ascii] [--fix] [-w|--write]`: `cmd_fmt` of `fermium/cli.py`.

use std::io::Write;

use fermium_syntax::Diagnostics;

use crate::{fix_source_report, format_source};

/// Read a program as Python's `open(path, encoding="utf-8").read()` does (universal newlines), with `_read`'s
/// messages. Err(exit code) after printing the message.
pub fn read_program(path: &str) -> Result<String, u8> {
    let p = std::path::Path::new(path);
    if p.is_dir() {
        eprintln!("'{path}' is a folder, not a program file\n  hint: give the path of a .fm file inside it");
        return Err(2);
    }
    match std::fs::read(p) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(s.replace("\r\n", "\n").replace('\r', "\n")),
            Err(_) => {
                eprintln!("'{path}' isn't a text file Fermium can read (it must be saved as UTF-8 text)\n  \
                           hint: in your editor use 'Save as' with the UTF-8 encoding");
                Err(2)
            }
        },
        Err(_) => {
            eprintln!("can't find the file '{path}'\n  hint: check the name, and that you're in the right folder \
                       (the command 'ls' lists the files here)");
            Err(2)
        }
    }
}

fn usage() -> u8 {
    eprintln!("usage: fermium fmt [-h] [--pretty | --ascii] [--fix] [-w] file");
    2
}

/// Returns the exit code.
pub fn cmd_fmt(args: &[String]) -> u8 {
    let (mut pretty, mut ascii, mut fix, mut write) = (false, false, false, false);
    let mut file = None;
    for a in args {
        match a.as_str() {
            "--pretty" => pretty = true,
            "--ascii" => ascii = true,
            "--fix" => fix = true,
            "-w" | "--write" => write = true,
            s if s.starts_with('-') && s.len() > 1 => return usage(),
            s => {
                if file.is_some() {
                    return usage();
                }
                file = Some(s.to_string());
            }
        }
    }
    if pretty && ascii {
        eprintln!("usage: fermium fmt [-h] [--pretty | --ascii] [--fix] [-w] file\n\
                   fermium fmt: error: argument --ascii: not allowed with argument --pretty");
        return 2;
    }
    let Some(file) = file else { return usage() };
    let src = match read_program(&file) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let name = std::path::Path::new(&file).file_name().and_then(|s| s.to_str()).unwrap_or(&file).to_string();
    let mut d = Diagnostics::new();
    let mut left = None;
    let result: Result<(String, String), fermium_syntax::Diagnostic> = (|| {
        if fix {
            let (mut out, n, l) = fix_source_report(&src, 20)?;
            left = l;
            let what = format!("fixed {n} unit/variable collision{}", if n != 1 { "s" } else { "" });
            if pretty || ascii {
                out = format_source(&out, if ascii { "ascii" } else { "pretty" }, &mut d)?;
            }
            Ok((out, what))
        } else {
            let mode = if ascii { "ascii" } else { "pretty" };
            let out = format_source(&src, mode, &mut d)?;
            Ok((out, format!("formatted ({mode})")))
        }
    })();
    let (out, what) = match result {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{}", e.format(Some(&src), Some(&name)));
            return 1;
        }
    };
    let mut err = std::io::stderr();
    for w in &d.warnings {
        let _ = writeln!(err, "{}", w.format(Some(&src), None));
    }
    if let Some(l) = &left {
        let _ = writeln!(err, "left for you to decide (Fermium 1 stopped here too):\n{}", l.format(Some(&out), Some(&name)));
    }
    if write {
        if std::fs::write(&file, &out).is_err() {
            eprintln!("can't write the file '{file}'");
            return 1;
        }
        println!("rewrote {file}: {what}");
    } else {
        let mut so = std::io::stdout();
        let _ = so.write_all(out.as_bytes());
        let _ = so.flush();
        let head = what.split(" (").next().unwrap_or(&what);
        let _ = writeln!(err, "{head} {name} (not run — use fermium run)");
    }
    0
}
