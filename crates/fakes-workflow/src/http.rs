//! Cliente HTTP de los workflows con protección SSRF.
//!
//! La política se aplica a la IP **resuelta** (tanto en una verificación
//! previa como dentro del resolvedor DNS del cliente), así un nombre que
//! cambia de IP entre consultas (DNS rebinding) tampoco pasa. No se siguen
//! redirecciones ni se usan proxies del entorno.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use fakes_domain::WebhookSettings;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde_json::{Map, Value};

/// Bytes máximos que se guardan de la respuesta de un webhook.
pub const MAX_CAPTURE: usize = 256 * 1024;

const BLOCKED_HOSTNAMES: &[&str] = &[
    "metadata.google.internal",
    "metadata.goog",
    "metadata",
    "instance-data",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpClass {
    Public,
    Loopback,
    Private,
    /// Link-local, metadatos de nube, multicast, sin especificar...: nunca se permiten.
    Forbidden(&'static str),
}

fn classify_v4(ip: Ipv4Addr) -> IpClass {
    let o = ip.octets();
    if ip == Ipv4Addr::new(169, 254, 169, 254) || ip == Ipv4Addr::new(100, 100, 100, 200) {
        return IpClass::Forbidden("es una dirección de metadatos de nube");
    }
    if ip.is_link_local() {
        return IpClass::Forbidden("es una dirección link-local");
    }
    if ip.is_unspecified() || o[0] == 0 {
        return IpClass::Forbidden("es una dirección no especificada");
    }
    if ip.is_multicast() || ip.is_broadcast() || o[0] >= 240 {
        return IpClass::Forbidden("es una dirección de multicast o reservada");
    }
    if ip.is_loopback() {
        return IpClass::Loopback;
    }
    if ip.is_private() || (o[0] == 100 && (64..128).contains(&o[1])) {
        return IpClass::Private;
    }
    IpClass::Public
}

pub fn classify(ip: IpAddr) -> IpClass {
    match ip {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return classify_v4(v4);
            }
            let seg = v6.segments();
            if v6 == "fd00:ec2::254".parse::<Ipv6Addr>().unwrap() {
                return IpClass::Forbidden("es una dirección de metadatos de nube");
            }
            if v6.is_unspecified() {
                return IpClass::Forbidden("es una dirección no especificada");
            }
            if v6.is_multicast() {
                return IpClass::Forbidden("es una dirección de multicast");
            }
            if (seg[0] & 0xffc0) == 0xfe80 {
                return IpClass::Forbidden("es una dirección link-local");
            }
            if v6.is_loopback() {
                return IpClass::Loopback;
            }
            if (seg[0] & 0xfe00) == 0xfc00 {
                return IpClass::Private;
            }
            IpClass::Public
        }
    }
}

/// Decide si una IP es un destino permitido. `Err` explica el motivo.
pub fn check_ip(ip: IpAddr, policy: &WebhookSettings, host_allowed: bool) -> Result<(), String> {
    let explicit = host_allowed
        || policy
            .allowed_hosts
            .iter()
            .any(|h| h.trim().parse::<IpAddr>().is_ok_and(|a| a == ip));
    match classify(ip) {
        IpClass::Public => Ok(()),
        IpClass::Forbidden(why) => Err(format!("{ip} {why}")),
        IpClass::Loopback if policy.allow_loopback || explicit => Ok(()),
        IpClass::Loopback => Err(format!(
            "{ip} es local (loopback); habilita «Permitir localhost» en la política de webhooks"
        )),
        IpClass::Private if policy.allow_private_networks || explicit => Ok(()),
        IpClass::Private => Err(format!(
            "{ip} es una red privada; habilita «Permitir redes privadas» o agrega el host a la lista permitida"
        )),
    }
}

fn host_is_allowed(policy: &WebhookSettings, host: &str) -> bool {
    policy
        .allowed_hosts
        .iter()
        .any(|h| h.trim().eq_ignore_ascii_case(host))
}

