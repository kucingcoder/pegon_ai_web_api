use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::{learn, users};
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, post};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct CheckReadRequest {
    pub guess: String,
    pub real: String,
}

#[derive(Debug, Serialize)]
pub struct CheckReadResponse {
    pub success: bool,
    pub message: String,
    pub current_level: i32,
    pub current_stage_level: i32,
}

#[post("/check/read", data = "<data>")]
pub async fn check_read(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    data: Json<CheckReadRequest>,
) -> Result<Json<CheckReadResponse>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // VALIDASI: Trim whitespace & Case Insensitive
    if data.guess.trim().to_lowercase() != data.real.trim().to_lowercase() {
        return Err((
            Status::BadRequest,
            "Jawaban salah atau tidak cocok.".to_string(),
        ));
    }

    // STEP 1: Ambil Level & Stage User Saat Ini
    let (current_level, current_stage) = users::Entity::find_by_id(auth.id)
        .select_only()
        .column(users::Column::LearningLevel)
        .column(users::Column::LearningStageLevel)
        .into_tuple::<(i32, i32)>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // STEP 2: Ambil Max Stage dari Config (Tabel Learn)
    let max_stage = learn::Entity::find()
        .filter(learn::Column::Level.eq(current_level))
        .select_only()
        .column(learn::Column::MaxStage)
        .into_tuple::<i32>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
        .ok_or((
            Status::NotFound,
            "Konfigurasi level tidak ditemukan".to_string(),
        ))?;

    // STEP 3: Hitung Level/Stage Berikutnya (Logic Split)
    let (next_level, next_stage_level) = if current_stage >= max_stage {
        (current_level + 1, 1)
    } else {
        (current_level, current_stage + 1)
    };

    // STEP 4: Update Level/Stage User
    let user_update = users::ActiveModel {
        id: Set(auth.id),
        learning_level: Set(next_level),
        learning_stage_level: Set(next_stage_level),
        ..Default::default()
    };

    user_update
        .update(db)
        .await
        .map_err(|e| (Status::InternalServerError, format!("Update failed: {}", e)))?;

    // STEP 5: Return JSON Response
    Ok(Json(CheckReadResponse {
        success: true,
        message: "Jawaban benar! Progress disimpan.".to_string(),
        current_level: next_level,
        current_stage_level: next_stage_level,
    }))
}
