//! Servicio de ejemplo para el primer arranque.

use fakes_domain::*;
use serde_json::json;

fn scenario(id: &str, name: &str, rules: Vec<Rule>, response: ResponseDef) -> Scenario {
    Scenario {
        id: id.into(),
        name: name.into(),
        description: String::new(),
        enabled: true,
        rule_mode: RuleMode::All,
        rules,
        response,
    }
}

fn endpoint(
    id: &str,
    name: &str,
    method: HttpMethod,
    path: &str,
    scenarios: Vec<Scenario>,
) -> Endpoint {
    Endpoint {
        schema_version: SCHEMA_VERSION,
        id: id.into(),
        name: name.into(),
        description: String::new(),
        enabled: true,
        method,
        path: path.into(),
        request: RequestSpec::default(),
        scenarios,
        active_scenario: None,
        on_validation_error: None,
    }
}

pub fn example_service() -> Service {
    let hola = endpoint(
        "get-hola",
        "Saludo",
        HttpMethod::Get,
        "/hola",
        vec![scenario(
            "ok",
            "Saludo",
            vec![],
            ResponseDef::json(
                200,
                json!({
                    "mensaje": "Hola {{ request.query.nombre or 'desde Fake Services' }}",
                    "id": "{{ uuid() }}",
                    "fecha": "{{ now() }}"
                }),
            ),
        )],
    );

    let mut usuario = endpoint(
        "get-usuarios-id",
        "Consultar usuario",
        HttpMethod::Get,
        "/usuarios/{id}",
        vec![
            scenario(
                "no-encontrado",
                "Usuario inexistente",
                vec![Rule {
                    source: RuleSource::Path,
                    key: "id".into(),
                    operator: Operator::Equals,
                    value: Some(json!("0")),
                }],
                ResponseDef::json(
                    404,
                    json!({"error": "not_found", "message": "El usuario {{ request.params.id }} no existe"}),
                ),
            ),
            scenario(
                "ok",
                "Usuario encontrado",
                vec![],
                ResponseDef::json(
                    200,
                    json!({
                        "id": "{{ request.params.id }}",
                        "nombre": "{{ fake('full_name') }}",
                        "email": "{{ fake('email') }}",
                        "rfc": "{{ fake('rfc') }}",
                        "saldo": "{{ random_float(100, 50000) }}"
                    }),
                ),
            ),
        ],
    );
    usuario.description = "Prueba con /usuarios/0 para ver el escenario 404.".into();

    let mut crear = endpoint(
        "post-pagos",
        "Crear pago",
        HttpMethod::Post,
        "/pagos",
        vec![
            scenario(
                "rechazado",
                "Pago rechazado (monto mayor a 10,000)",
                vec![Rule {
                    source: RuleSource::Body,
                    key: "monto".into(),
                    operator: Operator::GreaterThan,
                    value: Some(json!(10000)),
                }],
                ResponseDef::json(
                    402,
                    json!({"error": "insufficient_funds", "message": "Fondos insuficientes"}),
                ),
            ),
            scenario("ok", "Pago creado", vec![], {
                let mut r = ResponseDef::json(
                    201,
                    json!({
                        "id": "pay_{{ random_hex(12) }}",
                        "monto": "{{ request.body.monto }}",
                        "moneda": "{{ request.body.moneda or 'MXN' }}",
                        "estado": "pendiente"
                    }),
                );
                r.delay_ms = 300;
                r
            }),
        ],
    );
    crear.request.body = Some(BodySpec {
        kind: BodyKind::Json,
        required: true,
        schema: Some(json!({
            "type": "object",
            "required": ["monto"],
            "properties": {
                "monto": {"type": "number", "exclusiveMinimum": 0},
                "moneda": {"type": "string", "enum": ["MXN", "USD"]}
            }
        })),
        example: Some(json!({"monto": 1500, "moneda": "MXN"})),
    });

    Service {
        schema_version: SCHEMA_VERSION,
        id: "ejemplo".into(),
        name: "Ejemplo".into(),
        description: "Servicio de muestra. Edítalo o bórralo cuando quieras.".into(),
        upstream_url: None,
        prefix: "/ejemplo".into(),
        port: None,
        auto_start: true,
        variables: Default::default(),
        auth: AuthConfig::None,
        endpoints: vec![hola, usuario, crear],
    }
}
