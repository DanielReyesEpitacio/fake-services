use super::*;
use fakes_domain::*;
use serde_json::json;

fn scenario(id: &str, rules: Vec<Rule>, status: u16) -> Scenario {
    Scenario {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        enabled: true,
        rule_mode: RuleMode::All,
        workflows: vec![],
        rules,
        response: ResponseDef::json(status, json!({"s": id})),
    }
}

fn endpoint(id: &str, method: HttpMethod, path: &str, scenarios: Vec<Scenario>) -> Endpoint {
    Endpoint {
        schema_version: 1,
        id: id.into(),
        name: String::new(),
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

fn service(id: &str, prefix: &str, endpoints: Vec<Endpoint>) -> Service {
    Service {
        schema_version: 1,
        id: id.into(),
        name: id.into(),
        description: String::new(),
        upstream_url: None,
        prefix: prefix.into(),
        port: None,
        auto_start: true,
        variables: Default::default(),
        auth: AuthConfig::None,
        workflows: vec![],
        endpoints,
    }
}

fn req(method: &str, path: &str) -> IncomingRequest {
    IncomingRequest {
        method: method.into(),
        path: path.into(),
        ..Default::default()
    }
}

fn rule(source: RuleSource, key: &str, operator: Operator, value: Option<Value>) -> Rule {
    Rule {
        source,
        key: key.into(),
        operator,
        value,
    }
}

fn workspace() -> CompiledWorkspace {
    let mifiel = service(
        "mifiel",
        "/mifiel",
        vec![
            endpoint(
                "doc",
                HttpMethod::Get,
                "/documents/{id}",
                vec![scenario("ok", vec![], 200)],
            ),
            endpoint(
                "latest",
                HttpMethod::Get,
                "/documents/latest",
                vec![scenario("ok", vec![], 200)],
            ),
            endpoint(
                "sign",
                HttpMethod::Post,
                "/documents/{id}/sign",
                vec![scenario("ok", vec![], 201)],
            ),
        ],
    );
    let aws = service(
        "aws",
        "/aws",
        vec![endpoint("any", HttpMethod::Any, "/{*rest}", vec![])],
    );
    let s3 = service(
        "s3",
        "/aws/s3",
        vec![endpoint("obj", HttpMethod::Get, "/{bucket}/{*key}", vec![])],
    );
    CompiledWorkspace::compile(Workspace::new("w", "W"), vec![aws, mifiel, s3])
}

#[test]
fn routes_by_prefix_with_segment_boundaries() {
    let ws = workspace();
    let (s, rest) = ws.service_for_path("/mifiel/documents/1").unwrap();
    assert_eq!(s.service.id, "mifiel");
    assert_eq!(rest, "/documents/1");
    assert!(ws.service_for_path("/mifielx/documents").is_none());
    let (s, rest) = ws.service_for_path("/mifiel").unwrap();
    assert_eq!((s.service.id.as_str(), rest.as_str()), ("mifiel", "/"));
    // prefijo más largo gana
    let (s, _) = ws.service_for_path("/aws/s3/bucket/a.txt").unwrap();
    assert_eq!(s.service.id, "s3");
    let (s, _) = ws.service_for_path("/aws/ec2").unwrap();
    assert_eq!(s.service.id, "aws");
}

#[test]
fn literal_routes_win_over_params() {
    let ws = workspace();
    let svc = ws.service("mifiel").unwrap();
    match svc.find_endpoint("GET", "/documents/latest") {
        EndpointMatch::Found { endpoint, .. } => assert_eq!(endpoint.endpoint.id, "latest"),
        _ => panic!("debió coincidir"),
    }
    match svc.find_endpoint("GET", "/documents/doc%20123") {
        EndpointMatch::Found { endpoint, params } => {
            assert_eq!(endpoint.endpoint.id, "doc");
            assert_eq!(params["id"], "doc 123");
        }
        _ => panic!("debió coincidir"),
    }
}

#[test]
fn method_not_allowed_and_not_found() {
    let ws = workspace();
    let svc = ws.service("mifiel").unwrap();
    match svc.find_endpoint("DELETE", "/documents/1/sign") {
        EndpointMatch::MethodNotAllowed { allowed } => assert_eq!(allowed, vec!["POST"]),
        _ => panic!("debió ser 405"),
    }
    assert!(matches!(
        svc.find_endpoint("GET", "/nada"),
        EndpointMatch::NotFound
    ));
    // HEAD usa el endpoint GET
    assert!(matches!(
        svc.find_endpoint("HEAD", "/documents/1"),
        EndpointMatch::Found { .. }
    ));
}

#[test]
fn catch_all_and_any_method() {
    let ws = workspace();
    let svc = ws.service("s3").unwrap();
    match svc.find_endpoint("GET", "/my-bucket/a/b.txt") {
        EndpointMatch::Found { params, .. } => {
            assert_eq!(params["bucket"], "my-bucket");
            assert_eq!(params["key"], "a/b.txt");
        }
        _ => panic!(),
    }
    let svc = ws.service("aws").unwrap();
    assert!(matches!(
        svc.find_endpoint("PATCH", "/x/y"),
        EndpointMatch::Found { .. }
    ));
}

#[test]
fn scenario_selection_by_rules_and_overrides() {
    let ep = endpoint(
        "sign",
        HttpMethod::Post,
        "/documents/{id}/sign",
        vec![
            scenario(
                "unauthorized",
                vec![rule(
                    RuleSource::Header,
                    "authorization",
                    Operator::NotExists,
                    None,
                )],
                401,
            ),
            scenario(
                "invalid",
                vec![rule(RuleSource::Body, "signers", Operator::NotExists, None)],
                422,
            ),
            scenario(
                "big",
                vec![rule(
                    RuleSource::Body,
                    "amount",
                    Operator::GreaterThan,
                    Some(json!(1000)),
                )],
                402,
            ),
            scenario("ok", vec![], 201),
        ],
    );
    let ce = CompiledEndpoint::compile(ep.clone()).unwrap();
    let vars = BTreeMap::new();

    let mut r = req("POST", "/documents/1/sign");
    assert_eq!(ce.select_scenario(&r, &vars).unwrap().0.id, "unauthorized");

    r.headers.push(("authorization".into(), "Bearer x".into()));
    r.raw_body = b"{}".to_vec();
    r.body = json!({});
    assert_eq!(ce.select_scenario(&r, &vars).unwrap().0.id, "invalid");

    r.body = json!({"signers": [], "amount": "5000"});
    assert_eq!(ce.select_scenario(&r, &vars).unwrap().0.id, "big");

    r.body = json!({"signers": [], "amount": 10});
    let (s, why) = ce.select_scenario(&r, &vars).unwrap();
    assert_eq!((s.id.as_str(), why), ("ok", SelectionReason::Rules));

    // header fuerza escenario
    r.headers
        .push((SCENARIO_HEADER.into(), "unauthorized".into()));
    let (s, why) = ce.select_scenario(&r, &vars).unwrap();
    assert_eq!(
        (s.id.as_str(), why),
        ("unauthorized", SelectionReason::Header)
    );
    r.headers.pop();

    // escenario activo
    let mut ep2 = ep;
    ep2.active_scenario = Some("big".into());
    let ce2 = CompiledEndpoint::compile(ep2).unwrap();
    let (s, why) = ce2.select_scenario(&r, &vars).unwrap();
    assert_eq!((s.id.as_str(), why), ("big", SelectionReason::Active));
}

#[test]
fn any_mode_and_disabled_scenarios() {
    let mut s1 = scenario(
        "a",
        vec![
            rule(RuleSource::Query, "x", Operator::Equals, Some(json!("1"))),
            rule(RuleSource::Query, "y", Operator::Equals, Some(json!("1"))),
        ],
        200,
    );
    s1.rule_mode = RuleMode::Any;
    let mut s0 = scenario("off", vec![], 500);
    s0.enabled = false;
    let ce = CompiledEndpoint::compile(endpoint("e", HttpMethod::Get, "/", vec![s0, s1])).unwrap();
    let mut r = req("GET", "/");
    r.query.push(("y".into(), "1".into()));
    assert_eq!(ce.select_scenario(&r, &BTreeMap::new()).unwrap().0.id, "a");
    let r = req("GET", "/");
    assert!(ce.select_scenario(&r, &BTreeMap::new()).is_none());
}

#[test]
fn operators() {
    let s = |v: &str| json!(v);
    assert!(eval_operator(
        Operator::Equals,
        Some(&s("5")),
        Some(&json!(5)),
        None
    ));
    assert!(eval_operator(
        Operator::Equals,
        Some(&json!(true)),
        Some(&s("TRUE")),
        None
    ));
    assert!(eval_operator(
        Operator::NotEquals,
        None,
        Some(&s("x")),
        None
    ));
    assert!(eval_operator(
        Operator::Contains,
        Some(&s("hello world")),
        Some(&s("lo w")),
        None
    ));
    assert!(eval_operator(
        Operator::Contains,
        Some(&json!([1, 2])),
        Some(&s("2")),
        None
    ));
    assert!(eval_operator(
        Operator::Contains,
        Some(&json!({"k": 1})),
        Some(&s("k")),
        None
    ));
    assert!(eval_operator(
        Operator::NotContains,
        None,
        Some(&s("x")),
        None
    ));
    assert!(eval_operator(
        Operator::LessThan,
        Some(&s("3.5")),
        Some(&json!(4)),
        None
    ));
    assert!(!eval_operator(
        Operator::LessThan,
        Some(&s("abc")),
        Some(&json!(4)),
        None
    ));
    assert!(eval_operator(
        Operator::Regex,
        Some(&s("doc_123")),
        Some(&s("^doc_\\d+$")),
        None
    ));
    assert!(!eval_operator(
        Operator::Exists,
        Some(&Value::Null),
        None,
        None
    ));
    assert!(eval_operator(
        Operator::Equals,
        Some(&json!({"a": [1]})),
        Some(&json!({"a": ["1"]})),
        None
    ));
}

#[test]
fn validation_of_headers_query_and_body() {
    let mut ep = endpoint("e", HttpMethod::Post, "/docs/{id}", vec![]);
    ep.request.headers.push(ParamSpec {
        name: "Authorization".into(),
        required: true,
        description: String::new(),
        pattern: Some("Bearer .+".into()),
        example: None,
    });
    ep.request.query.push(ParamSpec {
        name: "page".into(),
        required: false,
        description: String::new(),
        pattern: Some("\\d+".into()),
        example: None,
    });
    ep.request.path_params.push(ParamSpec {
        name: "id".into(),
        required: true,
        description: String::new(),
        pattern: Some("doc_\\w+".into()),
        example: None,
    });
    ep.request.body = Some(BodySpec {
        kind: BodyKind::Json,
        required: true,
        schema: Some(json!({
            "type": "object",
            "required": ["callback_url"],
            "properties": {"callback_url": {"type": "string"}}
        })),
        example: None,
    });
    let ce = CompiledEndpoint::compile(ep).unwrap();

    let mut r = req("POST", "/docs/x");
    r.params.insert("id".into(), "x".into());
    r.query.push(("page".into(), "abc".into()));
    let fields: Vec<String> = ce.validate(&r).into_iter().map(|i| i.field).collect();
    assert!(fields.contains(&"header.Authorization".to_string()));
    assert!(fields.contains(&"query.page".to_string()));
    assert!(fields.contains(&"path.id".to_string()));
    assert!(fields.contains(&"body".to_string()));

    let mut r = req("POST", "/docs/doc_1");
    r.params.insert("id".into(), "doc_1".into());
    r.headers.push(("authorization".into(), "Bearer t".into()));
    r.headers
        .push(("content-type".into(), "application/json".into()));
    r.content_type = Some("application/json".into());
    r.raw_body = br#"{"callback_url": 5}"#.to_vec();
    r.body = json!({"callback_url": 5});
    let issues = ce.validate(&r);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].field, "body.callback_url");

    r.body = json!({"callback_url": "http://localhost:3000/hook"});
    assert!(ce.validate(&r).is_empty());
}

