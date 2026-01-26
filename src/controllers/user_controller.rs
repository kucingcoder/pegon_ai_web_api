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
use crate::models::{
    image_transliteration_model, learn_model, text_transliteration_model, user_model,
};

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

    let user = user_model::Entity::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    match user {
        Some(user) => Ok(Json(json!({
            "full_name": user.full_name,
            "gender": user.gender,
            "date_of_birth": user.date_of_birth,
            "add_in_code": user.add_in_code,
            "photo_profile": user.photo_profile,
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

    // Definisikan Future untuk query yang independen (bisa jalan bareng)
    
    // 1. Future User
    let user_future = user_model::Entity::find_by_id(auth.id).one(db);

    // 2. Future Text Count
    let text_count_future = text_transliteration_model::Entity::find()
        .filter(text_transliteration_model::Column::IdUser.eq(auth.id))
        .count(db);

    // 3. Future Image Count
    let image_count_future = image_transliteration_model::Entity::find()
        .filter(image_transliteration_model::Column::IdUser.eq(auth.id))
        .count(db);

    // 4. Future Image History
    let history_future = image_transliteration_model::Entity::find()
        .filter(image_transliteration_model::Column::IdUser.eq(auth.id))
        .order_by_desc(image_transliteration_model::Column::CreatedAt)
        .limit(5)
        .select_only()
        .column(image_transliteration_model::Column::Id)
        .column(image_transliteration_model::Column::Title)
        .column(image_transliteration_model::Column::Image)
        .column(image_transliteration_model::Column::CreatedAt)
        .column_as(
            Expr::expr(
                Func::cust("SUBSTRING")
                    .arg(Expr::col(image_transliteration_model::Column::Result))
                    .arg(1)
                    .arg(100),
            ),
            "result",
        )
        .into_json()
        .all(db);

    // EKSEKUSI PARALLEL
    let (user_res, text_count_res, image_count_res, history_res) =
        rocket::tokio::join!(user_future, text_count_future, image_count_future, history_future);

    // Handle Hasil
    let user_model = user_res
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    let text_transliteration_count =
        text_count_res.map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    let image_transliteration_count =
        image_count_res.map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    let image_transliterations =
        history_res.map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    // max stage level (Query ini bergantung pada user_model, jadi harus setelah user ketemu)
    let max_stage = learn_model::Entity::find()
        .filter(learn_model::Column::Level.eq(user_model.learning_level))
        .select_only()
        .column(learn_model::Column::MaxStage)
        .into_tuple::<i32>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
        .ok_or((
            Status::NotFound,
            "Konfigurasi level tidak ditemukan".to_string(),
        ))?;

    Ok(Json(json!({
        "user": {
            "full_name": user_model.full_name,
            "gender": user_model.gender,
            "date_of_birth": user_model.date_of_birth,
            "photo_profile": user_model.photo_profile,
            "learning_level": user_model.learning_level,
            "learning_stage_level": user_model.learning_stage_level,
            "learning_stage_max": max_stage,
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

    let user_model = user_model::Entity::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    let mut user: user_model::ActiveModel = user_model.into();
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
