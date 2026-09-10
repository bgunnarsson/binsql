//! One pass over an argument list.
//!
//! Small on purpose. Command mode has three verbs and a dozen flags between
//! them, which is under the weight of a parser dependency and over the weight
//! of reading the vector by hand in three places.

use std::collections::HashMap;

use super::{Failure, usage};

pub struct Args {
    values: HashMap<String, Vec<String>>,
    switches: Vec<String>,
    positional: Vec<String>,
}

impl Args {
    /// Splits `args` into flags and positionals. `takes_value` names the flags
    /// that swallow the argument after them; everything else beginning with `-`
    /// is a switch, and an unknown flag is refused rather than ignored.
    ///
    /// Accepts `--flag value`, `--flag=value` and, for the single-letter
    /// spellings, `-f value`. `--` ends the flags, so a statement starting with
    /// a dash can still be passed.
    pub fn parse(
        args: Vec<String>,
        takes_value: &[&str],
        switches: &[&str],
    ) -> Result<Args, Failure> {
        let mut parsed = Args {
            values: HashMap::new(),
            switches: Vec::new(),
            positional: Vec::new(),
        };

        let mut rest = args.into_iter();
        let mut only_positional = false;

        while let Some(arg) = rest.next() {
            if only_positional || !arg.starts_with('-') || arg == "-" {
                parsed.positional.push(arg);
                continue;
            }
            if arg == "--" {
                only_positional = true;
                continue;
            }

            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (arg.clone(), None),
            };
            let name = name.trim_start_matches('-').to_string();

            if takes_value.contains(&name.as_str()) {
                let value = match inline {
                    Some(value) => value,
                    None => rest
                        .next()
                        .ok_or_else(|| usage(format!("--{name} needs a value")))?,
                };
                parsed.values.entry(name).or_default().push(value);
                continue;
            }

            if switches.contains(&name.as_str()) {
                if inline.is_some() {
                    return Err(usage(format!("--{name} does not take a value")));
                }
                parsed.switches.push(name);
                continue;
            }

            return Err(usage(format!("unknown option {arg}")));
        }

        Ok(parsed)
    }

    /// The first of several spellings of one flag that was given — `-o` and
    /// `--format` are the same flag, and either may be the one used.
    pub fn value(&self, names: &[&str]) -> Option<&str> {
        names
            .iter()
            .find_map(|name| self.values.get(*name))
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    /// Every value a repeatable flag was given, in the order given.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.values
            .get(name)
            .map(|values| values.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn is_set(&self, names: &[&str]) -> bool {
        names
            .iter()
            .any(|name| self.switches.iter().any(|switch| switch == name))
    }

    pub fn positional(&self) -> &[String] {
        &self.positional
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const VALUES: &[&str] = &["format", "o", "conn", "c"];
    const SWITCHES: &[&str] = &["pretty", "dry-run"];

    #[test]
    fn reads_both_spellings_of_a_flag() {
        let parsed = Args::parse(args(&["-o", "json", "select 1"]), VALUES, SWITCHES).unwrap();
        assert_eq!(parsed.value(&["format", "o"]), Some("json"));
        assert_eq!(parsed.positional(), ["select 1"]);

        let parsed = Args::parse(args(&["--format=csv"]), VALUES, SWITCHES).unwrap();
        assert_eq!(parsed.value(&["format", "o"]), Some("csv"));
    }

    #[test]
    fn reads_switches() {
        let parsed = Args::parse(args(&["--pretty", "--dry-run"]), VALUES, SWITCHES).unwrap();
        assert!(parsed.is_set(&["pretty"]));
        assert!(parsed.is_set(&["dry-run"]));
        assert!(!parsed.is_set(&["tx"]));
    }

    #[test]
    fn a_repeated_flag_keeps_every_value_in_order() {
        let parsed = Args::parse(
            args(&["--conn", "a", "--conn=b", "--conn", "c"]),
            VALUES,
            SWITCHES,
        )
        .unwrap();
        assert_eq!(parsed.values("conn"), ["a", "b", "c"]);
        assert_eq!(parsed.value(&["conn"]), Some("c"));
        assert!(parsed.values("format").is_empty());
    }

    #[test]
    fn a_double_dash_ends_the_flags() {
        let parsed = Args::parse(args(&["--", "--not-a-flag"]), VALUES, SWITCHES).unwrap();
        assert_eq!(parsed.positional(), ["--not-a-flag"]);
    }

    #[test]
    fn a_bare_dash_is_a_positional() {
        let parsed = Args::parse(args(&["-"]), VALUES, SWITCHES).unwrap();
        assert_eq!(parsed.positional(), ["-"]);
    }

    #[test]
    fn refuses_what_it_does_not_know() {
        assert!(Args::parse(args(&["--nope"]), VALUES, SWITCHES).is_err());
        assert!(Args::parse(args(&["--format"]), VALUES, SWITCHES).is_err());
        assert!(Args::parse(args(&["--pretty=yes"]), VALUES, SWITCHES).is_err());
    }
}
