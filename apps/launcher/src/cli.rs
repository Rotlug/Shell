pub const USAGE: &str = "\
gms-launcher: pick one of the entries it is given.

Usage: gms-launcher [--prompt TEXT] < entries

The entries are read from stdin, one per line: `title`, or `icon<TAB>title`. The icon is
the name of an icon from the icon theme, or the path of an image, or nothing. More tab
separated fields can follow, they are not shown.

The line of the entry that was picked is printed, as it was given. The exit status is
0 when something was picked, and 1 when it was closed without picking anything.

Options:
  -p, --prompt TEXT   What the search box says while nothing is typed in it
  -h, --help          This";

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub prompt: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Options),
    Help,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut args = args.into_iter();
    let mut options = Options { prompt: "Search".to_owned() };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-p" | "--prompt" => {
                options.prompt = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
            }
            _ => match arg.strip_prefix("--prompt=") {
                Some(prompt) => options.prompt = prompt.to_owned(),
                None => return Err(format!("Unknown option: {arg}")),
            },
        }
    }

    Ok(Parsed::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Parsed, String> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn defaults() {
        assert_eq!(parse_args(&[]), Ok(Parsed::Run(Options { prompt: "Search".into() })));
    }

    #[test]
    fn prompts() {
        let expected = Ok(Parsed::Run(Options { prompt: "Launch".into() }));

        assert_eq!(parse_args(&["--prompt", "Launch"]), expected);
        assert_eq!(parse_args(&["-p", "Launch"]), expected);
        assert_eq!(parse_args(&["--prompt=Launch"]), expected);
    }

    #[test]
    fn mistakes() {
        assert!(parse_args(&["--prompt"]).is_err());
        assert!(parse_args(&["--nope"]).is_err());
        assert_eq!(parse_args(&["-h"]), Ok(Parsed::Help));
    }
}
