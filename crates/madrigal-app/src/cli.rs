use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub smoke_quit_after_ms: Option<u64>,
    pub profile: Option<String>,
    pub files: Vec<String>,
}

pub fn parse<I>(args: I) -> Result<Options, String>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut opts = Options::default();
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke-quit-after-ms" => {
                let value = args
                    .next()
                    .ok_or("falta valor para --smoke-quit-after-ms")?;
                opts.smoke_quit_after_ms = Some(parse_ms(&value)?);
            }
            "--profile" => {
                let value = args.next().ok_or("falta valor para --profile")?;
                if !madrigal_paths::valid_profile_id(&value) {
                    return Err(format!("perfil inválido: {value:?}"));
                }
                opts.profile = Some(value);
            }
            other => {
                if let Some(value) = other.strip_prefix("--smoke-quit-after-ms=") {
                    opts.smoke_quit_after_ms = Some(parse_ms(value)?);
                } else if let Some(value) = other.strip_prefix("--profile=") {
                    if !madrigal_paths::valid_profile_id(value) {
                        return Err(format!("perfil inválido: {value:?}"));
                    }
                    opts.profile = Some(value.to_owned());
                } else if other.starts_with("--") {
                    return Err(format!("argumento desconocido: {other}"));
                } else {
                    opts.files.push(other.to_owned());
                }
            }
        }
    }
    Ok(opts)
}

fn parse_ms(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("milisegundos inválidos: {value:?}"))
}

pub fn profile_dir(opts: &Options) -> Result<PathBuf, String> {
    match &opts.profile {
        Some(name) => {
            let base = match std::env::var_os(madrigal_paths::PROFILE_ROOT_ENV) {
                Some(root) if !root.is_empty() => PathBuf::from(root),
                _ => madrigal_paths::data_dir(),
            };
            Ok(base.join("profiles").join(name))
        }
        None => Ok(PathBuf::new()),
    }
}
