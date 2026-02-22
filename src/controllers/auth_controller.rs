use chrono::NaiveDate;
use reqwest::Client;
use rocket::form::Form;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::request::FlashMessage;
use rocket::response::{Flash, Redirect};
use rocket::serde::json::{Json, serde_json::json};
use rocket::{State, post};
use rocket_dyn_templates::Template;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

use crate::models::sea_orm_active_enums::{Category, Gender};
use crate::models::user_model;

// Request dari add in
#[derive(FromForm)]
pub struct LoginRequest {
    pub add_in_code: String,
}

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
    client: &State<Client>,
    cookies: &CookieJar<'_>,
    data: Json<GoogleLoginRequest>,
) -> Result<Status, Status> {
    let db = db as &DatabaseConnection;
    // Client reused from state
    
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

    let mut cookie = Cookie::new("session", user.id.to_string());
    cookie.set_secure(false);
    cookie.set_http_only(true);
    cookie.make_permanent();
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookies.add_private(cookie);
    Ok(Status::Ok)
}

#[get("/login/add-in-auth-view")]
pub async fn login_add_in_auth_view(flash: Option<FlashMessage<'_>>) -> Template {
    let error_message = flash.map(|msg| msg.message().to_string());
    Template::render(
        "addin-login",
        json!({
            "error": error_message
        }),
    )
}

#[post("/login/add-in-auth-handle", data = "<form>")]
pub async fn login_add_in_auth_handle(
    db: &State<DatabaseConnection>,
    cookies: &CookieJar<'_>,
    form: Form<LoginRequest>,
) -> Result<Redirect, Flash<Redirect>> {
    let login_url = "/add-in/login/add-in-auth-view";
    let db = db as &DatabaseConnection;
    let query_result = user_model::Entity::find()
        .filter(user_model::Column::AddInCode.eq(&form.add_in_code))
        .select_only()
        .column(user_model::Column::Id)
        .column(user_model::Column::Category)
        .into_tuple::<(Uuid, Category)>()
        .one(db)
        .await;

    let (user_id, user_category) = match query_result {
        Ok(Some((id, category_str))) => (id, category_str),
        Ok(None) => {
            return Err(Flash::error(
                Redirect::to(login_url),
                "Kode Add-in Tidak Valid",
            ));
        }

        Err(e) => {
            return Err(Flash::error(
                Redirect::to(login_url),
                format!("Database Error: {}", e),
            ));
        }
    };

    if user_category != Category::Premium {
        return Err(Flash::error(
            Redirect::to(login_url),
            "Fitur ini memerlukan akun premium",
        ));
    }

    let mut cookie = Cookie::new("session", user_id.to_string());
    cookie.set_secure(false);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.make_permanent();
    cookies.add_private(cookie);

    Ok(Redirect::to(
        "/add-in/transliteration/add-in-transliterate-view",
    ))
}

#[get("/logout")]
pub async fn logout(cookies: &CookieJar<'_>) -> Status {
    cookies.remove_private(Cookie::from("session"));
    Status::NoContent
}

#[get("/logout/add-in")]
pub async fn logout_add_in(cookies: &CookieJar<'_>) -> Result<Redirect, Flash<Redirect>> {
    cookies.remove_private(Cookie::from("session"));
    Ok(Redirect::to("/add-in/login/add-in-auth-view"))
}
