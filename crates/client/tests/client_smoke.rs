use nolgia_client::ClientBuilder;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn adds_bearer_token_and_targets_v1_me() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .and(header("authorization", "Bearer nol_test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "2f2f1a1d-7d1c-4d34-91fd-28a4d5e5d5e5",
            "email": "ada@nolgia.ai",
            "name": "Ada Lovelace",
            "image_url": null,
            "created_at": "2026-06-13T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let client = ClientBuilder::new(server.uri())
        .bearer_token("nol_test_token")
        .build()
        .expect("client builds");

    let user = client
        .get_current_user()
        .send()
        .await
        .expect("request succeeds")
        .into_inner();

    assert_eq!(user.email, "ada@nolgia.ai");
    assert_eq!(user.name.as_deref(), Some("Ada Lovelace"));

    server.verify().await;
}
