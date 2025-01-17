#![allow(dead_code)]

extern crate diesel;
extern crate parking_lot;
extern crate rocket;
extern crate tempfile;
extern crate urlencoding;
extern crate zauth;

use diesel::sql_query;
use diesel::RunQueryDsl;
use parking_lot::Mutex;
use std::str::FromStr;

use crate::common::zauth::models::client::*;
use crate::common::zauth::models::user::*;
use crate::common::zauth::DbConn;
use lettre::Address;
use rocket::http::{ContentType, Status};
use std::future::Future;
use std::thread;
use std::time::Duration;
use zauth::mailer::STUB_MAILER_OUTBOX;

type HttpClient = rocket::local::asynchronous::Client;

// Rocket doesn't support transactional testing yet, so we use a lock to
// serialize tests.
static DB_LOCK: Mutex<()> = Mutex::new(());

pub const BCRYPT_COST: u32 = 4;

pub fn url(content: &str) -> String {
	urlencoding::encode(content).into_owned()
}

async fn reset_db(db: &DbConn) {
	db.run(|conn| {
		assert!(sql_query("TRUNCATE TABLE users").execute(conn).is_ok());
		assert!(sql_query("TRUNCATE TABLE clients").execute(conn).is_ok());
	})
	.await
}

/// Creates a rocket::local::Client for testing purposes. The rocket instance
/// will be configured with a Sqlite database located in a tmpdir  This
/// executes the given function with the Client a connection to that
/// database. The database and its directory will be removed after the function
/// has run.
pub async fn as_visitor<F, R>(run: F)
where
	F: FnOnce(HttpClient, DbConn) -> R,
	R: Future<Output = ()>,
{
	// Prepare config
	let db_url = "postgresql://zauth:zauth@localhost/zauth_test";
	let config = rocket::Config::figment()
		.merge(("databases.postgresql_database.url", db_url));

	let _lock = DB_LOCK.lock();
	let client = HttpClient::tracked(zauth::prepare_custom(config))
		.await
		.expect("rocket client");

	let db = DbConn::get_one(client.rocket())
		.await
		.expect("database connection");
	reset_db(&db).await;
	assert_eq!(0, User::all(&db).await.unwrap().len());
	assert_eq!(0, Client::all(&db).await.unwrap().len());

	run(client, db).await;
}

pub async fn as_user<F, R>(run: F)
where
	F: FnOnce(HttpClient, DbConn, User) -> R,
	R: Future<Output = ()>,
{
	as_visitor(async move |client, db| {
		let user = User::create(
			NewUser {
				username:  String::from("username"),
				password:  String::from("password"),
				full_name: String::from("full"),
				email:     String::from("user@domain.tld"),
				ssh_key:   Some(String::from("ssh-rsa base64== key@hostname")),
			},
			BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		{
			let response = client
				.post("/login")
				.body("username=username&password=password")
				.header(ContentType::Form)
				.dispatch()
				.await;
			assert_eq!(response.status(), Status::SeeOther, "login failed");
		}

		run(client, db, user).await;
	})
	.await;
}

pub async fn as_admin<F, R>(run: F)
where
	F: FnOnce(HttpClient, DbConn, User) -> R,
	R: Future<Output = ()>,
{
	as_visitor(async move |client, db| {
		let mut user = User::create(
			NewUser {
				username:  String::from("admin"),
				password:  String::from("password"),
				full_name: String::from("admin name"),
				email:     String::from("admin@domain.tld"),
				ssh_key:   Some(String::from("ssh-rsa admin admin@hostname")),
			},
			BCRYPT_COST,
			&db,
		)
		.await
		.unwrap();

		user.admin = true;
		let user = user.update(&db).await.unwrap();

		{
			let response = client
				.post("/login")
				.body("username=admin&password=password")
				.header(ContentType::Form)
				.dispatch()
				.await;
			assert_eq!(response.status(), Status::SeeOther, "login failed");
		}

		run(client, db, user).await;
	})
	.await;
}

pub async fn dont_expect_mail<T, F, R>(run: F) -> T
where
	F: FnOnce() -> R,
	R: Future<Output = T>,
{
	let (mailbox, _condvar) = &STUB_MAILER_OUTBOX;
	let outbox_size = { mailbox.lock().len() };
	let result: T = run().await;
	thread::sleep(Duration::from_secs(1));
	assert_eq!(
		outbox_size,
		mailbox.lock().len(),
		"Expected no mail to be sent"
	);
	result
}

pub async fn expect_mail_to<T, F, R>(receivers: Vec<&str>, run: F) -> T
where
	F: FnOnce() -> R,
	R: Future<Output = T>,
{
	let (mailbox, condvar) = &STUB_MAILER_OUTBOX;
	let outbox_size = { mailbox.lock().len() };
	let result: T = run().await;

	let mut mailbox = mailbox.lock();
	if mailbox.len() == outbox_size {
		let wait_result =
			condvar.wait_for(&mut mailbox, Duration::from_secs(1));
		assert!(
			!wait_result.timed_out(),
			"Timed out while waiting for email"
		);
	}

	assert_eq!(
		mailbox.len(),
		outbox_size + 1,
		"Expected an email to be sent"
	);
	let last_mail = mailbox.last().unwrap();
	let receivers = receivers
		.into_iter()
		.map(|e| Address::from_str(e).unwrap())
		.collect::<Vec<Address>>();
	assert_eq!(last_mail.envelope().to(), receivers, "Unexpected receivers");
	result
}
