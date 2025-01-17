#![feature(async_closure)]

extern crate diesel;
extern crate rocket;

use rocket::http::{Accept, ContentType, Status};

use pwhash::bcrypt;
use zauth::models::user::*;

mod common;

#[rocket::async_test]
async fn get_all_users() {
	common::as_visitor(async move |http_client, _db| {
		let response = http_client.get("/users").dispatch().await;
		assert_eq!(response.status(), Status::Unauthorized);
	})
	.await;

	common::as_user(async move |http_client, _db, _user| {
		let response = http_client.get("/users").dispatch().await;
		assert_eq!(response.status(), Status::Forbidden);
	})
	.await;

	common::as_admin(async move |http_client, _db, _admin| {
		let response = http_client.get("/users").dispatch().await;

		assert_eq!(response.status(), Status::Ok);
	})
	.await;
}

#[rocket::async_test]
async fn show_user_as_visitor() {
	common::as_visitor(async move |http_client, _db| {
		let response = http_client.get("/users/1").dispatch().await;
		assert_eq!(
			response.status(),
			Status::Unauthorized,
			"visitor should get unauthrorized"
		);
	})
	.await;
}

#[rocket::async_test]
async fn show_user_as_user() {
	common::as_user(async move |http_client, db, user| {
		let other = User::create(
			NewUser {
				username:  String::from("somebody"),
				password:  String::from("once told me"),
				full_name: String::from("zeus"),
				email:     String::from("would@be.forever"),
				ssh_key:   Some(String::from("ssh-rsa nananananananaaa")),
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = http_client
			.get(format!("/users/{}", other.id))
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::NotFound,
			"should not be able to see other user's profile"
		);

		let response = http_client
			.get(format!("/users/{}", user.id))
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"should be able to see own profile"
		);
	})
	.await;
}

#[rocket::async_test]
async fn show_user_as_admin() {
	common::as_admin(async move |http_client, db, admin| {
		let other = User::create(
			NewUser {
				username:  String::from("somebody"),
				password:  String::from("once told me"),
				full_name: String::from("zeus"),
				email:     String::from("would@be.forever"),
				ssh_key:   Some(String::from("ssh-rsa nananananananaaa")),
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = http_client
			.get(format!("/users/{}", other.id))
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"admin should see other's profile"
		);

		let response = http_client
			.get(format!("/users/{}", admin.id))
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"admin should see own profile"
		);
	})
	.await;
}

#[rocket::async_test]
async fn update_self() {
	common::as_user(async move |http_client, db, user| {
		let response = http_client
			.put(format!("/users/{}", user.id))
			.header(ContentType::Form)
			.header(Accept::JSON)
			.body("username=newusername")
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::NoContent,
			"user should be able to edit themself"
		);

		let updated = User::find(user.id, &db).await.unwrap();

		assert_eq!("newusername", updated.username);

		let other = User::create(
			NewUser {
				username:  String::from("somebody"),
				password:  String::from("once told me"),
				full_name: String::from("zeus"),
				email:     String::from("would@be.forever"),
				ssh_key:   Some(String::from("ssh-rsa nananananananaaa")),
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = http_client
			.put(format!("/users/{}", other.id))
			.header(ContentType::Form)
			.header(Accept::JSON)
			.body("username=newusername")
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Forbidden,
			"user should not be able to edit others"
		);
	})
	.await;
}

#[rocket::async_test]
async fn change_password() {
	common::as_user(async move |http_client, db, user| {
		let response = http_client
			.put(format!("/users/{}", user.id))
			.header(ContentType::Form)
			.header(Accept::JSON)
			.body("password=newpassword")
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::NoContent,
			"user should be able to change password"
		);

		let updated = User::find(user.id, &db).await.unwrap();

		assert_ne!(
			user.hashed_password, updated.hashed_password,
			"password should have changed"
		);
	})
	.await;
}

