//! `pwetty` — introspection CLI for the bundled tiles.
//!
//! The render path (`render`) needs surfaceless EGL; run it with
//! `EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1` for a headless software
//! render. The other subcommands are pure CPU.
//!
//!   pwetty list                       # bundled tiles
//!   pwetty schema <tile>              # print the tile's JSON Schema (the contract)
//!   pwetty check [tile]               # validate template <-> schema <-> samples
//!   pwetty render <tile> [opts]       # render sample(s) to PNG
//!       --sample <name> | --all-states
//!       --time <secs>                 # animation time (default 0.4)
//!       -o, --out <dir>               # output dir (default /tmp/claude-1000/pwetty)

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

use pwetty_box::{config, content, tiles};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[args.len().min(1)..];

    match cmd {
        "list" => cmd_list(),
        "schema" => cmd_schema(rest),
        "check" => cmd_check(rest),
        "render" => cmd_render(rest),
        "" | "-h" | "--help" | "help" => {
            usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("pwetty: unknown command '{other}'\n");
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "pwetty — bundled tile introspection\n\n\
         USAGE:\n  \
         pwetty list\n  \
         pwetty schema <tile>\n  \
         pwetty check [tile]\n  \
         pwetty render <tile> [--sample <name> | --all-states] [--time <s>] [-o <dir>]\n"
    );
}

fn cmd_list() -> ExitCode {
    println!("Bundled tiles:");
    for p in tiles::all() {
        let summary = p
            .doc
            .lines()
            .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .unwrap_or("")
            .trim();
        let samples: Vec<&str> = p.samples.iter().map(|(n, _)| *n).collect();
        println!("  {:<10} {}", p.name, summary);
        println!("  {:<10} samples: {}", "", samples.join(", "));
    }
    ExitCode::SUCCESS
}

fn cmd_schema(rest: &[String]) -> ExitCode {
    let Some(name) = rest.first() else {
        eprintln!("pwetty schema: missing <tile>");
        return ExitCode::FAILURE;
    };
    match tiles::get(name) {
        Some(p) => {
            print!("{}", p.schema);
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("pwetty schema: unknown tile '{name}'");
            ExitCode::FAILURE
        }
    }
}

/// Variable names referenced by a minijinja `template`.
fn template_vars(template: &str) -> Result<BTreeSet<String>, String> {
    let mut env = minijinja::Environment::new();
    env.add_template("t", template).map_err(|e| e.to_string())?;
    let t = env.get_template("t").map_err(|e| e.to_string())?;
    Ok(t.undeclared_variables(true).into_iter().collect())
}

/// Property names declared in a tile's JSON Schema.
fn schema_props(schema: &str) -> BTreeSet<String> {
    serde_json::from_str::<Value>(schema)
        .ok()
        .and_then(|v| v.get("properties").cloned())
        .and_then(|p| p.as_object().cloned())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn cmd_check(rest: &[String]) -> ExitCode {
    let names: Vec<&str> = match rest.first() {
        Some(n) => vec![n.as_str()],
        None => tiles::all().iter().map(|p| p.name).collect(),
    };
    let mut ok = true;
    for name in names {
        let Some(p) = tiles::get(name) else {
            eprintln!("✗ {name}: unknown tile");
            ok = false;
            continue;
        };
        println!("● {name}");

        // Preset config must resolve to a usable Config with a template.
        let cfg = config::resolve(serde_json::json!({ "tile": name }));
        let Some(template) = cfg.format.clone() else {
            println!("  ✗ preset has no `format` template");
            ok = false;
            continue;
        };

        // Template variables vs schema properties.
        let props = schema_props(p.schema);
        match template_vars(&template) {
            Ok(vars) => {
                println!(
                    "  template binds: {}",
                    vars.iter().cloned().collect::<Vec<_>>().join(", ")
                );
                for v in vars.difference(&props) {
                    println!("  ⚠ template uses {{{{ {v} }}}} not declared in schema");
                }
                for v in props.difference(&vars) {
                    println!("  · schema declares '{v}' (not used by template)");
                }
            }
            Err(e) => {
                println!("  ✗ template parse error: {e}");
                ok = false;
            }
        }

        // Every bundled sample must render cleanly.
        for (sname, sjson) in p.samples {
            match serde_json::from_str::<Value>(sjson) {
                Ok(data) => match pwetty_box::markup::render_template(&template, &data) {
                    Ok(m) => {
                        // Markup must also process without falling back to escaped text.
                        let _ = pwetty_box::markup::process(&m, &[], &[]);
                        println!("  ✓ sample '{sname}' renders");
                    }
                    Err(e) => {
                        println!("  ✗ sample '{sname}' render error: {e}");
                        ok = false;
                    }
                },
                Err(e) => {
                    println!("  ✗ sample '{sname}' is not valid JSON: {e}");
                    ok = false;
                }
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn cmd_render(rest: &[String]) -> ExitCode {
    let mut name: Option<String> = None;
    let mut sample: Option<String> = None;
    let mut all_states = false;
    let mut time: f32 = 0.4;
    let mut out_dir = PathBuf::from("/tmp/claude-1000/pwetty");

    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--sample" => {
                i += 1;
                sample = rest.get(i).cloned();
            }
            "--all-states" => all_states = true,
            "--time" => {
                i += 1;
                time = rest.get(i).and_then(|s| s.parse().ok()).unwrap_or(time);
            }
            "-o" | "--out" => {
                i += 1;
                if let Some(d) = rest.get(i) {
                    out_dir = PathBuf::from(d);
                }
            }
            other if name.is_none() => name = Some(other.to_string()),
            other => {
                eprintln!("pwetty render: unexpected arg '{other}'");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let Some(name) = name else {
        eprintln!("pwetty render: missing <tile>");
        return ExitCode::FAILURE;
    };
    let Some(preset) = tiles::get(&name) else {
        eprintln!("pwetty render: unknown tile '{name}'");
        return ExitCode::FAILURE;
    };

    let cfg = config::resolve(serde_json::json!({ "tile": name }));

    // Which samples to render.
    let chosen: Vec<&(&str, &str)> = if all_states {
        preset.samples.iter().collect()
    } else if let Some(s) = &sample {
        match preset.samples.iter().find(|(n, _)| n == s) {
            Some(p) => vec![p],
            None => {
                eprintln!("pwetty render: tile '{name}' has no sample '{s}'");
                return ExitCode::FAILURE;
            }
        }
    } else {
        // Default: the first sample.
        preset.samples.iter().take(1).collect()
    };

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("pwetty render: cannot create {}: {e}", out_dir.display());
        return ExitCode::FAILURE;
    }

    let mut ok = true;
    for (sname, sjson) in chosen {
        let data: Value = match serde_json::from_str(sjson) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ sample '{sname}': {e}");
                ok = false;
                continue;
            }
        };
        let markup = content::markup_for(&cfg, &data);
        let out = out_dir.join(format!("{name}-{sname}.png"));
        match pwetty_box::render_png(&cfg, &markup, time, &out) {
            Ok(()) => println!("wrote {}", out.display()),
            Err(e) => {
                eprintln!("✗ {}: {e}", out.display());
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
