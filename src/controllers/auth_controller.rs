use chrono::NaiveDate;
use reqwest::Client;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, post};
use rocket_dyn_templates::Template;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::env;

use crate::models::sea_orm_active_enums::{Category, Gender};
use crate::models::user_model;

// Request dari app
#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleLoginRequest {
    pub id_token: String,
}

// Response dari Google API
// Saat backend kita tanya ke Google: "Token ini punya siapa?", Google balas JSON ini.
// Kita cuma butuh ambil field penting saja.
#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleTokenPayload {
    pub aud: String,             // Audience (Harus cocok dengan Client ID kita)
    pub email: String,           // Email user
    pub name: String,            // Nama Lengkap
    pub picture: Option<String>, // Foto Profil (Bisa ada bisa nggak)
}

#[post("/login", data = "<data>")]
pub async fn login(
    db: &State<DatabaseConnection>,
    cookies: &CookieJar<'_>,
    data: Json<GoogleLoginRequest>,
) -> Result<Status, Status> {
    let db = db as &DatabaseConnection;
    let client = Client::new();
    let google_validation_url = "https://oauth2.googleapis.com/tokeninfo";
    let resp = client
        .get(google_validation_url)
        .query(&[("id_token", &data.id_token)])
        .send()
        .await
        .map_err(|_| Status::InternalServerError)?;

    if !resp.status().is_success() {
        println!("Attack Attempt! Google token validation failed.",);
        return Err(Status::Unauthorized);
    }

    let payload: GoogleTokenPayload = resp.json().await.map_err(|_| Status::InternalServerError)?;
    let my_client_id = env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
    if payload.aud != my_client_id {
        println!("Attack Attempt! Token audience mismatch.");
        return Err(Status::Forbidden);
    }

    let user_opt = user_model::Entity::find()
        .filter(user_model::Column::Email.eq(&payload.email))
        .one(db)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let user = match user_opt {
        Some(existing_user) => existing_user,
        None => {
            let new_user = user_model::ActiveModel {
                email: Set(payload.email),
                full_name: Set(payload.name),
                photo_profile: Set(payload.picture),
                gender: Set(Gender::Male),
                date_of_birth: Set(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
                category: Set(Category::Standard),
                learning_level: Set(1),
                learning_stage_level: Set(1),
                ..Default::default()
            };

            new_user.insert(db).await.map_err(|e| {
                println!("Db Error: {}", e);
                Status::InternalServerError
            })?
        }
    };

    let mut cookie = Cookie::new("user_id", user.id.to_string());
    cookie.set_secure(false);
    cookie.set_http_only(true);
    cookie.make_permanent();
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookies.add_private(cookie);
    Ok(Status::Ok)
}

#[get("/login/add-in-auth-view")]
pub async fn login_add_in_auth_view() -> Template {
    Template::render("addin-login", json!({}))
}

#[post("/login/add-in-auth-handle", data = "<data>")]
pub async fn login_add_in_auth_handle(
    db: &State<DatabaseConnection>,
    cookies: &CookieJar<'_>,
    data: Json<GoogleLoginRequest>,
) -> (Status, Json<Value>) {
    let db = db as &DatabaseConnection;
    let client = Client::new();

    let google_validation_url = "https://oauth2.googleapis.com/tokeninfo";
    let resp = match client
        .get(google_validation_url)
        .query(&[("id_token", &data.id_token)])
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            return (
                Status::InternalServerError,
                Json(json!({
                    "status": "error",
                    "message": "Gagal koneksi ke server Google"
                })),
            );
        }
    };

    if !resp.status().is_success() {
        return (
            Status::Unauthorized,
            Json(json!({
                "status": "error",
                "message": "Token Google tidak valid"
            })),
        );
    }

    let payload: GoogleTokenPayload = match resp.json().await {
        Ok(p) => p,
        Err(_) => {
            return (
                Status::InternalServerError,
                Json(json!({
                    "status": "error",
                    "message": "Gagal memproses data dari Google"
                })),
            );
        }
    };

    let my_client_id = env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
    if payload.aud != my_client_id {
        return (
            Status::Forbidden,
            Json(json!({
                "status": "error",
                "message": "Client ID tidak cocok (Potensi serangan)"
            })),
        );
    }

    let user_opt = match user_model::Entity::find()
        .filter(user_model::Column::Email.eq(&payload.email))
        .one(db)
        .await
    {
        Ok(opt) => opt,
        Err(e) => {
            return (
                Status::InternalServerError,
                Json(json!({
                    "status": "error",
                    "message": format!("Database Error: {}", e)
                })),
            );
        }
    };

    let user = match user_opt {
        Some(existing_user) => {
            if existing_user.category != Category::Premium {
                return (
                    Status::Forbidden,
                    Json(json!({
                        "status": "error",
                        "message": "Fitur ini memerlukan akun premium"
                    })),
                );
            }
            existing_user
        }
        None => {
            return (
                Status::Forbidden,
                Json(json!({
                    "status": "error",
                    "message": "Akun tidak ditemukan. Silakan daftar dan upgrade ke Premium via Aplikasi Mobile."
                })),
            );
        }
    };

    let mut cookie = Cookie::new("user_id", user.id.to_string());
    cookie.set_secure(false);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.make_permanent();
    cookies.add_private(cookie);

    (
        Status::Ok,
        Json(json!({
            "status": "success",
            "message": "Login berhasil",
        })),
    )
}

#[get("/logout")]
pub async fn logout(cookies: &CookieJar<'_>) -> Status {
    cookies.remove_private(Cookie::from("user_id"));
    Status::NoContent
}