/// Resuelve un host y aplica la política. Devuelve solo direcciones permitidas.
async fn resolve_checked(host: &str, policy: &WebhookSettings) -> Result<Vec<SocketAddr>, String> {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if BLOCKED_HOSTNAMES.contains(&lower.as_str()) {
        return Err(format!("{host} es un servicio de metadatos de nube"));
    }
    let allowed = host_is_allowed(policy, &lower);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((lower.as_str(), 0))
        .await
        .map_err(|e| format!("no se pudo resolver {host}: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("{host} no tiene direcciones"));
    }
    let mut ok = Vec::new();
    let mut last_err = String::new();
    for a in addrs {
        match check_ip(a.ip(), policy, allowed) {
            Ok(()) => ok.push(a),
            Err(e) => last_err = e,
        }
    }
    if ok.is_empty() {
        Err(format!("destino bloqueado: {last_err}"))
    } else {
        Ok(ok)
    }
}

struct GuardedResolver {
    policy: Arc<ArcSwap<WebhookSettings>>,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let policy = self.policy.load_full();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = resolve_checked(&host, &policy)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Solicitud ya renderizada.
#[derive(Debug, Clone)]
pub struct OutgoingRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct HttpResult {
    pub status: u16,
    pub headers: Map<String, Value>,
    /// JSON si se pudo interpretar; si no, texto.
    pub body: Value,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum HttpError {
    #[error("destino bloqueado por la política de webhooks: {0}")]
    Blocked(String),
    #[error("URL inválida: {0}")]
    InvalidUrl(String),
    #[error("tiempo de espera agotado tras {0} ms")]
    Timeout(u64),
    #[error("error de conexión: {0}")]
    Connection(String),
}

impl HttpError {
    /// Los bloqueos y URLs inválidas no se reintentan: no cambiarán.
    pub fn retryable(&self) -> bool {
        matches!(self, HttpError::Timeout(_) | HttpError::Connection(_))
    }
}

pub struct WebhookClient {
    client: reqwest::Client,
    policy: Arc<ArcSwap<WebhookSettings>>,
}

impl WebhookClient {
    pub fn new() -> Self {
        let policy = Arc::new(ArcSwap::from_pointee(WebhookSettings::default()));
        let client = reqwest::Client::builder()
            .dns_resolver(Arc::new(GuardedResolver {
                policy: policy.clone(),
            }))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent(concat!(
                "FakeServices/",
                env!("CARGO_PKG_VERSION"),
                " (webhooks)"
            ))
            .pool_max_idle_per_host(4)
            .build()
            .expect("cliente HTTP");
        Self { client, policy }
    }

    pub fn set_policy(&self, policy: WebhookSettings) {
        self.policy.store(Arc::new(policy));
    }

    pub async fn send(&self, req: &OutgoingRequest) -> Result<HttpResult, HttpError> {
        let url = reqwest::Url::parse(req.url.trim())
            .map_err(|e| HttpError::InvalidUrl(format!("'{}': {e}", req.url)))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(HttpError::InvalidUrl(format!(
                "solo se permiten http y https, no '{}'",
                url.scheme()
            )));
        }
        let policy = self.policy.load_full();
        // Verificación previa con mensaje claro (el resolvedor vuelve a verificar).
        match url.host() {
            Some(url::Host::Ipv4(ip)) => {
                check_ip(IpAddr::V4(ip), &policy, false).map_err(HttpError::Blocked)?
            }
            Some(url::Host::Ipv6(ip)) => {
                check_ip(IpAddr::V6(ip), &policy, false).map_err(HttpError::Blocked)?
            }
            Some(url::Host::Domain(d)) => {
                resolve_checked(d, &policy).await.map_err(|e| {
                    if e.starts_with("no se pudo resolver") || e.ends_with("no tiene direcciones") {
                        HttpError::Connection(e)
                    } else {
                        HttpError::Blocked(e.trim_start_matches("destino bloqueado: ").to_string())
                    }
                })?;
            }
            None => return Err(HttpError::InvalidUrl("la URL no tiene host".into())),
        }

        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| HttpError::InvalidUrl(format!("método inválido: {}", req.method)))?;
        let mut builder = self.client.request(method, url).timeout(req.timeout);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        if let Some(body) = &req.body {
            builder = builder.body(body.clone());
        }

        let started = Instant::now();
        let timeout_ms = req.timeout.as_millis() as u64;
        let mut resp = builder.send().await.map_err(|e| map_error(e, timeout_ms))?;
        let status = resp.status().as_u16();
        let mut headers = Map::new();
        for (k, v) in resp.headers() {
            headers.entry(k.as_str().to_string()).or_insert_with(|| {
                Value::String(String::from_utf8_lossy(v.as_bytes()).into_owned())
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| map_error(e, timeout_ms))? {
            let room = MAX_CAPTURE.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
            if bytes.len() >= MAX_CAPTURE {
                break;
            }
        }
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        Ok(HttpResult {
            status,
            headers,
            body,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

impl Default for WebhookClient {
    fn default() -> Self {
        Self::new()
    }
}

fn map_error(e: reqwest::Error, timeout_ms: u64) -> HttpError {
    if e.is_timeout() {
        return HttpError::Timeout(timeout_ms);
    }
    // El resolvedor puede rechazar el destino (DNS rebinding).
    let mut src: Option<&dyn std::error::Error> = Some(&e);
    while let Some(s) = src {
        let msg = s.to_string();
        if msg.starts_with("destino bloqueado") {
            return HttpError::Blocked(msg.trim_start_matches("destino bloqueado: ").to_string());
        }
        src = s.source();
    }
    let detail = std::error::Error::source(&e)
        .map(|s| s.to_string())
        .unwrap_or_else(|| e.to_string());
    HttpError::Connection(detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(loopback: bool, private: bool, allowed: &[&str]) -> WebhookSettings {
        WebhookSettings {
            allow_loopback: loopback,
            allow_private_networks: private,
            allowed_hosts: allowed.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn classification() {
        let c = |s: &str| classify(s.parse().unwrap());
        assert_eq!(c("8.8.8.8"), IpClass::Public);
        assert_eq!(c("127.0.0.1"), IpClass::Loopback);
        assert_eq!(c("::1"), IpClass::Loopback);
        assert_eq!(c("10.1.2.3"), IpClass::Private);
        assert_eq!(c("172.20.0.1"), IpClass::Private);
        assert_eq!(c("192.168.1.10"), IpClass::Private);
        assert_eq!(c("fd12::1"), IpClass::Private);
        assert!(matches!(c("169.254.169.254"), IpClass::Forbidden(_)));
        assert!(matches!(c("169.254.1.1"), IpClass::Forbidden(_)));
        assert!(matches!(c("::ffff:169.254.169.254"), IpClass::Forbidden(_)));
        assert!(matches!(c("fe80::1"), IpClass::Forbidden(_)));
        assert!(matches!(c("0.0.0.0"), IpClass::Forbidden(_)));
        assert!(matches!(c("224.0.0.1"), IpClass::Forbidden(_)));
    }

    #[test]
    fn policy_rules() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        let default = WebhookSettings::default();
        assert!(check_ip(ip("127.0.0.1"), &default, false).is_ok());
        assert!(check_ip(ip("192.168.1.5"), &default, false).is_err());
        assert!(check_ip(ip("192.168.1.5"), &policy(true, true, &[]), false).is_ok());
        assert!(
            check_ip(
                ip("192.168.1.5"),
                &policy(true, false, &["192.168.1.5"]),
                false
            )
            .is_ok()
        );
        assert!(check_ip(ip("127.0.0.1"), &policy(false, false, &[]), false).is_err());
        // Los metadatos nunca, ni con lista permitida.
        assert!(
            check_ip(
                ip("169.254.169.254"),
                &policy(true, true, &["169.254.169.254"]),
                true
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn blocks_before_connecting() {
        let client = WebhookClient::new();
        let req = |url: &str| OutgoingRequest {
            method: "POST".into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            timeout: Duration::from_secs(2),
        };
        let e = client
            .send(&req("http://169.254.169.254/latest/meta-data"))
            .await
            .unwrap_err();
        assert!(matches!(e, HttpError::Blocked(_)), "{e}");
        let e = client
            .send(&req("http://metadata.google.internal/"))
            .await
            .unwrap_err();
        assert!(matches!(e, HttpError::Blocked(_)), "{e}");
        let e = client.send(&req("http://10.0.0.1:9/")).await.unwrap_err();
        assert!(matches!(e, HttpError::Blocked(_)), "{e}");
        let e = client.send(&req("file:///etc/passwd")).await.unwrap_err();
        assert!(matches!(e, HttpError::InvalidUrl(_)), "{e}");
        assert!(!HttpError::Blocked("x".into()).retryable());

        client.set_policy(policy(false, false, &[]));
        let e = client.send(&req("http://localhost:9/")).await.unwrap_err();
        assert!(matches!(e, HttpError::Blocked(_)), "{e}");
    }
}
