// https://docs.rs/axum-client-ip/0.3.1/src/axum_client_ip/lib.rs.html#1-194

use std::net::{IpAddr, SocketAddr};

use axum::{extract::ConnectInfo, http::Extensions};
use forwarded_header_value::{ForwardedHeaderValue, Identifier};
use hyper::{header::FORWARDED, HeaderMap};

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";

pub fn real_ip(headers: &HeaderMap, extensions: &Extensions) -> Option<IpAddr> {
    maybe_x_forwarded_for(headers)
        .or_else(|| maybe_x_real_ip(headers))
        .or_else(|| maybe_forwarded(headers))
        .or_else(|| maybe_connect_info(extensions))
}

/// Tries to parse the `x-forwarded-for` header
fn maybe_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_FORWARDED_FOR)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| {
            s.split(',')
                .rev()
                .find_map(|s| s.trim().parse::<IpAddr>().ok())
        })
}

/// Tries to parse the `x-real-ip` header
fn maybe_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_REAL_IP)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.parse::<IpAddr>().ok())
}

/// Tries to parse `forwarded` headers
fn maybe_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    headers.get_all(FORWARDED).iter().find_map(|hv| {
        hv.to_str()
            .ok()
            .and_then(|s| ForwardedHeaderValue::from_forwarded(s).ok())
            .and_then(|f| {
                f.iter()
                    .filter_map(|fs| fs.forwarded_for.as_ref())
                    .find_map(|ff| match ff {
                        Identifier::SocketAddr(a) => Some(a.ip()),
                        Identifier::IpAddr(ip) => Some(*ip),
                        _ => None,
                    })
            })
    })
}

/// Looks in `ConnectInfo` extension
fn maybe_connect_info(extensions: &Extensions) -> Option<IpAddr> {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip())
}