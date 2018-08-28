use config;
use std::path::Path;

pub fn read_config(maybe_filename: &Option<&str>) -> Result<config::Config, String> {
    let mut settings = config::Config::new();

    settings
        .merge(config::File::with_name("conf/defaults.yml"))
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
