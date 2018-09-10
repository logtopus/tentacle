use config;
use std::path::Path;

pub fn read_config(maybe_filename: &Option<&str>) -> Result<config::Config, String> {
    let mut settings = config::Config::new();

    let defaults = include_bytes!("../conf/defaults.yml");
    settings
        .merge(config::File::from_str(String::from_utf8_lossy(defaults).as_ref(), config::FileFormat::Yaml))
        .unwrap();

    match maybe_filename {
        &Some(filename) => {
            if !Path::new(filename).exists() {
                return Err(format!("Configuration file {} does not exist", filename));
            } else {
                settings.merge(config::File::with_name(&filename)).unwrap()
            }
        }
        &None => &settings,
    };
    settings
        .merge(config::Environment::with_prefix("app"))
        .unwrap();

    Ok(settings)
}

#[cfg(test)]
mod tests {
    use cfg;

    #[test]
    fn test_read_config() {
        let settings = cfg::read_config(&Some("tests/testconf.yml")).unwrap();
        assert_eq!(12345, settings.get_int("http.bind.port").unwrap());
        assert_eq!("127.0.0.1", settings.get_str("http.bind.ip").unwrap());
    }
}
