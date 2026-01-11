use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::{learn_model, user_model};
use rocket::form::{Form, FromForm};
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, post};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct CheckReadRequest {
    pub guess: String,
    pub real: String,
}

#[derive(FromForm)]
pub struct CheckWriteRequest<'r> {
    pub image: TempFile<'r>,
    pub real_text: String,
}

#[derive(Debug, Serialize)]
pub struct CheckResponse {
    pub success: bool,
    pub message: String,
    pub current_level: i32,
    pub current_stage_level: i32,
}

#[get("/check/ping")]
pub async fn check_ping() -> Result<Status, (Status, String)> {
    Ok(Status::Ok)
}

#[get("/check/level-stage")]
pub async fn check_level_stage(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Json<CheckResponse>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let (current_level, current_stage) = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::LearningLevel)
        .column(user_model::Column::LearningStageLevel)
        .into_tuple::<(i32, i32)>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    Ok(Json(CheckResponse {
        success: true,
        message: "Success".to_string(),
        current_level,
        current_stage_level: current_stage,
    }))
}

#[post("/check/update-level-stage")]
pub async fn check_update_level_stage(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Json<CheckResponse>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // STEP 1: Ambil Level & Stage User Saat Ini
    let (current_level, current_stage) = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::LearningLevel)
        .column(user_model::Column::LearningStageLevel)
        .into_tuple::<(i32, i32)>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // STEP 2: Ambil Max Stage untuk Level user saat ini
    let max_stage_in_current_level = learn_model::Entity::find()
        .filter(learn_model::Column::Level.eq(current_level))
        .select_only()
        .column(learn_model::Column::MaxStage)
        .into_tuple::<i32>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
        .ok_or((
            Status::NotFound,
            format!("Konfigurasi untuk level {} tidak ditemukan", current_level),
        ))?;

    // Variabel mutables untuk next state
    let mut next_level = current_level;
    let mut next_stage_level = current_stage;
    let mut is_updated = false;

    if current_stage < max_stage_in_current_level {
        next_stage_level = current_stage + 1;
        is_updated = true;
    } else {
        let max_global_level = learn_model::Entity::find()
            .select_only()
            .column(learn_model::Column::Level)
            .order_by_desc(learn_model::Column::Level)
            .into_tuple::<i32>()
            .one(db)
            .await
            .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
            .unwrap_or(0);

        if current_level < max_global_level {
            next_level = current_level + 1;
            next_stage_level = 1;
            is_updated = true;
        }
    }

    // STEP 3: Update Database (Hanya jika ada perubahan)
    if is_updated {
        let user_update = user_model::ActiveModel {
            id: Set(auth.id),
            learning_level: Set(next_level),
            learning_stage_level: Set(next_stage_level),
            ..Default::default()
        };

        user_update
            .update(db)
            .await
            .map_err(|e| (Status::InternalServerError, format!("Update failed: {}", e)))?;
    }

    // STEP 4: Return Response
    Ok(Json(CheckResponse {
        success: true,
        message: "".to_string(),
        current_level: next_level,
        current_stage_level: next_stage_level,
    }))
}

