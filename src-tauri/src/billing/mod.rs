//! Billing module
//!
//! Handles subscription management, plan definitions, and Stripe integration.
//!
//! # Submodules
//! - `client`: Billing service client for API communication
//!
//! # Backend Communication
//! The desktop app communicates with a backend (Token Service / Cloudflare Worker)
//! that handles Stripe interactions. The desktop app does NOT interact with Stripe directly.
//!
//! # Requirements
//! - Requirement 10.1: Three plans (BYOK Free, Starter, Pro)
//! - Requirement 10.3: Stripe connection with 30s timeout
//! - Requirement 10.4: Retry logic (2 retries, 5s interval)

pub mod client;

// Re-export types for convenient access
pub use client::{
    BillingClient, BillingError, Plan, PlanDetails, SubscriptionInfo, SubscriptionStatus,
    CheckoutSession, CustomerPortalSession,
};
