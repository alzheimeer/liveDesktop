//! Authentication module
//!
//! Handles user authentication, API key management, and token services.
//!
//! # Submodules
//! - `better_auth`: Better Auth client for OAuth and email/password authentication
//! - `token_service`: Ephemeral token generation for Gemini API access
//! - `keyring`: Secure OS keyring storage for BYOK API keys
//! - `byok_validator`: BYOK API key validation against Gemini API
//! - `byok_connector`: Direct BYOK connection to Gemini Live (Requirements 8.4, 8.5, 23.4)

pub mod better_auth;
pub mod byok_connector;
pub mod byok_validator;
pub mod keyring;
pub mod token_service;

// Re-export Better Auth types, functions, and client
pub use better_auth::{
    validate_email, validate_password, BetterAuthClient, BetterAuthError, SubscriptionPlan,
    UserSession,
};

// Re-export keyring types and convenience functions
pub use keyring::{
    delete_byok_key, get_byok_key, has_byok_key, set_byok_key, validate_key_format,
    KeyringError, KeyringManager,
};

// Re-export BYOK validator types and functions
pub use byok_validator::{
    validate_api_key, validate_api_key_format, validate_stored_byok_key, ByokValidationError,
    ByokValidator, ValidationResult,
};

// Re-export token service types and client
pub use token_service::{EphemeralToken, TokenServiceClient, TokenServiceError};

// Re-export token renewal manager types
pub use token_service::{
    emit_token_event, spawn_token_event_forwarder, token_event_names, RenewalConfig,
    RenewalState, TokenRenewalEvent, TokenRenewalManager,
};

// Re-export BYOK connector types and struct
pub use byok_connector::{ByokConnectionError, ByokGeminiConnector};
