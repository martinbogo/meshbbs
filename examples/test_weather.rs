use meshbbs::bbs::weather::WeatherService;
use meshbbs::config::Config;

#[tokio::main]
async fn main() {
    println!("Testing weather service...");

    // Try to load config from file
    let config_result = Config::load("config.toml").await;
    if config_result.is_err() {
        println!("Error: Could not load config.toml");
        println!("Please ensure config.toml exists with a valid weather.api_key");
        return;
    }

    let config = config_result.unwrap();
    if config.apps.weather.api_key.is_empty() {
        println!("Error: No API key found in config.toml");
        println!("Please add your OpenWeatherMap API key to the [weather] section");
        return;
    }

    let mut service = WeatherService::new(config.apps.weather.clone());

    println!("Service configured: {}", service.is_configured());

    // Test URL building
    match service.build_api_url(&config.apps.weather.default_location) {
        Ok(url) => println!("Built URL: {}", url),
        Err(e) => println!("URL build error: {:?}", e),
    }

    // Test API call
    match service.get_weather().await {
        Ok(weather) => println!("Weather result: {}", weather),
        Err(e) => println!("Weather error: {:?}", e),
    }
}
