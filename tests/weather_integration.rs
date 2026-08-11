//! Integration tests for weather service with real API calls
//! These tests require a valid OpenWeatherMap API key in config.toml

use meshbbs::bbs::weather::WeatherService;
use meshbbs::config::{Config, WeatherAppConfig};

/// Test weather service with real API (requires config.toml with valid API key)
#[tokio::test]
#[ignore] // Ignore by default since it requires network and API key
async fn test_weather_service_real_api() {
    // Try to load config from file
    let config_result = Config::load("config.toml").await;
    if config_result.is_err() {
        println!("Skipping integration test: no config.toml file found");
        return;
    }

    let config = config_result.unwrap();
    if config.apps.weather.api_key.is_empty() {
        println!("Skipping integration test: no API key in config.toml");
        return;
    }

    let mut service = WeatherService::new(config.apps.weather.clone());

    // Test fetching weather for default location
    match service.get_weather().await {
        Ok(weather) => {
            println!("Weather result: {}", weather);
            assert!(weather.starts_with("Weather:"));
            // Should contain temperature information
            assert!(weather.contains("°"));
        }
        Err(e) => {
            panic!("Weather API call failed: {:?}", e);
        }
    }
}

/// Test weather service configuration validation
#[test]
fn test_weather_config_validation() {
    let valid_config = WeatherAppConfig {
        api_key: "test_key".to_string(),
        default_location: "Los Angeles".to_string(),
        location_type: "city".to_string(),
        country_code: Some("US".to_string()),
        cache_ttl_minutes: 10,
        timeout_seconds: 5,
        enabled: true,
    };

    let service = WeatherService::new(valid_config);
    assert!(service.is_configured());
}

/// Test weather service URL building
#[test]
fn test_weather_url_building() {
    let config = WeatherAppConfig {
        api_key: "test_api_key".to_string(),
        default_location: "Los Angeles".to_string(),
        location_type: "city".to_string(),
        country_code: Some("US".to_string()),
        cache_ttl_minutes: 10,
        timeout_seconds: 5,
        enabled: true,
    };

    let service = WeatherService::new(config);

    // Test city URL building
    let url = service.build_api_url("New York").unwrap();
    assert!(url.contains("q=New%20York%2CUS"));
    assert!(url.contains("appid=test_api_key"));
    assert!(url.contains("units=imperial"));
}

/// Test zipcode URL building
#[test]
fn test_zipcode_url_building() {
    let config = WeatherAppConfig {
        api_key: "test_key".to_string(),
        default_location: "90210".to_string(),
        location_type: "zipcode".to_string(),
        country_code: Some("US".to_string()),
        cache_ttl_minutes: 10,
        timeout_seconds: 5,
        enabled: true,
    };

    let service = WeatherService::new(config);
    let url = service.build_api_url("10001").unwrap();
    assert!(url.contains("zip=10001%2CUS"));
    assert!(url.contains("appid=test_key"));
}
