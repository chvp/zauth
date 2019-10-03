extern crate diesel;
extern crate regex;
extern crate rocket;
extern crate serde_json;
extern crate urlencoding;
extern crate zauth;

use self::serde_json::Value;
use regex::Regex;
use rocket::http::ContentType;
use rocket::http::Cookie;
use rocket::http::Header;
use rocket::http::Status;

use zauth::models::user::User;
use zauth::token_store::TokenStore;

mod common;

fn url(content: &str) -> String {
	urlencoding::encode(content)
}

fn get_param(param_name: &str, query: &String) -> Option<String> {
	Regex::new(&format!("{}=([^&]+)", param_name))
		.expect("valid regex")
		.captures(query)
		.map(|c| c[1].to_string())
}

#[test]
fn normal_flow() {
	let http_client = common::create_http_client();

	let redirect_uri = "https://example.com/redirect/me/here";
	let client_id = "test";
	let client_secret = "nananana";
	let client_state = "anarchy (╯°□°)╯ ┻━┻";
	let user_username = "batman";
	let user_password = "wolololo";

	// 1. User is redirected to OAuth server with request params given by
	// the client    The OAuth server should respond with a redirect to
	// the login page.
	let authorize_url = format!(
		"/oauth/authorize?response_type=code&redirect_uri={}&client_id={}&state={}",
		url(redirect_uri),
		url(client_id),
		url(client_state)
	);
	let response = http_client.get(authorize_url).dispatch();

	assert_eq!(response.status(), Status::SeeOther);
	let login_location = response
		.headers()
		.get_one("Location")
		.expect("Location header");
	dbg!(login_location);
	assert!(login_location.starts_with("/oauth/login"));

	// 2. User requests the login page
	let mut response = http_client.get(login_location).dispatch();

	assert_eq!(response.status(), Status::Ok);
	assert_eq!(response.content_type(), Some(ContentType::HTML));

	let state_regex =
		Regex::new("<input type=\"hidden\" name=\"state\" value=\"([^\"]+)\">")
			.unwrap();
	let body = response.body_string().expect("response body");
	let form_state = state_regex
		.captures(&body)
		.map(|c| c[1].to_string())
		.expect("hidden state field");

	// 3. User posts it credentials to the login path
	let login_url = "/oauth/login";
	let form_body = format!(
		"username={}&password={}&state={}&remember_me=on",
		url(user_username),
		url(user_password),
		form_state
	);

	let response = http_client
		.post(login_url)
		.body(form_body)
		.header(ContentType::Form)
		.dispatch();

	assert_eq!(response.status(), Status::SeeOther);
	let grant_location = response
		.headers()
		.get_one("Location")
		.expect("Location header");
	assert!(grant_location.starts_with("/oauth/grant"));
	let session_cookie_str = response
		.headers()
		.get_one("Set-Cookie")
		.expect("Session cookie")
		.to_owned();
	let cookie_regex = Regex::new("^([^=]+)=([^;]+).*").unwrap();
	let (cookie_name, cookie_content) = cookie_regex
		.captures(&session_cookie_str)
		.map(|c| (c[1].to_string(), urlencoding::decode(&c[2]).unwrap()))
		.expect("session cookie");

	// 4. User requests grant page
	let mut response = http_client
		.get(grant_location)
		.cookie(Cookie::new(
			cookie_name.to_string(),
			cookie_content.to_string(),
		))
		.dispatch();

	assert_eq!(response.status(), Status::Ok);
	assert_eq!(response.content_type(), Some(ContentType::HTML));

	let state_regex =
		Regex::new("<input type=\"hidden\" name=\"state\" value=\"([^\"]+)\">")
			.unwrap();
	let body = response.body_string().expect("response body");
	let form_state = state_regex
		.captures(&body)
		.map(|c| c[1].to_string())
		.expect("hidden state field");

	// 5. User posts to grant page
	let grant_url = "/oauth/grant";
	let grant_form_body = format!("state={}&grant=true", form_state);

	let response = http_client
		.post(grant_url)
		.body(grant_form_body.clone())
		.cookie(Cookie::new(
			cookie_name.to_string(),
			cookie_content.to_string(),
		))
		.header(ContentType::Form)
		.dispatch();

	assert_eq!(response.status(), Status::SeeOther);
	let redirect_location = response
		.headers()
		.get_one("Location")
		.expect("Location header");

	let redirect_uri_regex = Regex::new("^([^?]+)?(.*)$").unwrap();
	let (redirect_uri_base, redirect_uri_params) = redirect_uri_regex
		.captures(&redirect_location)
		.map(|c| (c[1].to_string(), c[2].to_string()))
		.unwrap();

	assert_eq!(redirect_uri_base, redirect_uri);

	let authorization_code =
		get_param("code", &redirect_uri_params).expect("authorization code");
	let state = get_param("state", &redirect_uri_params).expect("state");

	assert_eq!(
		client_state,
		urlencoding::decode(&state).expect("state decoded")
	);

	// 6a. Client requests access code while sending its credentials
	//     trough HTTP Auth.
	let token_url = "/oauth/token";
	let form_body = format!(
		"grant_type=authorization_code&code={}&redirect_uri={}",
		authorization_code, redirect_uri
	);

	let credentials =
		base64::encode(&format!("{}:{}", client_id, client_secret));

	let req = http_client
		.post(token_url)
		.header(ContentType::Form)
		.header(Header::new(
			"Authorization",
			format!("Basic {}", credentials),
		))
		.body(form_body);

	let mut response = req.dispatch();

	assert_eq!(response.status(), Status::Ok);
	assert_eq!(
		response.content_type().expect("content type"),
		ContentType::JSON
	);

	let response_body = response.body_string().expect("response body");
	let data: Value =
		serde_json::from_str(&response_body).expect("response json values");

	assert!(data["access_token"].is_string());
	assert!(data["token_type"].is_string());
	assert_eq!(data["token_type"], "???");

	// 6b. Client requests access code while sending its credentials
	//     trough the form body.

	// First, re-create a token
	let token_store = http_client
		.rocket()
		.state::<TokenStore>()
		.expect("should have token store");
	let db = common::db();
	let user = User::find(1, &db).expect("user");
	let authorization_code = token_store.create_token(
		&String::from(client_id),
		&user,
		&String::from(redirect_uri),
	);

	let token_url = "/oauth/token";
	let form_body = format!(
		"grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}",
		authorization_code, redirect_uri, client_id, client_secret
	);

	let req = http_client
		.post(token_url)
		.header(ContentType::Form)
		.body(form_body);

	let mut response = req.dispatch();

	assert_eq!(response.status(), Status::Ok);
	assert_eq!(
		response.content_type().expect("content type"),
		ContentType::JSON
	);

	let response_body = response.body_string().expect("response body");
	let data: Value =
		serde_json::from_str(&response_body).expect("response json values");

	dbg!(&data);
	assert!(data["access_token"].is_string());
	assert!(data["token_type"].is_string());
	assert_eq!(data["token_type"], "???");
}