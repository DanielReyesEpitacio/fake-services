//! Detección de integraciones externas en un proyecto.
//!
//! Recorre el código (sin ejecutarlo) y busca tres tipos de evidencia:
//! dependencias de SDKs conocidos, URLs de APIs y variables de entorno con
//! prefijos de proveedores. El resultado sirve a un agente (o a una persona)
//! para decidir qué simulaciones crear y con qué plantilla.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use schemars::JsonSchema;
use serde::Serialize;

const MAX_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_EVIDENCE: usize = 8;

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".nuxt",
    "coverage",
    ".idea",
    ".vscode",
    ".gradle",
    "bin",
    "obj",
    ".terraform",
    ".cache",
    ".fakes",
];

const TEXT_EXTENSIONS: &[&str] = &[
    "js",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "jsx",
    "vue",
    "svelte",
    "py",
    "rb",
    "go",
    "rs",
    "java",
    "kt",
    "kts",
    "cs",
    "php",
    "swift",
    "scala",
    "ex",
    "exs",
    "dart",
    "json",
    "yaml",
    "yml",
    "toml",
    "env",
    "properties",
    "xml",
    "gradle",
    "ini",
    "cfg",
    "conf",
    "sh",
    "txt",
    "md",
    "lock",
];

/// Proveedor conocido.
struct Provider {
    id: &'static str,
    name: &'static str,
    /// Plantilla incluida en Fake Services, si existe.
    template: Option<&'static str>,
    hosts: &'static [&'static str],
    packages: &'static [&'static str],
    env_prefixes: &'static [&'static str],
}

const PROVIDERS: &[Provider] = &[
    Provider {
        id: "mifiel",
        name: "MiFiel",
        template: Some("mifiel"),
        hosts: &["mifiel.com"],
        packages: &["mifiel"],
        env_prefixes: &["MIFIEL_"],
    },
    Provider {
        id: "finkok",
        name: "Finkok",
        template: Some("finkok"),
        hosts: &["finkok.com"],
        packages: &["finkok"],
        env_prefixes: &["FINKOK_"],
    },
    Provider {
        id: "stripe",
        name: "Stripe",
        template: Some("stripe"),
        hosts: &["api.stripe.com"],
        packages: &[
            "stripe",
            "@stripe/stripe-js",
            "stripe-go",
            "stripe/stripe-php",
            "async-stripe",
        ],
        env_prefixes: &["STRIPE_"],
    },
    Provider {
        id: "sendgrid",
        name: "SendGrid",
        template: Some("sendgrid"),
        hosts: &["api.sendgrid.com"],
        packages: &[
            "@sendgrid/mail",
            "@sendgrid/client",
            "sendgrid",
            "sendgrid/sendgrid",
        ],
        env_prefixes: &["SENDGRID_"],
    },
    Provider {
        id: "aws-s3",
        name: "AWS S3",
        template: Some("aws-s3"),
        hosts: &["s3.amazonaws.com", ".s3.", "s3-"],
        packages: &[
            "@aws-sdk/client-s3",
            "aws-sdk",
            "boto3",
            "aws-sdk-s3",
            "github.com/aws/aws-sdk-go-v2/service/s3",
            "aws/aws-sdk-php",
        ],
        env_prefixes: &["AWS_S3_", "S3_BUCKET", "AWS_BUCKET"],
    },
    Provider {
        id: "belvo",
        name: "Belvo",
        template: Some("belvo"),
        hosts: &["belvo.com"],
        packages: &["belvo", "belvo-python", "belvo-js"],
        env_prefixes: &["BELVO_"],
    },
    Provider {
        id: "copomex",
        name: "COPOMEX",
        template: Some("copomex"),
        hosts: &["copomex.com"],
        packages: &[],
        env_prefixes: &["COPOMEX_"],
    },
    Provider {
        id: "twilio",
        name: "Twilio",
        template: None,
        hosts: &["api.twilio.com"],
        packages: &["twilio"],
        env_prefixes: &["TWILIO_"],
    },
    Provider {
        id: "mercadopago",
        name: "Mercado Pago",
        template: None,
        hosts: &["api.mercadopago.com"],
        packages: &["mercadopago"],
        env_prefixes: &["MERCADOPAGO_", "MP_ACCESS_TOKEN"],
    },
    Provider {
        id: "conekta",
        name: "Conekta",
        template: None,
        hosts: &["api.conekta.io"],
        packages: &["conekta"],
        env_prefixes: &["CONEKTA_"],
    },
    Provider {
        id: "openpay",
        name: "Openpay",
        template: None,
        hosts: &["openpay.mx"],
        packages: &["openpay"],
        env_prefixes: &["OPENPAY_"],
    },
    Provider {
        id: "facturama",
        name: "Facturama",
        template: None,
        hosts: &["facturama.mx"],
        packages: &["facturama"],
        env_prefixes: &["FACTURAMA_"],
    },
    Provider {
        id: "sw-sapien",
        name: "SW Sapien",
        template: None,
        hosts: &["sw.com.mx"],
        packages: &["sw-sdk"],
        env_prefixes: &["SW_SAPIEN_", "SW_TOKEN"],
    },
    Provider {
        id: "paypal",
        name: "PayPal",
        template: None,
        hosts: &["api-m.paypal.com", "api.paypal.com", "sandbox.paypal.com"],
        packages: &["@paypal/checkout-server-sdk", "paypalrestsdk"],
        env_prefixes: &["PAYPAL_"],
    },
    Provider {
        id: "mailgun",
        name: "Mailgun",
        template: None,
        hosts: &["api.mailgun.net"],
        packages: &["mailgun.js", "mailgun-js"],
        env_prefixes: &["MAILGUN_"],
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        template: None,
        hosts: &["api.openai.com"],
        packages: &["openai"],
        env_prefixes: &["OPENAI_"],
    },
    Provider {
        id: "slack",
        name: "Slack",
        template: None,
        hosts: &["hooks.slack.com", "slack.com/api"],
        packages: &["@slack/web-api", "slack_sdk"],
        env_prefixes: &["SLACK_"],
    },
];

