use std::path::{Path, PathBuf};

pub const GROUPS: [&str; 4] = ["offline", "native", "live", "visual"];

pub struct Check {
    pub name: String,
    pub group: String,
}

pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

pub struct Options {
    pub group: Option<String>,
    pub only: Option<String>,
    pub max_fails: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            group: None,
            only: None,
            max_fails: usize::MAX,
        }
    }
}

pub fn parse_args(argv: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--group" => {
                i += 1;
                let g = argv.get(i).ok_or("falta valor para --group")?.clone();
                if !GROUPS.contains(&g.as_str()) {
                    return Err(format!("grupo invalido: {g}"));
                }
                opts.group = Some(g);
            }
            "--only" => {
                i += 1;
                opts.only = Some(argv.get(i).ok_or("falta valor para --only")?.clone());
            }
            "--max-fails" => {
                i += 1;
                let n: usize = argv
                    .get(i)
                    .ok_or("falta valor para --max-fails")?
                    .parse()
                    .map_err(|_| "max-fails debe ser un entero >= 1")?;
                if n < 1 {
                    return Err("max-fails debe ser un entero >= 1".to_string());
                }
                opts.max_fails = n;
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
        i += 1;
    }
    Ok(opts)
}

fn strip_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return line[..idx].trim_end(),
            _ => {}
        }
    }
    line
}

fn parse_quoted(value: &str) -> Option<String> {
    let v = value.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"'))
            || (v.starts_with('\'') && v.ends_with('\'')))
    {
        Some(v[1..v.len() - 1].to_string())
    } else {
        None
    }
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once('=')?;
    Some((k.trim(), v.trim()))
}

pub fn parse_registry(text: &str) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut name: Option<String> = None;
    let mut group: Option<String> = None;
    let mut flush = |name: &mut Option<String>, group: &mut Option<String>| {
        if let (Some(n), Some(g)) = (name.take(), group.take()) {
            checks.push(Check { name: n, group: g });
        }
    };
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            flush(&mut name, &mut group);
            if inner.trim() == "check" {
                name = None;
                group = None;
            }
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(&mut name, &mut group);
            if let Some(n) = inner.trim().strip_prefix("checks.") {
                name = Some(n.trim().to_string());
                group = None;
            }
            continue;
        }
        if let Some((k, v)) = split_key_value(line) {
            match k {
                "name" => {
                    if let Some(val) = parse_quoted(v) {
                        name = Some(val);
                    }
                }
                "group" => {
                    if let Some(val) = parse_quoted(v) {
                        group = Some(val);
                    }
                }
                _ => {}
            }
        }
    }
    flush(&mut name, &mut group);
    checks
}

pub fn select<'a>(checks: &'a [Check], opts: &Options) -> Vec<&'a Check> {
    checks
        .iter()
        .filter(|c| opts.group.as_ref().is_none_or(|g| &c.group == g))
        .filter(|c| opts.only.as_ref().is_none_or(|n| &c.name == n))
        .collect()
}

pub fn execute(
    selected: &[&Check],
    max_fails: usize,
    run_one: &dyn Fn(&Check) -> Outcome,
) -> (Vec<String>, i32) {
    if selected.is_empty() {
        return (vec!["0 checks".to_string()], 0);
    }
    let mut lines = Vec::new();
    let (mut pass, mut fail, mut skip) = (0, 0, 0);
    for check in selected {
        match run_one(check) {
            Outcome::Pass => {
                lines.push(format!("PASS {} {}", check.name, check.group));
                pass += 1;
            }
            Outcome::Fail => {
                lines.push(format!("FAIL {} {}", check.name, check.group));
                fail += 1;
            }
            Outcome::Skip => {
                lines.push(format!("SKIP {} {}", check.name, check.group));
                skip += 1;
            }
        }
        if fail >= max_fails {
            lines.push(format!("corte tras {fail} fallas (--max-fails)"));
            break;
        }
    }
    lines.push(format!("{pass} passed, {fail} failed, {skip} skipped"));
    (lines, i32::from(fail > 0))
}

pub fn run_cargo_test(check: &Check) -> Outcome {
    let target = format!("check_{}", check.name);
    match std::process::Command::new("cargo")
        .args(["test", "--test", &target])
        .output()
    {
        Ok(out) if out.status.success() => Outcome::Pass,
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("no test target matches") {
                Outcome::Skip
            } else {
                Outcome::Fail
            }
        }
        Err(_) => Outcome::Fail,
    }
}

pub fn find_registry() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let candidate = dir.join("checks.toml");
        if candidate.is_file() {
            return candidate;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return PathBuf::from("checks.toml"),
        }
    }
}

pub fn load_registry(path: &Path) -> Vec<Check> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_registry(&text),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
[[check]]
name = "queue"
group = "offline"

[[check]]
name = "player"
group = "native"

[[check]]
name = "stream"
group = "live"
"#;

    fn fixture() -> Vec<Check> {
        parse_registry(FIXTURE)
    }

    #[test]
    fn registro_vacio_informa_cero_checks() {
        for text in ["", "# solo un comentario\n", "[otra]\nclave = 1\n"] {
            let checks = parse_registry(text);
            assert!(checks.is_empty());
            let (lines, code) = execute(&[], usize::MAX, &|_| Outcome::Pass);
            assert_eq!(lines, vec!["0 checks".to_string()]);
            assert_eq!(code, 0);
        }
    }

    #[test]
    fn dummy_mixto_reporta_pass_fail_skip_y_sale_1() {
        let checks = fixture();
        let opts = Options::default();
        let selected = select(&checks, &opts);
        assert_eq!(selected.len(), 3);
        let run_one = |c: &Check| match c.name.as_str() {
            "queue" => Outcome::Pass,
            "player" => Outcome::Fail,
            _ => Outcome::Skip,
        };
        let (lines, code) = execute(&selected, opts.max_fails, &run_one);
        assert_eq!(
            lines,
            vec![
                "PASS queue offline".to_string(),
                "FAIL player native".to_string(),
                "SKIP stream live".to_string(),
                "1 passed, 1 failed, 1 skipped".to_string(),
            ]
        );
        assert_eq!(code, 1);
    }

    #[test]
    fn filtros_por_grupo_y_nombre() {
        let checks = fixture();
        let grouped = select(
            &checks,
            &Options {
                group: Some("offline".to_string()),
                ..Options::default()
            },
        );
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].name, "queue");
        let single = select(
            &checks,
            &Options {
                only: Some("stream".to_string()),
                ..Options::default()
            },
        );
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].group, "live");
    }

    #[test]
    fn max_fails_corta_tras_la_primera_falla() {
        let checks = fixture();
        let selected = select(&checks, &Options::default());
        let (lines, code) = execute(&selected, 1, &|c| {
            if c.name == "queue" {
                Outcome::Fail
            } else {
                Outcome::Pass
            }
        });
        assert_eq!(code, 1);
        assert!(lines.iter().any(|l| l.starts_with("FAIL queue")));
        assert!(!lines.iter().any(|l| l.starts_with("PASS player")));
    }
}
