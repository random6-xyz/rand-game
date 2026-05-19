#[derive(Debug, Default)]
pub(crate) struct Options {
    pub(crate) addr: Option<String>,
    pub(crate) player_id: Option<String>,
    pub(crate) map_id: Option<String>,
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) radius: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) once: bool,
}

pub(crate) fn parse_options(args: Vec<String>) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--addr" => options.addr = Some(required_value(&arg, iter.next())?),
            "--player-id" => options.player_id = Some(required_value(&arg, iter.next())?),
            "--map-id" => options.map_id = Some(required_value(&arg, iter.next())?),
            "--x" => options.x = Some(required_value(&arg, iter.next())?),
            "--y" => options.y = Some(required_value(&arg, iter.next())?),
            "--radius" => options.radius = Some(required_value(&arg, iter.next())?),
            "--path" => options.path = Some(required_value(&arg, iter.next())?),
            "--once" => options.once = true,
            other => return Err(format!("unknown option `{other}`").into()),
        }
    }

    Ok(options)
}

pub(crate) fn parse_i32_option(
    value: Option<&str>,
    default: &str,
    flag: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    value
        .unwrap_or(default)
        .parse::<i32>()
        .map_err(|err| format!("invalid {flag}: {err}").into())
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, Box<dyn std::error::Error>> {
    value.ok_or_else(|| format!("missing value for {flag}").into())
}
