use chrono::NaiveDate;
use reqwest::Client;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::serde::json::Json;
use rocket::{State, post};
use sea_orm::*;
use std::env;
use uuid::Uuid;

use crate::controllers::auth_structs::{GoogleLoginRequest, GoogleTokenPayload};
use crate::models::sea_orm_active_enums::{GenderEnum, StatusPremiumEnum};
use crate::models::{prelude::*, users};

#[post("/auth/google", data = "<data>")]
pub async fn google_login(
    db: &State<DatabaseConnection>,
    cookies: &CookieJar<'_>,
    data: Json<GoogleLoginRequest>,
) -> Result<Json<users::Model>, Status> {
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
        return Err(Status::Unauthorized);
    }

    let payload: GoogleTokenPayload = resp.json().await.map_err(|_| Status::InternalServerError)?;

    let my_client_id = env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
    if payload.aud != my_client_id {
        println!("Attack Attempt! Token audience mismatch.");
        return Err(Status::Forbidden);
    }

    let user_opt = Users::find()
        .filter(users::Column::Email.eq(&payload.email))
        .one(db)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let user = match user_opt {
        Some(existing_user) => existing_user,
        None => {
            let new_id = Uuid::new_v4();
            let new_user = users::ActiveModel {
                id: Set(new_id),
                email: Set(payload.email),
                full_name: Set(payload.name),
                photo_profile: Set(payload.picture),
                gender: Set(GenderEnum::Male),
                date_of_birth: Set(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
                status: Set(StatusPremiumEnum::Standard),
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
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookies.add_private(cookie);
    Ok(Json(user))
}

#[post("/auth/logout")]
pub async fn logout(cookies: &CookieJar<'_>) -> Status {
    cookies.remove_private(Cookie::from("user_id"));
    Status::Ok
}