/// Hosts que no son integraciones (documentación, esquemas, CDNs...).
const IGNORED_HOSTS: &[&str] = &[
    "localhost",
    "127.0.0.1",
    "0.0.0.0",
    "example.com",
    "example.org",
    "www.w3.org",
    "w3.org",
    "schemas.xmlsoap.org",
    "www.sat.gob.mx",
    "json-schema.org",
    "github.com",
    "raw.githubusercontent.com",
    "www.npmjs.com",
    "registry.npmjs.org",
    "npmjs.org",
    "pypi.org",
    "crates.io",
    "docs.rs",
    "fonts.googleapis.com",
    "fonts.gstatic.com",
    "cdn.jsdelivr.net",
    "unpkg.com",
    "cdnjs.cloudflare.com",
    "www.apache.org",
    "opensource.org",
    "stackoverflow.com",
    "developer.mozilla.org",
    "goo.gl",
    "bit.ly",
    "schema.org",
    "xmlns.com",
    "purl.org",
    "www.google.com",
    "maven.apache.org",
    "repo.maven.apache.org",
    "gradle.org",
    "nodejs.org",
    "rustup.rs",
    "yarnpkg.com",
    "registry.yarnpkg.com",
    "tools.ietf.org",
    "www.rfc-editor.org",
];

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    /// Ruta relativa al proyecto.
    pub file: String,
    pub line: usize,
    /// `dependency`, `url` o `env`.
    pub kind: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedIntegration {
    pub provider_id: String,
    pub name: String,
    /// Plantilla de Fake Services lista para instalar (`instalar_plantilla`).
    pub template: Option<String>,
    /// `alta` (SDK o URL), `media` (variables de entorno).
    pub confidence: String,
    /// URLs base encontradas en el código.
    pub base_urls: Vec<String>,
    pub occurrences: usize,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnknownHost {
    pub host: String,
    pub urls: Vec<String>,
    pub occurrences: usize,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectionReport {
    pub root: String,
    pub files_scanned: usize,
    /// `true` si se alcanzó el límite de archivos.
    pub truncated: bool,
    pub integrations: Vec<DetectedIntegration>,
    /// Otros dominios de APIs encontrados que no son proveedores conocidos.
    pub other_hosts: Vec<UnknownHost>,
}

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"https?://([A-Za-z0-9][A-Za-z0-9.-]*\.[A-Za-z]{2,})(:\d+)?(/[^\s"'`<>)\]},;\\]*)?"#,
        )
        .unwrap()
    })
}

fn env_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([A-Z][A-Z0-9]*_[A-Z0-9_]+)\b").unwrap())
}

#[derive(Default)]
struct Acc {
    occurrences: usize,
    confidence_high: bool,
    base_urls: BTreeSet<String>,
    evidence: Vec<Evidence>,
}

impl Acc {
    fn add(&mut self, ev: Evidence, high: bool) {
        self.occurrences += 1;
        self.confidence_high |= high;
        if self.evidence.len() < MAX_EVIDENCE
            && !self
                .evidence
                .iter()
                .any(|e| e.file == ev.file && e.kind == ev.kind)
        {
            self.evidence.push(ev);
        }
    }
}

