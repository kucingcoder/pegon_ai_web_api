use chrono::NaiveDate;
use rocket::form::{Form, FromForm};
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, get, patch};
use sea_orm::sea_query::{Expr, Func};
use sea_orm::*;
use std::env;
use std::path::Path;
use uuid::Uuid;

use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::Gender;
use crate::models::{image_transliterations, text_transliterations, users};

fn make_full_url(path: &str) -> String {
    let app_url = env::var("APP_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    format!("{}/images/{}", app_url, path)
}

#[get("/profile")]
pub async fn get_profile(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let user = users::Entity::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    match user {
        Some(user) => Ok(Json(json!({
            "full_name": user.full_name,
            "gender": user.gender,
            "date_of_birth": user.date_of_birth,
            "photo_profile": user.photo_profile,
            "learning_level": user.learning_level,
            "learning_stage_level": user.learning_stage_level,
            "category": user.category,
            "created_at": user.created_at,
            "expired_at": user.expired_at
        }))),
        None => Err((Status::NotFound, "User not found".to_string())),
    }
}

#[get("/profile/detail")]
pub async fn get_profile_detail(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // data diri user
    let user_model = users::Entity::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // data statistik text transliteration
    let text_transliteration_count = text_transliterations::Entity::find()
        .filter(text_transliterations::Column::IdUser.eq(auth.id))
        .count(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    // data statistik image transliteration
    let image_transliteration_count = image_transliterations::Entity::find()
        .filter(image_transliterations::Column::IdUser.eq(auth.id))
        .count(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    // Riwayat image transliteration
    let image_transliterations = image_transliterations::Entity::find()
        .filter(image_transliterations::Column::IdUser.eq(auth.id))
        .order_by_desc(image_transliterations::Column::CreatedAt)
        .limit(5)
        .select_only()
        .column(image_transliterations::Column::Id)
        .column(image_transliterations::Column::Title)
        .column(image_transliterations::Column::Image)
        .column(image_transliterations::Column::CreatedAt)
        .column_as(
            Expr::expr(
                Func::cust("SUBSTRING")
                    .arg(Expr::col(image_transliterations::Column::Result))
                    .arg(1)
                    .arg(100),
            ),
            "result",
        )
        .into_json()
        .all(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    Ok(Json(json!({
        "user": {
            "full_name": user_model.full_name,
            "gender": user_model.gender,
            "date_of_birth": user_model.date_of_birth,
            "photo_profile": user_model.photo_profile,
            "learning_level": user_model.learning_level,
            "learning_stage_level": user_model.learning_stage_level,
            "category": user_model.category,
            "created_at": user_model.created_at,
            "expired_at": user_model.expired_at
        },
        "text_transliteration_count": text_transliteration_count,
        "image_transliteration_count": image_transliteration_count,
        "image_transliterations": image_transliterations
    })))
}

#[derive(FromForm)]
pub struct UpdateProfileRequest<'r> {
    pub full_name: String,
    pub gender: String,
    pub date_of_birth: String,
    pub photo_profile: Option<TempFile<'r>>,
}

#[patch("/profile/update", data = "<form>")]
pub async fn update_profile(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    mut form: Form<UpdateProfileRequest<'_>>,
) -> Result<Status, (Status, String)> {
    let db = db as &DatabaseConnection;

    let user_model = users::Entity::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    let mut user: users::ActiveModel = user_model.into();
    user.full_name = Set(form.full_name.clone());
    let gender_enum = match form.gender.as_str() {
        "Male" | "male" => Gender::Male,
        _ => Gender::Female,
    };
    user.gender = Set(gender_enum);

    let dob = NaiveDate::parse_from_str(&form.date_of_birth, "%Y-%m-%d").map_err(|_| {
        (
            Status::BadRequest,
            "Format tanggal salah. Gunakan YYYY-MM-DD".to_string(),
        )
    })?;
    user.date_of_birth = Set(dob);

    if let Some(ref mut file) = form.photo_profile {
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
        let filename = format!("{}.{}", Uuid::new_v4(), ext);
        let save_path = Path::new(upload_dir).join(&filename);

        file.persist_to(&save_path)
            .await
            .map_err(|_| (Status::InternalServerError, "Gagal simpan file".to_string()))?;

        user.photo_profile = Set(Some(make_full_url(&format!("photo_profiles/{}", filename))));
    }

    user.update(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Gagal update DB".to_string()))?;

    Ok(Status::Ok)
}