#[rocket::async_test]
async fn make_admin() {
	common::as_admin(async move |http_client, db, _admin| {
		let other = User::create(
			NewUser {
				username:  String::from("somebody"),
				password:  String::from("once told me"),
				full_name: String::from("zeus"),
				email:     String::from("would@be.forever"),
				ssh_key:   Some(String::from("ssh-rsa nananananananaaa")),
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = http_client
			.post(format!("/users/{}/admin", other.id))
			.header(ContentType::Form)
			.header(Accept::JSON)
			.body("admin=true")
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::NoContent,
			"admin should be able to make other admin"
		);

		let updated = User::find(other.id, &db).await.unwrap();

		assert!(updated.admin, "other user should be admin now");
	})
	.await;
}

#[rocket::async_test]
async fn try_make_admin() {
	common::as_user(async move |http_client, db, _user| {
		let other = User::create(
			NewUser {
				username:  String::from("somebody"),
				password:  String::from("once told me"),
				full_name: String::from("zeus"),
				email:     String::from("would@be.forever"),
				ssh_key:   Some(String::from("ssh-rsa nananananananaaa")),
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = http_client
			.post(format!("/users/{}/admin", other.id))
			.header(ContentType::Form)
			.header(Accept::JSON)
			.body("admin=true")
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Forbidden,
			"user should not be able to make other admin"
		);
	})
	.await;
}

#[rocket::async_test]
async fn create_user_form() {
	common::as_admin(async move |http_client, db, _admin| {
		let user_count = User::all(&db).await.unwrap().len();

		let response = http_client
			.post("/users")
			.header(ContentType::Form)
			.header(Accept::JSON)
			.body(
				"username=testuser&password=testpassword&full_name=abc&\
				 email=hij@klm.op&ssh_key=ssh-rsa%20base64%3D%3D%20user@\
				 hostname",
			)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Ok);

		assert_eq!(user_count + 1, User::all(&db).await.unwrap().len());

		let last_created = User::last(&db).await.unwrap();
		assert_eq!("testuser", last_created.username);
	})
	.await;
}

#[rocket::async_test]
async fn create_user_json() {
	common::as_admin(async move |http_client, db, _admin| {
		let user_count = User::all(&db).await.unwrap().len();

		let response = http_client
			.post("/users")
			.header(ContentType::JSON)
			.header(Accept::JSON)
			.body(
				"{\"username\": \"testuser\", \"password\": \"testpassword\", \
				 \"full_name\": \"abc\", \"email\": \"hij@klm.op\", \
				 \"ssh_key\": \"ssh-rsa qrs tuv@wxyz\"}",
			)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Ok);

		assert_eq!(user_count + 1, User::all(&db).await.unwrap().len());

		let last_created = User::last(&db).await.unwrap();
		assert_eq!("testuser", last_created.username);
	})
	.await;
}

#[rocket::async_test]
async fn forgot_password() {
	common::as_visitor(async move |http_client, db| {
		let email = String::from("test@example.com");
		let user = User::create(
			NewUser {
				username:  String::from("user"),
				password:  String::from("password"),
				full_name: String::from("name"),
				email:     email.clone(),
				ssh_key:   None,
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		assert!(user.password_reset_token.is_none());
		assert!(user.password_reset_expiry.is_none());

		let response = http_client
			.get("/users/forgot_password")
			.header(Accept::HTML)
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"should get forgot password page"
		);

		let response = common::expect_mail_to(vec![&email], async || {
			http_client
				.post("/users/forgot_password")
				.header(ContentType::Form)
				.header(Accept::HTML)
				.body(format!("for_email={}", &email))
				.dispatch()
				.await
		})
		.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"should post email to forgot password"
		);

		let user = user.reload(&db).await.unwrap();

		assert!(user.password_reset_token.is_some());
		assert!(user.password_reset_expiry.is_some());

		let token = user.password_reset_token.clone().unwrap();

		let response = http_client
			.get(format!("/users/reset_password/{}", token,))
			.header(Accept::HTML)
			.dispatch()
			.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"should get reset password page"
		);

		let old_password_hash = user.hashed_password.clone();
		let new_password = "passw0rd";

		dbg!(&user);

		let response = common::expect_mail_to(vec![&email], async || {
			http_client
				.post(format!("/users/reset_password/"))
				.header(ContentType::Form)
				.header(Accept::HTML)
				.body(format!(
					"token={}&new_password={}",
					&token, &new_password
				))
				.dispatch()
				.await
		})
		.await;

		dbg!(&user);

		assert_eq!(
			response.status(),
			Status::Ok,
			"should post to reset password page"
		);

		let user = user.reload(&db).await.unwrap();

		assert!(user.password_reset_token.is_none());
		assert!(user.password_reset_expiry.is_none());
		assert_ne!(user.hashed_password, old_password_hash);
		assert!(bcrypt::verify(new_password, &user.hashed_password));
	})
	.await;
}

