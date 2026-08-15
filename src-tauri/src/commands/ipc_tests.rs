//! Property-Based Tests for IPC Command Round-Trip
//!
//! **Property 9: IPC Command Round-Trip**
//!
//! Verifies that `deserialize(serialize(input)) = input` for all IPC types.
//! This ensures that data passed between the Rust backend and TypeScript frontend
//! maintains integrity through JSON serialization/deserialization.
//!
//! **Validates: Requirements 22.3, 22.4**
//! - 22.3: THE Desktop_App SHALL serializar datos entre Rust y TypeScript usando JSON
//! - 22.4: FOR ALL comandos IPC definidos, invocar desde TypeScript y manejar en Rust
//!         SHALL producir la respuesta esperada (propiedad de round-trip)
//!
//! # Types Tested
//! - AudioDevice
//! - ChannelConfig
//! - ChannelState
//! - ChannelType
//! - EngineState
//! - AudioMetrics
//! - PauseReason
//! - DeviceEvent
//! - DeviceEventType
//! - UserSession
//! - SubscriptionPlan
//! - AppConfig
//! - LanguageConfig
//! - DeviceConfig
//! - PreferencesConfig
//! - UsageStats
//! - DailyUsage
//! - ValidationResult

#[cfg(test)]
#[cfg(feature = "audio")]
mod property_tests {
    use proptest::prelude::*;
    use serde::{Deserialize, Serialize};
    
    // Import IPC types from audio module
    use crate::audio::engine::{
        AudioDevice, AudioMetrics, ChannelConfig, ChannelState, ChannelType,
        DeviceEvent, DeviceEventType, EngineState, PauseReason,
    };
    
    // Import auth types
    use crate::auth::{SubscriptionPlan, UserSession, ValidationResult};
    
    // Import config types
    use crate::storage::{
        AppConfig, DeviceConfig, LanguageConfig, PreferencesConfig,
        DailyUsage, UsageStats,
    };


    // ========================================================================
    // Proptest Strategies for IPC Types
    // ========================================================================

