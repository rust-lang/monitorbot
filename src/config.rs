use std::env::VarError;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

const ENVIRONMENT_VARIABLE_PREFIX: &str = "MONITORBOT_";

#[derive(Clone, Debug)]
pub struct Config {
    // http server port to bind to
    pub port: u16,
    // github api tokens to collect rate limit statistics
    pub gh_rate_limit_tokens: String,
    // github rate limit stats data cache refresh rate frequency (in seconds)
    pub gh_rate_limit_stats_cache_refresh: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            port: default_env("PORT", 3001)?,
            gh_rate_limit_tokens: require_env("RATE_LIMIT_TOKENS")?,
            gh_rate_limit_stats_cache_refresh: default_env(
                "GH_RATE_LIMIT_STATS_REFRESH",
                120,
            )?,
        })
    }
}

fn maybe_env<T: FromStr>(name: &str) -> Result<Option<T>, ConfigError> {
    match std::env::var(format!("{}{}", ENVIRONMENT_VARIABLE_PREFIX, name)) {
        Ok(val) => match val.parse() {
            Ok(v) => Ok(Some(v)),
            _ => Err(ConfigError(format!(
                "the {} environment variable has invalid content",
                name
            ))),
        },
        Err(VarError::NotPresent) => Ok(None),
        Err(not_unicode) => Err(ConfigError(format!("the {} {}", name, not_unicode))),
    }
}

fn require_env<T: FromStr>(name: &str) -> Result<T, ConfigError> {
    match maybe_env::<T>(name)? {
        Some(res) => Ok(res),
        None => Err(ConfigError(format!(
            "missing environment variable {}{}",
            ENVIRONMENT_VARIABLE_PREFIX, name
        ))),
    }
}

fn default_env<T: FromStr>(name: &str, default: T) -> Result<T, ConfigError> {
    Ok(maybe_env::<T>(name)?.unwrap_or(default))
}

#[derive(Debug, PartialEq)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for ConfigError {}

#[cfg(test)]
mod tests {
    // note: if you add new unit tests here and need to set up an env var
    // you need to use a unique env var name for your test. cargo by default will run
    // your tests in parallel using threads and one test setup may interfere with
    // another test's outcome if they both share the same env var name.
    use super::{maybe_env, default_env, require_env};
    use super::ENVIRONMENT_VARIABLE_PREFIX;

    #[test]
    fn config_some_value_not_present() {
        let expected: Option<String> = None;
        let result = match maybe_env("NOT_EXISTENT") {
            Ok(r) => r,
            Err(_) => panic!("return value as Err, we expected Option"), // we failed
        };

        assert_eq!(expected, result);
    }

    #[test]
    fn config_some_value_string_present() {
        let expected: String = String::from("12345678");
        std::env::set_var(
            format!("{}TEST_VAR_STR", ENVIRONMENT_VARIABLE_PREFIX),
            &expected,
        );
        let result = match maybe_env("TEST_VAR_STR") {
            Ok(r) => r,
            Err(_) => panic!("return value as Err, we expected Option"), // we failed
        };

        assert_eq!(Some(expected), result);
    }

    #[test]
    fn config_some_value_u16_present() {
        let expected = 12345u16;
        std::env::set_var(
            format!("{}TEST_VAR", ENVIRONMENT_VARIABLE_PREFIX),
            expected.to_string(),
        );
        let result = match maybe_env("TEST_VAR") {
            Ok(r) => r,
            Err(_) => panic!("return value as Err, we expected Option"), // we failed
        };

        assert_eq!(Some(expected), result);
    }

    #[test]
    fn config_default_value_u16_not_present() {
        let expected = 3001u16;
        let result = match default_env("PORT_NOT_SET", expected) {
            Ok(r) => r,
            Err(_) => panic!("return value as Err, we expected Option"), // we failed
        };

        assert_eq!(expected, result);
    }

    #[test]
    fn config_default_value_u16_present() {
        let expected = 80u16;
        std::env::set_var(
            format!("{}PORT", ENVIRONMENT_VARIABLE_PREFIX),
            expected.to_string(),
        );

        let result = match default_env("PORT", 3001u16) {
            Ok(r) => r,
            Err(_) => panic!("return value as Err, we expected Option"), // we failed
        };

        assert_eq!(expected, result);
    }

    #[test]
    fn config_require_string_present() {
        let expected = "TOKENS,TOKENS,TOKENS".to_string();
        std::env::set_var(
            format!("{}RATE_LIMIT_TOKENS", ENVIRONMENT_VARIABLE_PREFIX),
            expected.to_string(),
        );

        let result = match require_env::<String>("RATE_LIMIT_TOKENS") {
            Ok(r) => r,
            Err(_) => panic!("return value as Err, we expected Option"), // we failed
        };

        assert_eq!(expected, result);
    }

    #[test]
    fn config_require_string_not_present() {
        use super::ConfigError;

        let env_var = format!("{}TOKENS_NOT_PRESENT", ENVIRONMENT_VARIABLE_PREFIX);
        match require_env::<String>("TOKENS_NOT_PRESENT") {
            Ok(r) => r,
            Err(e) => {
                assert_eq!(
                    ConfigError(format!("missing environment variable {}", env_var)),
                    e
                );
                return;
            }
        };

        // we failed if our code gets here
        panic!("return value as Err, we expected Option");
    }
}