#[test]
fn auth_checks() {
    let ws = {
        let mut s = service("s", "/s", vec![]);
        s.auth = AuthConfig::Bearer {
            token: Some("secret".into()),
        };
        let mut b = service("b", "/b", vec![]);
        b.auth = AuthConfig::Basic {
            username: Some("u".into()),
            password: Some("p".into()),
        };
        let mut k = service("k", "/k", vec![]);
        k.auth = AuthConfig::ApiKey {
            header: "X-Api-Key".into(),
            value: None,
        };
        CompiledWorkspace::compile(Workspace::new("w", "W"), vec![s, b, k])
    };
    let mut r = req("GET", "/");
    assert!(ws.service("s").unwrap().check_auth(&r).is_err());
    r.headers
        .push(("authorization".into(), "Bearer secret".into()));
    assert!(ws.service("s").unwrap().check_auth(&r).is_ok());

    let mut r = req("GET", "/");
    r.headers
        .push(("authorization".into(), "Basic dTpw".into())); // u:p
    assert!(ws.service("b").unwrap().check_auth(&r).is_ok());

    let mut r = req("GET", "/");
    assert!(ws.service("k").unwrap().check_auth(&r).is_err());
    r.headers.push(("x-api-key".into(), "anything".into()));
    assert!(ws.service("k").unwrap().check_auth(&r).is_ok());
}
