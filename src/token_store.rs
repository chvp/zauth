use chrono::{DateTime, Local, Duration};
use std::collections::HashMap;
use rand::distributions::{Alphanumeric};
use rand::{thread_rng, Rng};
use std::sync::Mutex;

use models::*;


const TOKEN_VALIDITY_SECONDS : i64 = 3600;
const TOKEN_LENGTH : usize = 32;

#[derive(Debug)]
struct Token {
    token_str : String,
    username : String,
    client_id : String,
    redirect_uri : String,
    expiry    : DateTime<Local>
}

#[derive(Debug)]
pub struct TokenStore {
    tokens : Mutex<HashMap<String, Token>>
}

impl TokenStore {
    pub fn new() -> TokenStore {
        TokenStore {
            tokens : Mutex::new(HashMap::new())
        }
    }

    fn generate_random_token() -> String {
        thread_rng().sample_iter(&Alphanumeric).take(TOKEN_LENGTH).collect()
    }

    fn remove_expired_tokens(tokens : &mut HashMap<String, Token>) {
        let now = Local::now();
        tokens.retain(|_key, token| now < token.expiry);
    }

    pub fn create_token(&self, client_id : &String, user : &User, redirect_uri : &String) -> String {
        let tokens : &mut HashMap<String, Token> = &mut self.tokens.lock().unwrap();

        Self::remove_expired_tokens(tokens);

        let mut token_str = Self::generate_random_token();
        let mut token = Token {
            token_str : token_str.clone(),
            redirect_uri : redirect_uri.clone(),
            username : user.username().clone(),
            client_id : client_id.clone(),
            expiry : Local::now() + Duration::seconds(TOKEN_VALIDITY_SECONDS)
        };
        while tokens.contains_key(&token_str) {
            token_str = Self::generate_random_token();
            token.token_str = token_str.clone();
        }
        tokens.insert(token_str.clone(), token);
        return token_str
    }

    pub fn fetch_token_username(&self, client : &Client, redirect_uri : String, token_str : String)
        -> Result<String, &'static str> {
        let tokens : &mut HashMap<String, Token> = &mut self.tokens.lock().unwrap();
        Self::remove_expired_tokens(tokens);
        tokens.remove(&token_str)
            .ok_or("Token not in use")
            .and_then(|token| if token.redirect_uri == redirect_uri {
                Ok(token)
            } else {
                Err("Redirect URI mismatch")
            }).and_then(|token| if &token.client_id == client.id() {
                Ok(token.username)
            } else {
                Err("Client ID mismatch")
            })
    }
}