fn snippet(line: &str) -> String {
    let t = line.trim();
    if t.chars().count() > 160 {
        format!("{}…", t.chars().take(160).collect::<String>())
    } else {
        t.to_string()
    }
}

fn host_matches(host: &str, url: &str, pattern: &str) -> bool {
    if pattern.contains('/') {
        return url.contains(pattern);
    }
    if pattern.starts_with('.') || pattern.ends_with('-') {
        return host.contains(pattern) && host.ends_with("amazonaws.com");
    }
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

/// Analiza un proyecto. No ejecuta nada ni sigue enlaces simbólicos.
pub fn detect_integrations(root: &Path) -> Result<DetectionReport, String> {
    if !root.is_dir() {
        return Err(format!("{} no es una carpeta", root.display()));
    }
    let mut accs: BTreeMap<&'static str, Acc> = BTreeMap::new();
    let mut unknown: BTreeMap<String, (usize, BTreeSet<String>, BTreeSet<String>)> =
        BTreeMap::new();
    let mut files = 0usize;
    let mut truncated = false;

    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir()
                && e.depth() > 0
                && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        });
    for entry in walker.filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_env = name.starts_with(".env") || name.ends_with(".env");
        let is_manifest = matches!(
            name.as_str(),
            "package.json"
                | "requirements.txt"
                | "pyproject.toml"
                | "Pipfile"
                | "go.mod"
                | "composer.json"
                | "Gemfile"
                | "Cargo.toml"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
                | "pubspec.yaml"
        );
        if !(is_env || is_manifest || TEXT_EXTENSIONS.contains(&ext.as_str())) {
            continue;
        }
        if name.ends_with(".lock")
            || name == "package-lock.json"
            || name == "pnpm-lock.yaml"
            || name == "yarn.lock"
        {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > MAX_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        files += 1;
        if files > MAX_FILES {
            truncated = true;
            break;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        for (lineno, line) in content.lines().enumerate() {
            let lineno = lineno + 1;
            if line.len() > 4000 {
                continue; // archivos minificados
            }
            // Dependencias de SDKs.
            if is_manifest {
                let lower = line.to_ascii_lowercase();
                for p in PROVIDERS {
                    if p.packages
                        .iter()
                        .any(|pkg| dependency_mentions(&lower, pkg))
                    {
                        accs.entry(p.id).or_default().add(
                            Evidence {
                                file: rel.clone(),
                                line: lineno,
                                kind: "dependency".into(),
                                snippet: snippet(line),
                            },
                            true,
                        );
                    }
                }
            }
            // URLs.
            for cap in url_re().captures_iter(line) {
                let host = cap[1].to_ascii_lowercase();
                let url = cap[0].trim_end_matches(['.', '/']).to_string();
                if IGNORED_HOSTS
                    .iter()
                    .any(|h| host == *h || host.ends_with(&format!(".{h}")))
                {
                    continue;
                }
                if let Some(p) = PROVIDERS
                    .iter()
                    .find(|p| p.hosts.iter().any(|h| host_matches(&host, &url, h)))
                {
                    let acc = accs.entry(p.id).or_default();
                    acc.base_urls.insert(url.clone());
                    acc.add(
                        Evidence {
                            file: rel.clone(),
                            line: lineno,
                            kind: "url".into(),
                            snippet: snippet(line),
                        },
                        true,
                    );
                } else if host.starts_with("api.")
                    || host.contains(".api.")
                    || url.contains("/api/")
                    || url.contains("/v1")
                    || url.contains("/v2")
                    || is_env
                {
                    let e = unknown.entry(host).or_default();
                    e.0 += 1;
                    if e.1.len() < 5 {
                        e.1.insert(url);
                    }
                    if e.2.len() < 5 {
                        e.2.insert(rel.clone());
                    }
                }
            }
            // Variables de entorno.
            for cap in env_re().captures_iter(line) {
                let var = &cap[1];
                if let Some(p) = PROVIDERS
                    .iter()
                    .find(|p| p.env_prefixes.iter().any(|pre| var.starts_with(pre)))
                {
                    accs.entry(p.id).or_default().add(
                        Evidence {
                            file: rel.clone(),
                            line: lineno,
                            kind: "env".into(),
                            snippet: snippet(line),
                        },
                        false,
                    );
                }
            }
        }
    }

    let mut integrations: Vec<DetectedIntegration> = accs
        .into_iter()
        .filter_map(|(id, acc)| {
            let p = PROVIDERS.iter().find(|p| p.id == id)?;
            Some(DetectedIntegration {
                provider_id: p.id.into(),
                name: p.name.into(),
                template: p.template.map(str::to_string),
                confidence: if acc.confidence_high { "alta" } else { "media" }.into(),
                base_urls: acc.base_urls.into_iter().take(5).collect(),
                occurrences: acc.occurrences,
                evidence: acc.evidence,
            })
        })
        .collect();
    integrations.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then(b.occurrences.cmp(&a.occurrences))
    });

    let mut other_hosts: Vec<UnknownHost> = unknown
        .into_iter()
        .map(|(host, (n, urls, files))| UnknownHost {
            host,
            urls: urls.into_iter().collect(),
            occurrences: n,
            files: files.into_iter().collect(),
        })
        .collect();
    other_hosts.sort_by_key(|h| std::cmp::Reverse(h.occurrences));
    other_hosts.truncate(30);

    Ok(DetectionReport {
        root: root.display().to_string(),
        files_scanned: files.min(MAX_FILES),
        truncated,
        integrations,
        other_hosts,
    })
}