    /// Strategy for generating valid ISO 639-1 language codes
    fn language_code_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("en".to_string()),
            Just("es".to_string()),
            Just("fr".to_string()),
            Just("de".to_string()),
            Just("it".to_string()),
            Just("pt".to_string()),
            Just("ja".to_string()),
            Just("ko".to_string()),
            Just("zh".to_string()),
            Just("ru".to_string()),
            Just("ar".to_string()),
            Just("hi".to_string()),
        ]
    }

    /// Strategy for generating valid device IDs
    fn device_id_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,64}".prop_map(|s| s)
    }

    /// Strategy for generating valid device names
    fn device_name_strategy() -> impl Strategy<Value = String> {
        "[A-Za-z0-9 ()_-]{1,128}".prop_map(|s| s)
    }


    /// Strategy for generating valid theme values
    fn theme_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("dark".to_string()),
            Just("light".to_string()),
            Just("system".to_string()),
        ]
    }

    /// Strategy for AudioDevice
    fn audio_device_strategy() -> impl Strategy<Value = AudioDevice> {
        (
            device_id_strategy(),
            device_name_strategy(),
            prop_oneof![
                Just("input".to_string()),
                Just("output".to_string()),
                Just("loopback".to_string()),
            ],
            any::<bool>(),
        )
            .prop_map(|(id, name, device_type, is_default)| AudioDevice {
                id,
                name,
                device_type,
                is_default,
            })
    }

    /// Strategy for ChannelConfig
    fn channel_config_strategy() -> impl Strategy<Value = ChannelConfig> {
        (
            language_code_strategy(),
            language_code_strategy(),
            device_id_strategy(),
            device_id_strategy(),
        )
            .prop_map(|(source_lang, target_lang, input_device, output_device)| {
                ChannelConfig {
                    source_lang,
                    target_lang,
                    input_device,
                    output_device,
                }
            })
    }


    /// Strategy for ChannelState
    fn channel_state_strategy() -> impl Strategy<Value = ChannelState> {
        prop_oneof![
            Just(ChannelState::Inactive),
            Just(ChannelState::Active),
            Just(ChannelState::Paused),
            "[A-Za-z0-9 .,!?]{1,200}".prop_map(|message| ChannelState::Error { message }),
        ]
    }

    /// Strategy for ChannelType
    fn channel_type_strategy() -> impl Strategy<Value = ChannelType> {
        prop_oneof![Just(ChannelType::System), Just(ChannelType::User),]
    }

    /// Strategy for AudioMetrics
    fn audio_metrics_strategy() -> impl Strategy<Value = AudioMetrics> {
        (
            -60.0f32..=0.0f32,
            -60.0f32..=0.0f32,
            0u32..=2000u32,
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(
                |(input_level_db, output_level_db, latency_ms, packets_sent, packets_received)| {
                    AudioMetrics {
                        input_level_db,
                        output_level_db,
                        latency_ms,
                        packets_sent,
                        packets_received,
                    }
                },
            )
    }


    /// Strategy for PauseReason
    fn pause_reason_strategy() -> impl Strategy<Value = PauseReason> {
        prop_oneof![
            Just(PauseReason::UserRequested),
            (device_id_strategy(), device_name_strategy()).prop_map(|(device_id, device_name)| {
                PauseReason::DeviceDisconnected {
                    device_id,
                    device_name,
                }
            }),
            Just(PauseReason::NetworkError),
            Just(PauseReason::GeminiDisconnected),
        ]
    }

    /// Strategy for DeviceEventType
    fn device_event_type_strategy() -> impl Strategy<Value = DeviceEventType> {
        prop_oneof![
            Just(DeviceEventType::Connected),
            Just(DeviceEventType::Disconnected),
            Just(DeviceEventType::StateChanged),
        ]
    }

    /// Strategy for DeviceEvent
    fn device_event_strategy() -> impl Strategy<Value = DeviceEvent> {
        (device_event_type_strategy(), audio_device_strategy())
            .prop_map(|(event_type, device)| DeviceEvent { event_type, device })
    }


    /// Strategy for EngineState
    fn engine_state_strategy() -> impl Strategy<Value = EngineState> {
        (
            channel_state_strategy(),
            channel_state_strategy(),
            proptest::option::of(audio_metrics_strategy()),
            proptest::option::of(pause_reason_strategy()),
        )
            .prop_map(
                |(system_channel, user_channel, metrics, pause_reason)| EngineState {
                    system_channel,
                    user_channel,
                    metrics,
                    pause_reason,
                },
            )
    }

    /// Strategy for SubscriptionPlan
    fn subscription_plan_strategy() -> impl Strategy<Value = SubscriptionPlan> {
        prop_oneof![
            Just(SubscriptionPlan::ByokFree),
            Just(SubscriptionPlan::Starter),
            Just(SubscriptionPlan::Pro),
        ]
    }

    /// Strategy for valid email addresses
    fn email_strategy() -> impl Strategy<Value = String> {
        "[a-z]{3,10}@[a-z]{3,8}\\.(com|org|net|io)"
    }


    /// Strategy for ISO 8601 date strings
    fn iso_date_strategy() -> impl Strategy<Value = String> {
        (2020i32..2030i32, 1u32..=12u32, 1u32..=28u32, 0u32..24u32, 0u32..60u32, 0u32..60u32)
            .prop_map(|(year, month, day, hour, min, sec)| {
                format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                    year, month, day, hour, min, sec
                )
            })
    }

    /// Strategy for UserSession
    fn user_session_strategy() -> impl Strategy<Value = UserSession> {
        (
            "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}", // user_id UUID
            email_strategy(),
            "[A-Za-z ]{2,50}",                                               // name
            proptest::option::of("[a-z]{10,30}"),                            // avatar_url
            subscription_plan_strategy(),
            "[a-zA-Z0-9_-]{20,64}",                                          // session_token
            iso_date_strategy(),                                             // expires_at
        )
            .prop_map(
                |(user_id, email, name, avatar_url, plan, session_token, expires_at)| {
                    UserSession {
                        user_id,
                        email,
                        name,
                        avatar_url,
                        plan,
                        session_token,
                        expires_at,
                    }
                },
            )
    }


    /// Strategy for LanguageConfig
    fn language_config_strategy() -> impl Strategy<Value = LanguageConfig> {
        (
            language_code_strategy(),
            language_code_strategy(),
            language_code_strategy(),
            language_code_strategy(),
        )
            .prop_map(
                |(system_source_lang, system_target_lang, user_source_lang, user_target_lang)| {
                    LanguageConfig {
                        system_source_lang,
                        system_target_lang,
                        user_source_lang,
                        user_target_lang,
                    }
                },
            )
    }

    /// Strategy for DeviceConfig
    fn device_config_strategy() -> impl Strategy<Value = DeviceConfig> {
        (
            proptest::option::of(device_id_strategy()),
            proptest::option::of(device_id_strategy()),
            proptest::option::of(device_id_strategy()),
        )
            .prop_map(|(input_device, system_capture_device, output_device)| {
                DeviceConfig {
                    input_device,
                    system_capture_device,
                    output_device,
                }
            })
    }


    /// Strategy for PreferencesConfig
    fn preferences_config_strategy() -> impl Strategy<Value = PreferencesConfig> {
        (any::<bool>(), any::<bool>(), theme_strategy(), any::<bool>()).prop_map(
            |(start_minimized, auto_start, theme, enable_sentry)| PreferencesConfig {
                start_minimized,
                auto_start,
                theme,
                enable_sentry,
            },
        )
    }

    /// Strategy for AppConfig
    fn app_config_strategy() -> impl Strategy<Value = AppConfig> {
        (
            language_config_strategy(),
            device_config_strategy(),
            preferences_config_strategy(),
        )
            .prop_map(|(languages, devices, preferences)| AppConfig {
                languages,
                devices,
                preferences,
            })
    }

    /// Strategy for UsageStats
    fn usage_stats_strategy() -> impl Strategy<Value = UsageStats> {
        (0u32..10000u32, 0u32..5000u32).prop_map(|(total_minutes_used, minutes_limit)| {
            UsageStats::new(total_minutes_used, minutes_limit)
        })
    }


    /// Strategy for DailyUsage
    fn daily_usage_strategy() -> impl Strategy<Value = DailyUsage> {
        (
            "[0-9]{4}-[0-9]{2}-[0-9]{2}", // date in YYYY-MM-DD format
            0u32..1440u32,                 // system_minutes (max 24 hours)
            0u32..1440u32,                 // user_minutes (max 24 hours)
        )
            .prop_map(|(date, system_minutes, user_minutes)| DailyUsage {
                date,
                system_minutes,
                user_minutes,
                total_minutes: system_minutes + user_minutes,
            })
    }

    /// Strategy for ValidationResult
    fn validation_result_strategy() -> impl Strategy<Value = ValidationResult> {
        prop_oneof![
            Just(ValidationResult {
                valid: true,
                error_message: None,
                suggestion: None,
            }),
            ("[A-Za-z0-9 .,!?]{10,100}", "[A-Za-z0-9 .,!?]{10,100}").prop_map(
                |(error_message, suggestion)| ValidationResult {
                    valid: false,
                    error_message: Some(error_message),
                    suggestion: Some(suggestion),
                }
            ),
        ]
    }


    // ========================================================================
    // Property Test: IPC Round-Trip Serialization
    // ========================================================================

    /// Helper function to test round-trip serialization
    /// 
    /// Verifies that for any value T:
    /// - T can be serialized to JSON
    /// - The JSON can be deserialized back to T
    /// - The result equals the original value
    fn assert_roundtrip<T>(original: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        // Serialize to JSON
        let json = serde_json::to_string(original)
            .expect("Serialization should succeed");
        
        // Deserialize back
        let deserialized: T = serde_json::from_str(&json)
            .expect("Deserialization should succeed");
        
        // Verify equality
        assert_eq!(
            *original, deserialized,
            "Round-trip failed:\nOriginal: {:?}\nJSON: {}\nDeserialized: {:?}",
            original, json, deserialized
        );
    }


    // ========================================================================
    // Property Tests using proptest!
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Property 9a: AudioDevice IPC Round-Trip**
        ///
        /// Verifies that AudioDevice serializes and deserializes correctly.
        /// This type is used in enumerate_audio_devices and device selection.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_audio_device_roundtrip(device in audio_device_strategy()) {
            assert_roundtrip(&device);
        }

        /// **Property 9b: ChannelConfig IPC Round-Trip**
        ///
        /// Verifies that ChannelConfig serializes and deserializes correctly.
        /// This type is used in start_system_channel and start_user_channel.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_channel_config_roundtrip(config in channel_config_strategy()) {
            assert_roundtrip(&config);
        }


        /// **Property 9c: ChannelState IPC Round-Trip**
        ///
        /// Verifies that ChannelState (including Error variant) serializes correctly.
        /// This type is used in get_audio_state and channel-state events.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_channel_state_roundtrip(state in channel_state_strategy()) {
            assert_roundtrip(&state);
        }

        /// **Property 9d: ChannelType IPC Round-Trip**
        ///
        /// Verifies that ChannelType serializes and deserializes correctly.
        /// This enum is used to specify which channel to operate on.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_channel_type_roundtrip(channel_type in channel_type_strategy()) {
            assert_roundtrip(&channel_type);
        }

        /// **Property 9e: AudioMetrics IPC Round-Trip**
        ///
        /// Verifies that AudioMetrics with all fields serializes correctly.
        /// This type is used in audio-metrics events (100ms updates).
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_audio_metrics_roundtrip(metrics in audio_metrics_strategy()) {
            assert_roundtrip(&metrics);
        }


        /// **Property 9f: EngineState IPC Round-Trip**
        ///
        /// Verifies that the complete EngineState serializes correctly.
        /// This type is returned by get_audio_state command.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_engine_state_roundtrip(state in engine_state_strategy()) {
            assert_roundtrip(&state);
        }

        /// **Property 9g: PauseReason IPC Round-Trip**
        ///
        /// Verifies that PauseReason (all variants) serializes correctly.
        /// This is part of EngineState and device-changed events.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_pause_reason_roundtrip(reason in pause_reason_strategy()) {
            assert_roundtrip(&reason);
        }

        /// **Property 9h: DeviceEvent IPC Round-Trip**
        ///
        /// Verifies that DeviceEvent serializes and deserializes correctly.
        /// This type is used in device-changed events.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_device_event_roundtrip(event in device_event_strategy()) {
            assert_roundtrip(&event);
        }


        /// **Property 9i: UserSession IPC Round-Trip**
        ///
        /// Verifies that UserSession serializes and deserializes correctly.
        /// This type is returned by login commands and get_session.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_user_session_roundtrip(session in user_session_strategy()) {
            assert_roundtrip(&session);
        }

        /// **Property 9j: SubscriptionPlan IPC Round-Trip**
        ///
        /// Verifies that SubscriptionPlan enum serializes correctly.
        /// This is part of UserSession and billing commands.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_subscription_plan_roundtrip(plan in subscription_plan_strategy()) {
            assert_roundtrip(&plan);
        }

        /// **Property 9k: AppConfig IPC Round-Trip**
        ///
        /// Verifies that complete AppConfig serializes correctly.
        /// This type is used in get_config, save_config, export_config.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_app_config_roundtrip(config in app_config_strategy()) {
            assert_roundtrip(&config);
        }


        /// **Property 9l: LanguageConfig IPC Round-Trip**
        ///
        /// Verifies that LanguageConfig serializes correctly.
        /// Part of AppConfig, used for language preferences.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_language_config_roundtrip(config in language_config_strategy()) {
            assert_roundtrip(&config);
        }

        /// **Property 9m: DeviceConfig IPC Round-Trip**
        ///
        /// Verifies that DeviceConfig serializes correctly.
        /// Part of AppConfig, used for device preferences.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_device_config_roundtrip(config in device_config_strategy()) {
            assert_roundtrip(&config);
        }

        /// **Property 9n: PreferencesConfig IPC Round-Trip**
        ///
        /// Verifies that PreferencesConfig serializes correctly.
        /// Part of AppConfig, used for UI preferences.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_preferences_config_roundtrip(config in preferences_config_strategy()) {
            assert_roundtrip(&config);
        }


        /// **Property 9o: UsageStats IPC Round-Trip**
        ///
        /// Verifies that UsageStats serializes correctly.
        /// This type is returned by get_usage_stats command.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_usage_stats_roundtrip(stats in usage_stats_strategy()) {
            assert_roundtrip(&stats);
        }

        /// **Property 9p: DailyUsage IPC Round-Trip**
        ///
        /// Verifies that DailyUsage serializes correctly.
        /// This type is returned by get_usage_history command.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_daily_usage_roundtrip(usage in daily_usage_strategy()) {
            assert_roundtrip(&usage);
        }

        /// **Property 9q: ValidationResult IPC Round-Trip**
        ///
        /// Verifies that ValidationResult serializes correctly.
        /// This type is returned by validate_byok_key_full command.
        ///
        /// **Validates: Requirements 22.3, 22.4**
        #[test]
        fn prop_validation_result_roundtrip(result in validation_result_strategy()) {
            assert_roundtrip(&result);
        }
    }


    // ========================================================================
    // Unit Tests for Edge Cases
    // ========================================================================

    #[test]
    fn test_channel_state_error_with_special_chars() {
        // Test error messages with special characters
        let state = ChannelState::Error {
            message: "Error: conexión perdida «timeout» — reintentar?".to_string(),
        };
        assert_roundtrip(&state);
    }

    #[test]
    fn test_empty_optional_fields() {
        // Test DeviceConfig with all None values
        let config = DeviceConfig {
            input_device: None,
            system_capture_device: None,
            output_device: None,
        };
        assert_roundtrip(&config);
    }

    #[test]
    fn test_full_optional_fields() {
        // Test DeviceConfig with all Some values
        let config = DeviceConfig {
            input_device: Some("mic-123".to_string()),
            system_capture_device: Some("loopback-456".to_string()),
            output_device: Some("speaker-789".to_string()),
        };
        assert_roundtrip(&config);
    }


    #[test]
    fn test_engine_state_all_variants() {
        // Test complete EngineState with all fields populated
        let state = EngineState {
            system_channel: ChannelState::Active,
            user_channel: ChannelState::Paused,
            metrics: Some(AudioMetrics {
                input_level_db: -12.5,
                output_level_db: -18.3,
                latency_ms: 250,
                packets_sent: 1000,
                packets_received: 998,
            }),
            pause_reason: Some(PauseReason::DeviceDisconnected {
                device_id: "usb-mic-001".to_string(),
                device_name: "USB Microphone".to_string(),
            }),
        };
        assert_roundtrip(&state);
    }

    #[test]
    fn test_user_session_all_fields() {
        let session = UserSession {
            user_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            email: "user@example.com".to_string(),
            name: "Test User".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            plan: SubscriptionPlan::Pro,
            session_token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9".to_string(),
            expires_at: "2025-01-15T10:30:00Z".to_string(),
        };
        assert_roundtrip(&session);
    }


    #[test]
    fn test_app_config_full() {
        let config = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "en".to_string(),
                system_target_lang: "es".to_string(),
                user_source_lang: "es".to_string(),
                user_target_lang: "en".to_string(),
            },
            devices: DeviceConfig {
                input_device: Some("mic-001".to_string()),
                system_capture_device: Some("loopback-002".to_string()),
                output_device: Some("speakers-003".to_string()),
            },
            preferences: PreferencesConfig {
                start_minimized: true,
                auto_start: false,
                theme: "dark".to_string(),
                enable_sentry: true,
            },
        };
        assert_roundtrip(&config);
    }

    #[test]
    fn test_validation_result_valid() {
        let result = ValidationResult {
            valid: true,
            error_message: None,
            suggestion: None,
        };
        assert_roundtrip(&result);
    }


    #[test]
    fn test_validation_result_invalid() {
        let result = ValidationResult {
            valid: false,
            error_message: Some("API key format is invalid".to_string()),
            suggestion: Some("Use only alphanumeric characters".to_string()),
        };
        assert_roundtrip(&result);
    }

    #[test]
    fn test_json_camel_case_serialization() {
        // Verify that serde renames fields to camelCase for TypeScript compatibility
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "mic-001".to_string(),
            output_device: "speaker-002".to_string(),
        };
        
        let json = serde_json::to_string(&config).unwrap();
        
        // Verify camelCase field names in JSON
        assert!(json.contains("sourceLang"), "Expected camelCase 'sourceLang' in JSON");
        assert!(json.contains("targetLang"), "Expected camelCase 'targetLang' in JSON");
        assert!(json.contains("inputDevice"), "Expected camelCase 'inputDevice' in JSON");
        assert!(json.contains("outputDevice"), "Expected camelCase 'outputDevice' in JSON");
        
        // Verify snake_case is NOT in JSON
        assert!(!json.contains("source_lang"), "Unexpected snake_case in JSON");
        assert!(!json.contains("target_lang"), "Unexpected snake_case in JSON");
    }
}
