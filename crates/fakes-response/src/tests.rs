use super::*;
use fakes_domain::{ResponseBody, ResponseDef};
use serde_json::json;

fn ctx() -> Value {
    json!({
        "request": {
            "method": "POST",
            "params": {"id": "doc_123"},
            "query": {"page": "2"},
            "headers": {"x-request-id": "abc"},
            "body": {"callback_url": "http://localhost:3000/hook", "amount": 42, "signers": [{"email": "a@b.c"}]}
        },
        "vars": {"TOKEN": "t0k"}
    })
}

#[test]
fn json_body_keeps_types_for_single_expressions() {
    let def = ResponseDef::json(
        201,
        json!({
            "id": "{{ request.params.id }}",
            "amount": "{{ request.body.amount }}",
            "signers": "{{ request.body.signers }}",
            "missing": "{{ request.body.nope.deep }}",
            "label": "Documento {{ request.params.id }} (página {{ request.query.page }})",
            "literal": 5
        }),
    );
    let r = render_response(&def, &ctx(), None).unwrap();
    assert_eq!(r.status, 201);
    assert_eq!(r.header("content-type"), Some("application/json"));
    let body: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(
        body,
        json!({
            "id": "doc_123",
            "amount": 42,
            "signers": [{"email": "a@b.c"}],
            "missing": null,
            "label": "Documento doc_123 (página 2)",
            "literal": 5
        })
    );
}

#[test]
fn text_headers_and_helpers() {
    let mut def = ResponseDef::json(200, json!(null));
    def.headers.insert(
        "x-request-id".into(),
        "{{ request.headers['x-request-id'] }}".into(),
    );
    def.headers
        .insert("Content-Type".into(), "application/xml".into());
    def.body = ResponseBody::Text {
        content: "<doc id=\"{{ request.params.id }}\" uuid=\"{{ uuid() }}\">{{ vars.TOKEN | b64encode }}</doc>".into(),
        content_type: None,
    };
    let r = render_response(&def, &ctx(), None).unwrap();
    assert_eq!(r.header("x-request-id"), Some("abc"));
    // header del usuario manda sobre el content-type por defecto
    assert_eq!(r.header("content-type"), Some("application/xml"));
    assert_eq!(r.headers.len(), 2);
    let text = String::from_utf8(r.body).unwrap();
    assert!(text.starts_with("<doc id=\"doc_123\" uuid=\""));
    assert!(text.ends_with(">dDBr</doc>"));
}

#[test]
fn helper_functions_work() {
    let ctx = MjValue::from_serialize(json!({}));
    for (src, check) in [
        (
            "{{ random_int(5, 5) }}",
            Box::new(|s: &str| s == "5") as Box<dyn Fn(&str) -> bool>,
        ),
        (
            "{{ random_string(8) | length }}",
            Box::new(|s: &str| s == "8"),
        ),
        ("{{ random_hex(6) | length }}", Box::new(|s: &str| s == "6")),
        ("{{ uuid() | length }}", Box::new(|s: &str| s == "36")),
        ("{{ fake('rfc') | length }}", Box::new(|s: &str| s == "13")),
        (
            "{{ fake('clabe') | length }}",
            Box::new(|s: &str| s == "18"),
        ),
        ("{{ pick(['a']) }}", Box::new(|s: &str| s == "a")),
        ("{{ now('%Y') | length }}", Box::new(|s: &str| s == "4")),
        (
            "{{ ('{\"a\": 1}' | fromjson).a }}",
            Box::new(|s: &str| s == "1"),
        ),
    ] {
        let out = render_text(src, &ctx, "t").unwrap();
        assert!(check(&out), "{src} -> {out}");
    }
    assert!(render_text("{{ fake('nope') }}", &ctx, "t").is_err());
}

#[test]
fn infinite_loops_are_cut_by_fuel() {
    let ctx = MjValue::from_serialize(json!({}));
    let src = "{% for i in range(100000) %}{% for j in range(100000) %}x{% endfor %}{% endfor %}";
    assert!(render_text(src, &ctx, "body").is_err());
}

#[test]
fn template_syntax_validation() {
    let mut def = ResponseDef::json(200, json!({"a": "{{ request.body.x ", "b": "{{ ok }}"}));
    def.headers.insert("x".into(), "{% if %}".into());
    let issues = validate_templates("response", &def);
    let fields: Vec<_> = issues.iter().map(|i| i.field.as_str()).collect();
    assert_eq!(
        fields,
        vec!["response.headers.x", "response.body.content.a"]
    );
}

#[test]
fn binary_and_file_bodies() {
    let mut def = ResponseDef::json(200, json!(null));
    def.body = ResponseBody::Binary {
        base64: "aGV5".into(),
        content_type: None,
    };
    let r = render_response(&def, &ctx(), None).unwrap();
    assert_eq!(r.body, b"hey");
    assert_eq!(r.header("content-type"), Some("application/octet-stream"));

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("doc_123.pdf"), b"%PDF").unwrap();
    def.body = ResponseBody::File {
        path: "{{ request.params.id }}.pdf".into(),
        content_type: None,
    };
    let r = render_response(&def, &ctx(), Some(tmp.path())).unwrap();
    assert_eq!(r.body, b"%PDF");
    assert_eq!(r.header("content-type"), Some("application/pdf"));

    def.body = ResponseBody::File {
        path: "../etc/passwd".into(),
        content_type: None,
    };
    assert!(render_response(&def, &ctx(), Some(tmp.path())).is_err());
}

#[test]
fn delay_and_jitter() {
    let mut def = ResponseDef::json(200, json!(null));
    def.delay_ms = 100;
    def.jitter_ms = 50;
    let r = render_response(&def, &ctx(), None).unwrap();
    assert!((100..=150).contains(&r.delay_ms));
}