/// ¿La línea de un manifiesto declara esa dependencia?
fn dependency_mentions(lower_line: &str, pkg: &str) -> bool {
    let pkg = pkg.to_ascii_lowercase();
    // "stripe": "^14", stripe==7.0, stripe = "0.30", gem 'stripe', require github.com/...
    let quoted = [format!("\"{pkg}\""), format!("'{pkg}'")];
    if quoted.iter().any(|q| lower_line.contains(q)) {
        return true;
    }
    let trimmed = lower_line.trim_start();
    trimmed.starts_with(&pkg)
        && trimmed[pkg.len()..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '=' | '>' | '<' | '~' | ' ' | '[' | ';' | '\t' | '^'))
        || lower_line.contains(&format!("<artifactid>{pkg}</artifactid>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sdks_urls_and_env() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        std::fs::write(
            p.join("package.json"),
            r#"{"dependencies": {"stripe": "^14.0.0", "@sendgrid/mail": "^8", "express": "^4"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(p.join("src/firma")).unwrap();
        std::fs::write(
            p.join("src/firma/cliente.ts"),
            "const BASE = 'https://app.mifiel.com/api/v1';\nfetch(`${BASE}/documents`)\n// docs: https://github.com/x/y\n",
        )
        .unwrap();
        std::fs::write(p.join("src/cp.py"), "r = get('https://api.copomex.com/query/info_cp/06000')\nother = 'https://api.miproveedor.mx/v2/clientes'\n").unwrap();
        std::fs::write(
            p.join(".env.example"),
            "BELVO_SECRET_ID=xxx\nFINKOK_USER=demo\n",
        )
        .unwrap();
        std::fs::create_dir_all(p.join("node_modules/stripe")).unwrap();
        std::fs::write(
            p.join("node_modules/stripe/index.js"),
            "https://api.stripe.com/v1",
        )
        .unwrap();

        let r = detect_integrations(p).unwrap();
        let ids: Vec<&str> = r
            .integrations
            .iter()
            .map(|i| i.provider_id.as_str())
            .collect();
        for id in ["stripe", "sendgrid", "mifiel", "copomex", "belvo", "finkok"] {
            assert!(ids.contains(&id), "falta {id}: {ids:?}");
        }
        let mifiel = r
            .integrations
            .iter()
            .find(|i| i.provider_id == "mifiel")
            .unwrap();
        assert_eq!(mifiel.base_urls, vec!["https://app.mifiel.com/api/v1"]);
        assert_eq!(mifiel.confidence, "alta");
        assert_eq!(mifiel.template.as_deref(), Some("mifiel"));
        assert_eq!(mifiel.evidence[0].file, "src/firma/cliente.ts");
        let belvo = r
            .integrations
            .iter()
            .find(|i| i.provider_id == "belvo")
            .unwrap();
        assert_eq!(belvo.confidence, "media");
        // node_modules se ignora; github.com también.
        let stripe = r
            .integrations
            .iter()
            .find(|i| i.provider_id == "stripe")
            .unwrap();
        assert!(
            stripe
                .evidence
                .iter()
                .all(|e| !e.file.contains("node_modules"))
        );
        assert!(r.other_hosts.iter().any(|h| h.host == "api.miproveedor.mx"));
        assert!(!r.other_hosts.iter().any(|h| h.host == "github.com"));
    }

    #[test]
    fn dependency_matching() {
        assert!(dependency_mentions("stripe==7.1.0", "stripe"));
        assert!(dependency_mentions("    \"stripe\": \"^14\",", "stripe"));
        assert!(!dependency_mentions("stripe-mock-helper==1", "stripe"));
        assert!(dependency_mentions("boto3>=1.28", "boto3"));
    }
}
