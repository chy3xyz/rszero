//! Distributed trace context propagation across HTTP and RPC boundaries.
//!
//! Implements W3C Trace Context propagation using `traceparent` and `tracestate`
//! headers, allowing spans to flow through service boundaries.
//!
//! # Example
//!
//! ```no_run
//! use rszero::trace::propagation::{TraceContext, inject_http, extract_http};
//!
//! # fn example() {
//! let ctx = TraceContext::new();
//! let mut headers = std::collections::HashMap::new();
//! inject_http(&ctx, &mut headers);
//! // send request with headers...
//! // on receiver:
//! let ctx2 = extract_http(&headers).unwrap();
//! # }
//! ```

use std::collections::HashMap;

/// W3C Trace Context header name.
pub const TRACEPARENT_HEADER: &str = "traceparent";
/// W3C Trace State header name.
pub const TRACESTATE_HEADER: &str = "tracestate";

/// Parsed W3C trace context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// Trace ID (16 bytes hex = 32 chars).
    pub trace_id: String,
    /// Parent span ID (8 bytes hex = 16 chars).
    pub parent_span_id: String,
    /// Flags (currently only bit 0 = sampled).
    pub flags: u8,
}

impl TraceContext {
    /// Create a new trace context with random IDs.
    pub fn new() -> Self {
        Self {
            trace_id: generate_hex(32),
            parent_span_id: generate_hex(16),
            flags: 1, // sampled
        }
    }

    /// Create a trace context from a traceparent string.
    ///
    /// Format: `00-{trace_id}-{parent_span_id}-{flags}`
    pub fn parse(traceparent: &str) -> Option<Self> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 || parts[0] != "00" {
            return None;
        }
        let trace_id = parts[1].to_string();
        let parent_span_id = parts[2].to_string();
        let flags = u8::from_str_radix(parts[3], 16).ok()?;

        if trace_id.len() != 32 || parent_span_id.len() != 16 {
            return None;
        }

        Some(Self {
            trace_id,
            parent_span_id,
            flags,
        })
    }

    /// Format as a W3C traceparent string.
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, self.parent_span_id, self.flags)
    }

    /// Check if the sampled flag is set.
    pub fn is_sampled(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Inject trace context into HTTP headers.
pub fn inject_http(ctx: &TraceContext, headers: &mut HashMap<String, String>) {
    headers.insert(TRACEPARENT_HEADER.to_string(), ctx.to_traceparent());
}

/// Extract trace context from HTTP headers.
pub fn extract_http(headers: &HashMap<String, String>) -> Option<TraceContext> {
    headers
        .get(TRACEPARENT_HEADER)
        .and_then(|v| TraceContext::parse(v))
}

/// Generate a random hex string of the given length.
fn generate_hex(len: usize) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(len);
    for _ in 0..(len / 2) {
        let _ = write!(s, "{:02x}", fastrand::u8(..));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_roundtrip() {
        let ctx = TraceContext::new();
        let tp = ctx.to_traceparent();
        let ctx2 = TraceContext::parse(&tp).unwrap();
        assert_eq!(ctx.trace_id, ctx2.trace_id);
        assert_eq!(ctx.parent_span_id, ctx2.parent_span_id);
        assert_eq!(ctx.flags, ctx2.flags);
    }

    #[test]
    fn test_trace_context_parse() {
        let tp = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::parse(tp).unwrap();
        assert_eq!(ctx.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.parent_span_id, "b7ad6b7169203331");
        assert_eq!(ctx.flags, 1);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_trace_context_invalid() {
        assert!(TraceContext::parse("bad").is_none());
        assert!(TraceContext::parse("00-short-123-01").is_none());
    }

    #[test]
    fn test_inject_extract_http() {
        let ctx = TraceContext::new();
        let mut headers = HashMap::new();
        inject_http(&ctx, &mut headers);
        let ctx2 = extract_http(&headers).unwrap();
        assert_eq!(ctx.trace_id, ctx2.trace_id);
    }
}
