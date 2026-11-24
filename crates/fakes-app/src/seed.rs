//! Servicio de ejemplo para el primer arranque.

use fakes_domain::*;
use serde_json::json;

fn workflow(v: serde_json::Value) -> Workflow {
    serde_json::from_value(v).expect("workflow de ejemplo válido")
}

fn link(workflow_id: &str) -> WorkflowLink {
    WorkflowLink {
        workflow_id: workflow_id.into(),
        when: StartWhen::ResponseSent,
        input: Default::default(),
        enabled: true,
    }
}

fn scenario(id: &str, name: &str, rules: Vec<Rule>, response: ResponseDef) -> Scenario {
    Scenario {
        id: id.into(),
        name: name.into(),
        description: String::new(),
        enabled: true,
        rule_mode: RuleMode::All,
        workflows: vec![],
        probability: None,
        resource: None,
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

    // Flujo asíncrono tipo MiFiel: responde 201 y luego notifica por webhook.
    let mut firmar = endpoint(
        "post-documentos-id-firmar",
        "Solicitar firma (con webhooks)",
        HttpMethod::Post,
        "/documentos/{id}/firmar",
        vec![
            {
                let mut sc = scenario(
                    "rechazada",
                    "Firma rechazada (firmante con «rechazo» en el correo)",
                    vec![Rule {
                        source: RuleSource::Body,
                        key: "firmante".into(),
                        operator: Operator::Contains,
                        value: Some(json!("rechazo")),
                    }],
                    ResponseDef::json(
                        201,
                        json!({"id": "{{ request.params.id }}", "estado": "pendiente"}),
                    ),
                );
                sc.workflows = vec![link("notificar-rechazo")];
                sc
            },
            {
                let mut sc = scenario(
                    "ok",
                    "Firma exitosa",
                    vec![],
                    ResponseDef::json(
                        201,
                        json!({
                            "id": "{{ request.params.id }}",
                            "solicitud": "sol_{{ random_hex(10) }}",
                            "estado": "pendiente"
                        }),
                    ),
                );
                sc.workflows = vec![link("notificar-firma")];
                sc
            },
        ],
    );
    firmar.description = "Responde 201 y después envía webhooks al callback_url: «pendiente» a los 5 s y «firmado» a los 15 s. Si omites callback_url, se usa el receptor de este mismo servicio.".into();
    firmar.request.body = Some(BodySpec {
        kind: BodyKind::Json,
        required: false,
        schema: Some(json!({
            "type": "object",
            "properties": {
                "callback_url": {"type": "string"},
                "firmante": {"type": "string"}
            }
        })),
        example: Some(
            json!({"callback_url": "http://127.0.0.1:4000/ejemplo/webhooks", "firmante": "ana@example.com"}),
        ),
    });

    let receptor = endpoint(
        "post-webhooks",
        "Receptor de webhooks (para probar)",
        HttpMethod::Post,
        "/webhooks",
        vec![scenario(
            "ok",
            "Recibido",
            vec![],
            ResponseDef::json(200, json!({"recibido": true})),
        )],
    );

    let destino = "{{ request.body.callback_url or (service.url ~ '/webhooks') }}";
    let notificar = workflow(json!({
        "id": "notificar-firma",
        "name": "Notificar firma",
        "description": "Avisa al callback que el documento está pendiente y, después, firmado.",
        "steps": [
            {"id": "espera-pendiente", "type": "wait", "durationMs": 5000},
            {"id": "pendiente", "type": "http", "method": "POST", "url": destino,
             "body": {"type": "json", "content": {
                 "evento": "documento.pendiente",
                 "documento": "{{ response.body.id }}",
                 "solicitud": "{{ response.body.solicitud }}"
             }},
             "retry": {"maxAttempts": 3, "backoffMs": 2000}},
            {"id": "espera-firma", "type": "wait", "durationMs": 15000},
            {"id": "firmado", "type": "http", "method": "POST", "url": destino,
             "body": {"type": "json", "content": {
                 "evento": "documento.firmado",
                 "documento": "{{ response.body.id }}",
                 "solicitud": "{{ response.body.solicitud }}",
                 "firmado_en": "{{ now() }}"
             }},
             "retry": {"maxAttempts": 3, "backoffMs": 2000}},
            {"id": "aviso", "type": "emit", "event": "documento.firmado", "data": {"documento": "{{ response.body.id }}"}}
        ]
    }));
    let rechazo = workflow(json!({
        "id": "notificar-rechazo",
        "name": "Notificar rechazo",
        "description": "Avisa al callback que el firmante rechazó el documento.",
        "steps": [
            {"id": "espera", "type": "wait", "durationMs": 5000},
            {"id": "rechazado", "type": "http", "method": "POST", "url": destino,
             "body": {"type": "json", "content": {
                 "evento": "documento.rechazado",
                 "documento": "{{ response.body.id }}",
                 "motivo": "El firmante rechazó el documento"
             }}}
        ]
    }));

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
        workflows: vec![notificar, rechazo],
        endpoints: vec![hola, usuario, crear, firmar, receptor],
    }
}
