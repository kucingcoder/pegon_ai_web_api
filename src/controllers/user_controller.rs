use chrono::NaiveDate;
use rocket::form::{Form, FromForm};
use rocket::fs::TempFile;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use rocket::{State, get, post};
use sea_orm::*;
use std::env;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::middleware::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::{GenderEnum, StatusPremiumEnum};
use crate::models::{prelude::*, users};

fn make_full_url(path: &str) -> String {
    let app_url = env::var("APP_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    format!("{}/images/{}", app_url, path)
}

#[get("/profile")]
pub async fn get_profile(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Json<users::Model>, Status> {
    let db = db as &DatabaseConnection;

    let user = Users::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| Status::InternalServerError)?;

    match user_opt {
        Some(mut user) => {
            if let Some(ref path) = user.photo_profile {
                user.photo_profile = Some(make_full_url(path));
            }
            Ok(Json(user))
        }
        None => Err(Status::NotFound),
    }
}

#[derive(FromForm)]
pub struct UpdateProfileRequest<'r> {
    pub full_name: String,
    pub gender: String,
    pub date_of_birth: String,
    pub photo: Option<TempFile<'r>>,
}

#[post("/profile/update", data = "<form>")]
pub async fn update_profile(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    mut form: Form<UpdateProfileRequest<'_>>,
) -> Result<Json<users::Model>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let user_model = Users::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    let mut user: users::ActiveModel = user_model.into();
    user.full_name = Set(form.full_name.clone());
    let gender_enum = match form.gender.as_str() {
        "male" | "Laki-laki" => GenderEnum::Male,
        _ => GenderEnum::Female,
    };
    user.gender = Set(gender_enum);

    let dob = NaiveDate::parse_from_str(&form.date_of_birth, "%Y-%m-%d").map_err(|_| {
        (
            Status::BadRequest,
            "Format tanggal salah. Gunakan YYYY-MM-DD".to_string(),
        )
    })?;
    user.date_of_birth = Set(dob);

    if let Some(ref mut file) = form.photo {
        let file_len = file.len();
        if file_len > 2 * 1024 * 1024 {
            return Err((Status::PayloadTooLarge, "Maksimal 2MB".to_string()));
        }

        let content_type = file
            .content_type()
            .ok_or((Status::BadRequest, "Tipe file error".to_string()))?;
        let ext = if content_type.is_jpeg() {
            "jpg"
        } else if content_type.is_png() {
            "png"
        } else {
            return Err((Status::BadRequest, "Hanya JPG/PNG".to_string()));
        };

        let upload_dir = "images/photo_profiles";
        if !Path::new(upload_dir).exists() {
            fs::create_dir_all(upload_dir)
                .map_err(|_| (Status::InternalServerError, "Gagal buat folder".to_string()))?;
        }

        let filename = format!("{}.{}", Uuid::new_v4(), ext);
        let save_path = Path::new(upload_dir).join(&filename);

        file.persist_to(&save_path)
            .await
            .map_err(|_| (Status::InternalServerError, "Gagal simpan file".to_string()))?;
        user.photo_profile = Set(Some(format!("photo_profiles/{}", filename)));
    }

    let mut updated_user = user
        .update(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Gagal update DB".to_string()))?;

    if let Some(ref path) = updated_user.photo_profile {
        updated_user.photo_profile = Some(make_full_url(path));
    }

    Ok(Json(updated_user))
}
