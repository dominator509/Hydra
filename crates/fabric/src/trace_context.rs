use axum::http::{HeaderMap, HeaderValue};

pub const TRACEPARENT_HEADER: &str = "traceparent";
pub const TRACESTATE_HEADER: &str = "tracestate";

pub fn server_trace_context(headers: &HeaderMap) -> store::TraceContext {
    let incoming = headers
        .get(TRACEPARENT_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|traceparent| {
            let tracestate = headers
                .get(TRACESTATE_HEADER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            store::TraceContext::new(traceparent.to_owned(), tracestate).ok()
        });

    incoming
        .map(|context| context.child())
        .unwrap_or_else(store::TraceContext::fresh)
}

pub fn insert_trace_headers(headers: &mut HeaderMap, trace_context: &store::TraceContext) {
    if let Ok(value) = HeaderValue::from_str(&trace_context.traceparent) {
        headers.insert(TRACEPARENT_HEADER, value);
    }
    if let Some(tracestate) = trace_context.tracestate.as_deref() {
        if let Ok(value) = HeaderValue::from_str(tracestate) {
            headers.insert(TRACESTATE_HEADER, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_parent_creates_server_child_without_accepting_baggage() {
        let mut headers = HeaderMap::new();
        headers.insert(
            TRACEPARENT_HEADER,
            HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
        );
        headers.insert("baggage", HeaderValue::from_static("customer=private"));

        let context = server_trace_context(&headers);
        assert_eq!(context.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_ne!(
            context.traceparent,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
        assert!(context.tracestate.is_none());
    }

    #[test]
    fn invalid_parent_starts_fresh_trace() {
        let mut headers = HeaderMap::new();
        headers.insert(TRACEPARENT_HEADER, HeaderValue::from_static("invalid"));

        let context = server_trace_context(&headers);
        assert!(context.validate().is_ok());
        assert_ne!(context.traceparent, "invalid");
    }
}
