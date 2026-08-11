//! Integration tests for the weather service
use meshbbs::bbs::weather::WeatherService;
use meshbbs::config::WeatherAppConfig;

#[tokio::test]
async fn test_weather_service_disabled() {
    let config = WeatherAppConfig {
        api_key: "".to_string(),
        default_location: "Los Angeles".to_string(),
        location_type: "city".to_string(),
        country_code: Some("US".to_string()),
        cache_ttl_minutes: 5,
        timeout_seconds: 10,
        enabled: false,
    };

    let mut service = WeatherService::new(config);
    let result = service.get_weather().await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Weather service is disabled");
}

#[tokio::test]
async fn test_weather_service_no_api_key() {
    let config = WeatherAppConfig {
        api_key: "".to_string(),
        default_location: "Los Angeles".to_string(),
        location_type: "city".to_string(),
        country_code: Some("US".to_string()),
        cache_ttl_minutes: 5,
        timeout_seconds: 10,
        enabled: true,
    };

    let mut service = WeatherService::new(config);
    let result = service.get_weather().await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Weather: API key not configured");
}

#[tokio::test]
async fn test_weather_service_invalid_api_key() {
    let config = WeatherAppConfig {
        api_key: "invalid_key".to_string(),
        default_location: "Los Angeles".to_string(),
        location_type: "city".to_string(),
        country_code: Some("US".to_string()),
        cache_ttl_minutes: 5,
        timeout_seconds: 2, // Short timeout for test
        enabled: true,
    };

    let mut service = WeatherService::new(config);
    let result = service.get_weather().await;
    // Should return an error message about being unable to fetch conditions
    match result {
        Ok(msg) => {
            assert!(msg.contains("Unable to fetch current conditions"));
        }
        Err(_) => {
            // It's also acceptable to return an Err
        }
    }
}

#[test]
fn test_weather_config_default() {
    let config = WeatherAppConfig::default();
    assert_eq!(config.default_location, "Los Angeles");
    assert_eq!(config.location_type, "city");
    assert_eq!(config.cache_ttl_minutes, 10);
    assert_eq!(config.timeout_seconds, 5);
    assert!(!config.enabled); // Should be disabled by default
    assert_eq!(config.country_code, Some("US".to_string()));
}

#[test]
fn test_weather_service_initialization() {
    let config = WeatherAppConfig::default();
    let service = WeatherService::new(config);
    assert!(!service.is_configured()); // Should not be configured without API key
}