#[rocket::async_test]
async fn forgot_password_non_existing_email() {
	common::as_visitor(async move |http_client, db| {
		let email = String::from("test@example.com");
		let _user = User::create(
			NewUser {
				username:  String::from("user"),
				password:  String::from("password"),
				full_name: String::from("name"),
				email:     email.clone(),
				ssh_key:   None,
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = common::dont_expect_mail(async || {
			http_client
				.post("/users/forgot_password")
				.header(ContentType::Form)
				.header(Accept::HTML)
				.body("for_email=not_this_email@example.com")
				.dispatch()
				.await
		})
		.await;

		assert_eq!(
			response.status(),
			Status::Ok,
			"should still say everything is OK, even when email does not exist"
		);
	})
	.await;
}

#[rocket::async_test]
async fn reset_password_invalid_token() {
	common::as_visitor(async move |http_client, db| {
		let email = String::from("test@example.com");
		let user = User::create(
			NewUser {
				username:  String::from("user"),
				password:  String::from("password"),
				full_name: String::from("name"),
				email:     email.clone(),
				ssh_key:   None,
			},
			common::BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		let response = http_client
			.post("/users/forgot_password")
			.header(ContentType::Form)
			.header(Accept::HTML)
			.body(format!("for_email={}", &email))
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Ok);

		let user = user.reload(&db).await.unwrap();
		let token = user.password_reset_token.clone().unwrap();
		let old_hash = user.hashed_password.clone();

		let response = common::dont_expect_mail(async || {
			http_client
				.post("/users/reset_password/")
				.header(ContentType::Form)
				.header(Accept::HTML)
				.body(format!(
					"token=not{}&new_password={}",
					&token, "passw0rd"
				))
				.dispatch()
				.await
		})
		.await;

		assert_eq!(response.status(), Status::Forbidden);

		let user = user.reload(&db).await.unwrap();
		assert_eq!(user.hashed_password, old_hash);
	})
	.await;
}

#[rocket::async_test]
async fn register_user() {
	common::as_visitor(async move |http_client, db| {
		let response = http_client
			.get("/register")
			.header(Accept::HTML)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Ok);

		let username = "somebody";
		let password = "toucha    ";
		let full_name = "maa";
		let email = "spaghet@zeus.ugent.be";

		let response = http_client
			.post("/register")
			.header(Accept::HTML)
			.header(ContentType::Form)
			.body(format!(
				"username={}&password={}&full_name={}&email={}",
				username, password, full_name, email
			))
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Created);

		let user = User::find_by_username(username.to_string(), &db)
			.await
			.expect("user should be created");

		assert_eq!(
			user.state,
			UserState::PendingApproval,
			"registered users should be pending for approval"
		);
	})
	.await;
}

#[rocket::async_test]
async fn validate_on_registration() {
	common::as_visitor(async move |http_client, db| {
		let response = http_client
			.get("/register")
			.header(Accept::HTML)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Ok);

		let username = "somebody";
		let password = "toucha    ";
		let invalid_full_name = "?";
		let email = "spaghet@zeus.ugent.be";

		let user_count = User::all(&db).await.unwrap().len();

		let response = http_client
			.post("/register")
			.header(Accept::HTML)
			.header(ContentType::Form)
			.body(format!(
				"username={}&password={}&full_name={}&email={}",
				username, password, invalid_full_name, email
			))
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::UnprocessableEntity);
		assert_eq!(
			user_count,
			User::all(&db).await.unwrap().len(),
			"should not have created user"
		)
	})
	.await;
}

#[rocket::async_test]
async fn validate_on_admin_create() {
	common::as_visitor(async move |http_client, db| {
		let response = http_client
			.get("/register")
			.header(Accept::HTML)
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::Ok);

		let username = "somebody";
		let password = "toucha    ";
		let invalid_full_name = "?";
		let email = "spaghet@zeus.ugent.be";

		let user_count = User::all(&db).await.unwrap().len();

		let response = http_client
			.post("/register")
			.header(Accept::HTML)
			.header(ContentType::Form)
			.body(format!(
				"username={}&password={}&full_name={}&email={}",
				username, password, invalid_full_name, email
			))
			.dispatch()
			.await;

		assert_eq!(response.status(), Status::UnprocessableEntity);
		assert_eq!(
			user_count,
			User::all(&db).await.unwrap().len(),
			"should not have created user"
		)
	})
	.await;
}