#[post("/check/read", data = "<data>")]
pub async fn check_read(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    data: Json<CheckReadRequest>,
) -> Result<Json<CheckResponse>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // VALIDASI: Trim whitespace & Case Insensitive
    if data.guess.trim().to_lowercase() != data.real.trim().to_lowercase() {
        return Err((
            Status::BadRequest,
            "Jawaban salah atau tidak cocok.".to_string(),
        ));
    }

    // STEP 1: Ambil Level & Stage User Saat Ini
    let (current_level, current_stage) = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::LearningLevel)
        .column(user_model::Column::LearningStageLevel)
        .into_tuple::<(i32, i32)>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // STEP 2: Ambil Max Stage untuk Level user saat ini
    let max_stage_in_current_level = learn_model::Entity::find()
        .filter(learn_model::Column::Level.eq(current_level))
        .select_only()
        .column(learn_model::Column::MaxStage)
        .into_tuple::<i32>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
        .ok_or((
            Status::NotFound,
            format!("Konfigurasi untuk level {} tidak ditemukan", current_level),
        ))?;

    // Variabel mutables untuk next state
    let mut next_level = current_level;
    let mut next_stage_level = current_stage;
    let mut is_updated = false;

    if current_stage < max_stage_in_current_level {
        next_stage_level = current_stage + 1;
        is_updated = true;
    } else {
        let max_global_level = learn_model::Entity::find()
            .select_only()
            .column(learn_model::Column::Level)
            .order_by_desc(learn_model::Column::Level)
            .into_tuple::<i32>()
            .one(db)
            .await
            .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
            .unwrap_or(0);

        if current_level < max_global_level {
            next_level = current_level + 1;
            next_stage_level = 1;
            is_updated = true;
        }
    }

    // STEP 3: Update Database (Hanya jika ada perubahan)
    if is_updated {
        let user_update = user_model::ActiveModel {
            id: Set(auth.id),
            learning_level: Set(next_level),
            learning_stage_level: Set(next_stage_level),
            ..Default::default()
        };

        user_update
            .update(db)
            .await
            .map_err(|e| (Status::InternalServerError, format!("Update failed: {}", e)))?;
    }

    // STEP 4: Return Response
    Ok(Json(CheckResponse {
        success: true,
        message: "Jawaban benar!".to_string(),
        current_level: next_level,
        current_stage_level: next_stage_level,
    }))
}

#[post("/check/write", data = "<form_data>")]
pub async fn check_write(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    form_data: Form<CheckWriteRequest<'_>>,
) -> Result<Json<CheckResponse>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // Unpack form data
    let data = form_data.into_inner();
    let detected_text = "hello world";

    // VALIDASI: Trim whitespace & Case Insensitive
    if detected_text.trim().to_lowercase() != data.real_text.trim().to_lowercase() {
        return Err((
            Status::BadRequest,
            "Jawaban salah atau tidak cocok.".to_string(),
        ));
    }

    // STEP 1: Ambil Level & Stage User Saat Ini
    let (current_level, current_stage) = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::LearningLevel)
        .column(user_model::Column::LearningStageLevel)
        .into_tuple::<(i32, i32)>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // STEP 2: Ambil Max Stage untuk Level user saat ini
    let max_stage_in_current_level = learn_model::Entity::find()
        .filter(learn_model::Column::Level.eq(current_level))
        .select_only()
        .column(learn_model::Column::MaxStage)
        .into_tuple::<i32>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
        .ok_or((
            Status::NotFound,
            format!("Konfigurasi untuk level {} tidak ditemukan", current_level),
        ))?;

    // Variabel mutables untuk next state
    let mut next_level = current_level;
    let mut next_stage_level = current_stage;
    let mut is_updated = false;

    if current_stage < max_stage_in_current_level {
        next_stage_level = current_stage + 1;
        is_updated = true;
    } else {
        let max_global_level = learn_model::Entity::find()
            .select_only()
            .column(learn_model::Column::Level)
            .order_by_desc(learn_model::Column::Level)
            .into_tuple::<i32>()
            .one(db)
            .await
            .map_err(|e| (Status::InternalServerError, format!("Db Error: {}", e)))?
            .unwrap_or(0);

        if current_level < max_global_level {
            next_level = current_level + 1;
            next_stage_level = 1;
            is_updated = true;
        }
    }

    // STEP 3: Update Database (Hanya jika ada perubahan)
    if is_updated {
        let user_update = user_model::ActiveModel {
            id: Set(auth.id),
            learning_level: Set(next_level),
            learning_stage_level: Set(next_stage_level),
            ..Default::default()
        };

        user_update
            .update(db)
            .await
            .map_err(|e| (Status::InternalServerError, format!("Update failed: {}", e)))?;
    }

    // STEP 4: Return Response
    Ok(Json(CheckResponse {
        success: true,
        message: "Jawaban benar!".to_string(),
        current_level: next_level,
        current_stage_level: next_stage_level,
    }))
}
