#![feature(async_closure)]

extern crate diesel;
extern crate rocket;

use rocket::http::Accept;
use rocket::http::ContentType;
use rocket::http::Status;

mod common;

use crate::common::url;
use zauth::models::client::{Client, NewClient};

#[rocket::async_test]
async fn create_and_update_client() {
	common::as_admin(async move |http_client, _db, _user| {
		let client_name = "test";

		let client_form = format!("name={}", url(&client_name),);

		let response = http_client
			.post("/clients")
			.body(client_form)
			.header(ContentType::Form)
			.header(Accept::JSON)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Created);
	})
	.await;
}

#[rocket::async_test]
async fn change_client_secret() {
	common::as_admin(async move |http_client, db, _user| {
		let client = Client::create(
			NewClient {
				name: "test".to_string(),
			},
			&db,
		)
		.await
		.expect("create client");

		let secret_pre = client.secret.clone();
		assert!(secret_pre.len() > 5);

		let response = http_client
			.post(format!("/clients/{}/generate_secret", &client.id))
			.header(Accept::JSON)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::NoContent);

		let client = client.reload(&db).await.expect("reload client");
		assert_ne!(secret_pre, client.secret);
	})
	.await;
}
