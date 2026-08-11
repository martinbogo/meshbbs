//! Weather module for fetching current weather conditions
//!
//! This module provides weather information using OpenWeatherMap API.
//! It supports both city lookup by name and zipcode lookup.

use crate::config::WeatherAppConfig;
use anyhow::{anyhow, Result};
use log::{debug, warn};
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// OpenWeatherMap API response structures
#[derive(Debug, Deserialize)]
pub struct WeatherResponse {
    pub name: String,
    pub sys: WeatherSys,
    pub main: WeatherMain,
    pub weather: Vec<WeatherCondition>,
    pub visibility: Option<i32>,
    pub wind: Option<WeatherWind>,
}

#[derive(Debug, Deserialize)]
pub struct WeatherSys {
    pub country: String,
}

#[derive(Debug, Deserialize)]
pub struct WeatherMain {
    pub temp: f64,
    pub feels_like: f64,
    pub humidity: i32,
    pub pressure: i32,
}

#[derive(Debug, Deserialize)]
pub struct WeatherCondition {
    pub main: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct WeatherWind {
    pub speed: f64,
    pub deg: Option<i32>,
}

/// Weather cache entry
#[derive(Debug, Clone)]
pub struct WeatherCacheEntry {
    pub fetched_at: Instant,
    pub data: String,
    pub location: String,
}

/// Weather service for fetching current conditions
pub struct WeatherService {
    config: WeatherAppConfig,
    cache: Option<WeatherCacheEntry>,
    client: reqwest::Client,
}

impl WeatherService {
    /// Create a new weather service with the given configuration
    pub fn new(config: WeatherAppConfig) -> Self {
        Self {
            config,
            cache: None,
            client: reqwest::Client::new(),
        }
    }

    /// Update the weather configuration
    pub fn update_config(&mut self, config: WeatherAppConfig) {
        self.config = config;
        // Clear cache when config changes
        self.cache = None;
    }

    /// Fetch current weather for the default location
    pub async fn get_weather(&mut self) -> Result<String> {
        match self.fetch_current_weather().await {
            Some(weather) => Ok(weather),
            None => Err(anyhow!("Failed to fetch weather data")),
        }
    }

    /// Fetch current weather for the default location
    pub async fn fetch_current_weather(&mut self) -> Option<String> {
        self.fetch_weather_for_location(&self.config.default_location.clone())
            .await
    }

    /// Fetch weather for a specific location
    pub async fn fetch_weather_for_location(&mut self, location: &str) -> Option<String> {
        if !self.config.enabled {
            debug!("Weather service is disabled");
            return Some("Weather service is disabled".to_string());
        }

        if self.config.api_key.is_empty() {
            warn!("OpenWeatherMap API key not configured");
            return Some("Weather: API key not configured".to_string());
        }

        // Check cache first
        if let Some(ref cache) = self.cache {
            if cache.location == location {
                let age = cache.fetched_at.elapsed();
                let ttl = Duration::from_secs(self.config.cache_ttl_minutes as u64 * 60);

                if age < ttl {
                    debug!(
                        "Returning cached weather for {} (age: {:.1}min)",
                        location,
                        age.as_secs_f64() / 60.0
                    );
                    return Some(cache.data.clone());
                }
            }
        }

        // Fetch fresh data
        match self.fetch_from_api(location).await {
            Ok(response) => {
                let formatted = self.format_weather_response(&response);

                // Update cache
                self.cache = Some(WeatherCacheEntry {
                    fetched_at: Instant::now(),
                    data: formatted.clone(),
                    location: location.to_string(),
                });

                debug!("Weather fetched successfully for {}", location);
                Some(formatted)
            }
            Err(e) => {
                warn!("Failed to fetch weather for {}: {}", location, e);

                // Return stale cache if available and not too old (up to 2 hours)
                if let Some(ref cache) = self.cache {
                    if cache.location == location {
                        let age = cache.fetched_at.elapsed();
                        if age < Duration::from_secs(2 * 60 * 60) {
                            debug!(
                                "Returning stale cached weather for {} (age: {:.1}min)",
                                location,
                                age.as_secs_f64() / 60.0
                            );
                            return Some(format!("{} (cached)", cache.data));
                        }
                    }
                }

                Some("Weather: Unable to fetch current conditions".to_string())
            }
        }
    }

    /// Fetch weather data from OpenWeatherMap API
    async fn fetch_from_api(&self, location: &str) -> Result<WeatherResponse> {
        let url = self.build_api_url(location)?;
        debug!("Fetching weather from: {}", url);

        let request = self.client.get(&url);
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds as u64);

        let response = timeout(timeout_duration, request.send())
            .await
            .map_err(|_| anyhow!("Request timeout after {}s", self.config.timeout_seconds))?
            .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("API returned status: {}", response.status()));
        }

        let weather_response: WeatherResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse JSON response: {}", e))?;

        Ok(weather_response)
    }

    /// Build the API URL based on location type
    pub fn build_api_url(&self, location: &str) -> Result<String> {
        let base_url = "https://api.openweathermap.org/data/2.5/weather";
        let api_key = &self.config.api_key;

        match self.config.location_type.as_str() {
            "city" => {
                let query = if let Some(country) = &self.config.country_code {
                    format!("{},{}", location, country)
                } else {
                    location.to_string()
                };
                Ok(format!(
                    "{}?q={}&appid={}&units=imperial",
                    base_url,
                    urlencoding::encode(&query),
                    api_key
                ))
            }
            "zipcode" => {
                let query = if let Some(country) = &self.config.country_code {
                    format!("{},{}", location, country)
                } else {
                    location.to_string()
                };
                Ok(format!(
                    "{}?zip={}&appid={}&units=imperial",
                    base_url,
                    urlencoding::encode(&query),
                    api_key
                ))
            }
            "city_id" => Ok(format!(
                "{}?id={}&appid={}&units=imperial",
                base_url, location, api_key
            )),
            _ => Err(anyhow!(
                "Invalid location_type: {}",
                self.config.location_type
            )),
        }
    }

    /// Format the weather response into a user-friendly string
    fn format_weather_response(&self, response: &WeatherResponse) -> String {
        let location = format!("{}, {}", response.name, response.sys.country);
        let temp = format!("{:.0}°F", response.main.temp);
        let condition = &response.weather[0].description;

        // Capitalize first letter of each word in condition
        let formatted_condition = condition
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        format!("Weather: {}: {} {}", location, formatted_condition, temp)
    }

    /// Clear the weather cache
    pub fn clear_cache(&mut self) {
        self.cache = None;
    }

    /// Check if the service is properly configured
    pub fn is_configured(&self) -> bool {
        self.config.enabled && !self.config.api_key.is_empty()
    }
}
