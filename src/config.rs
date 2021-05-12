use anyhow::{Context, Error};
use std::env::VarError;
use std::str::FromStr;

const ENVIRONMENT_VARIABLE_PREFIX: &str = "MONITORBOT_";

#[derive(Clone, Debug)]
pub struct Config {
    // authorization secret (token) to be able to scrape the metrics endpoint
    pub secret: String,
    // http server port to bind to
    pub port: u16,
    // github api tokens to collect rate limit statistics
    pub gh_rate_limit_tokens: String,
    // github rate limit stats data cache refresh rate frequency (in seconds)
    pub gh_rate_limit_stats_cache_refresh: u64,
    // github api token to be used when querying for gha runner's status
    // note: token must have (repo scope) authorization
    pub github_token: String,
    // gh runner's repos to track they status. multiple repos are allowed
    // ex. "rust,cargo,docs.rs"
    pub gha_runners_repos: String,
    // gha runner's status refresh rate frequency (in seconds)
    pub gha_runners_cache_refresh: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, Error> {
        Ok(Self {
            secret: require_env("SECRET")?,
            port: default_env("PORT", 3001)?,
            gh_rate_limit_tokens: require_env("RATE_LIMIT_TOKENS")?,
            gh_rate_limit_stats_cache_refresh: default_env("GH_RATE_LIMIT_STATS_REFRESH", 120)?,
            github_token: require_env("GITHUB_TOKEN")?,
            gha_runners_repos: require_env("RUNNERS_REPOS")?,
            gha_runners_cache_refresh: default_env("GHA_RUNNERS_REFRESH", 120)?,
        })
    }
}

fn maybe_env<T>(name: &str) -> Result<Option<T>, Error>
where
    T: FromStr,
    Error: From<T::Err>,
{
    match std::env::var(format!("{}{}", ENVIRONMENT_VARIABLE_PREFIX, name)) {
        Ok(val) => Ok(Some(val.parse().map_err(Error::from).context(format!(
            "the {} environment variable has invalid content",
            name
        ))?)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => {
            anyhow::bail!("environment variable {} is not unicode!", name)
        }
    }
}

fn require_env<T>(name: &str) -> Result<T, Error>
where
    T: FromStr,
    Error: From<T::Err>,
{
    match maybe_env::<T>(name)? {
        Some(res) => Ok(res),
        None => anyhow::bail!(
            "missing environment variable {}",
            format!("{}{}", ENVIRONMENT_VARIABLE_PREFIX, name)
        ),
    }
}

fn default_env<T>(name: &str, default: T) -> Result<T, Error>
where
    T: FromStr,
    Error: From<T::Err>,
{
    Ok(maybe_env::<T>(name)?.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    // note: if you add new unit tests here and need to set up an env var
    // you need to use a unique env var name for your test. cargo by default will run
    // your tests in parallel using threads and one test setup may interfere with
    // another test's outcome if they both share the same env var name.
    use super::ENVIRONMENT_VARIABLE_PREFIX;
    use super::{default_env, maybe_env, require_env};

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
            Err(_) => panic!("return value as Err, we expected str"), // we failed
        };

        assert_eq!(expected, result);
    }

    #[test]
    fn config_require_string_not_present() {
        let env_var = format!("{}TOKENS_NOT_PRESENT", ENVIRONMENT_VARIABLE_PREFIX);
        if let Err(e) = require_env::<String>("TOKENS_NOT_PRESENT") {
            let expected = anyhow::anyhow!("missing environment variable {}", env_var);
            assert_eq!(e.to_string(), expected.to_string());
            return;
        }

        panic!("expected an Err");
    }
}
