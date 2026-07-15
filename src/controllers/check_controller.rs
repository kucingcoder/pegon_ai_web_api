use crate::middlewares::auth_guard::AuthenticatedUser;
use reqwest::Client;
use base64::Engine;
use crate::models::{learn_model, user_model};
use rocket::form::{Form, FromForm};
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::json::{Json, Value, json};
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
    pub current_level: i32,
    pub current_stage: i32,
}

#[derive(FromForm)]
pub struct CheckWriteRequest<'r> {
    pub image: TempFile<'r>,
    pub real_text: String,
    pub current_level: i32,
    pub current_stage: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CheckUpdateLevelStageRequest {
    pub current_level: i32,
    pub current_stage: i32,
}

#[derive(Debug, Serialize)]
pub struct CheckResponse {
    pub success: bool,
    pub message: String,
    pub current_level: i32,
    pub current_stage_level: i32,
}

#[get("/check/level-stage")]
pub async fn check_level_stage(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // 1. Future User
    let user_future = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::LearningLevel)
        .column(user_model::Column::LearningStageLevel)
        .into_tuple::<(i32, i32)>()
        .one(db);

    // 2. Future Max Level
    let max_level_future = learn_model::Entity::find()
        .select_only()
        .column(learn_model::Column::Level)
        .order_by_desc(learn_model::Column::Level)
        .into_tuple::<i32>()
        .one(db);

    // EKSEKUSI PARALLEL
    let (user_res, max_level_res) = rocket::tokio::join!(user_future, max_level_future);

    let (current_level, current_stage) = user_res
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    let max_level = max_level_res
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "Level not found".to_string()))?;

    // 3. Max Stage (Dependent on user level)
    let max_stage_in_current_level = learn_model::Entity::find()
        .filter(learn_model::Column::Level.eq(current_level))
        .select_only()
        .column(learn_model::Column::MaxStage)
        .into_tuple::<i32>()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "Level not found".to_string()))?;

    Ok(Json(json!({
        "current_level": current_level,
        "current_stage": current_stage,
        "max_stage_in_current_level": max_stage_in_current_level,
        "max_level": max_level
    })))
}

#[post("/check/update-level-stage", data = "<data>")]
pub async fn check_update_level_stage(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    data: Json<CheckUpdateLevelStageRequest>,
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

    // STEP check: Only update if the user's current level/stage matches the request
    if current_level == data.current_level && current_stage == data.current_stage {
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

    // STEP check: Only update if the user's current level/stage matches the request
    if current_level == data.current_level && current_stage == data.current_stage {
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
    client: &State<Client>,
    auth: AuthenticatedUser,
    form_data: Form<CheckWriteRequest<'_>>,
) -> Result<Json<CheckResponse>, (Status, String)> {
    let db = db as &DatabaseConnection;

    // Unpack form data
    let mut data = form_data.into_inner();
    
    let is_bypass = std::env::var("BYPASS_VISION").unwrap_or_else(|_| "false".to_string()) == "true";

    if !is_bypass {
        // Save temp file to process
        let upload_dir = "images/temp";
        let filename = format!("{}.jpg", uuid::Uuid::new_v4());
        let save_path = std::path::Path::new(upload_dir).join(&filename);
        
        // Ensure directory exists
        if !std::path::Path::new(upload_dir).exists() {
            std::fs::create_dir_all(upload_dir).map_err(|_| (Status::InternalServerError, "Failed to create temp dir".to_string()))?;
        }

        data.image.persist_to(&save_path).await
            .map_err(|_| (Status::InternalServerError, "Gagal proses file".to_string()))?;
            
        let detected_text = call_llama_cpp_vision(client, &save_path, "jpg")
            .await
            .map_err(|_| (Status::InternalServerError, "Gagal (Model dalam pemeliharaan)".to_string()))?;

        // Cleanup temp file
        let _ = std::fs::remove_file(&save_path);

        // VALIDASI: Trim whitespace & Case Insensitive (Fuzzy logic could be better but sticking to strict for now)
        // Using simple contains or levenshtein distance would be better for AI output relying on exact match
        // For now, let's normalize both strings
        let normalized_detected = detected_text.trim().to_lowercase().replace(|c: char| !c.is_alphanumeric(), "");
        let normalized_real = data.real_text.trim().to_lowercase().replace(|c: char| !c.is_alphanumeric(), "");

        if normalized_detected != normalized_real {
            // Fallback: If AI includes extra polite text, try to see if real text is *contained* in detected
            if !detected_text.to_lowercase().contains(&data.real_text.to_lowercase()) {
                 return Err((
                    Status::BadRequest,
                    format!("Jawaban salah. Terdeteksi: '{}'", detected_text),
                ));
            }
        }
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

    // STEP check: Only update if the user's current level/stage matches the request
    if current_level == data.current_level && current_stage == data.current_stage {
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

async fn call_llama_cpp_vision(client: &Client, image_path: &std::path::Path, ext: &str) -> Result<String, String> {
    let model_url = std::env::var("MODEL_URL").map_err(|_| "MODEL_URL not set".to_string())?;
    let api_key = std::env::var("MODEL_API_KEY").unwrap_or_default();

    let image_data = std::fs::read(image_path).map_err(|e| format!("Failed to read image: {}", e))?;
    let base64_image = base64::engine::general_purpose::STANDARD.encode(&image_data);
    let data_url = format!("data:image/{};base64,{}", ext, base64_image);

    let request_body = json!({
        "messages": [
            {
                "role": "system",
                "content": "You are an expert Optical Character Recognition (OCR) engine specialized in reading traditional Indonesian and Javanese Pegon script from images.\n\nYour task is to extract and process the text found in the provided image strictly following these rules:\n1. Mixed Script Handling (CRUCIAL):\n   - If the text is standard Arabic (e.g., Quranic verses, Hadith, or Arabic terminology), TRANSCRIBE it exactly as original Arabic text.\n   - ONLY TRANSLITERATE the Pegon (Indonesian/Javanese) text into Latin script.\n2. Output ONLY the final text (which may be a mix of Latin script and Arabic script). Do not include any explanations, tags, or introductory text.\n3. For the Pegon-to-Latin transliteration, carefully analyze the visual modifiers of Pegon letters:\n   - 3 dots below (چ) transliterates to 'c'\n   - 3 dots above (ڤ) transliterates to 'p'\n   - 3 dots above (ڠ) transliterates to 'ng'\n   - 3 dots below (کٜ / گ) transliterates to 'g'\n   - 3 dots above/below (پ) transliterates to 'ny'\n4. Accurately interpret the harakat (vowels) and specific Pegon maddah/sukun combinations to construct the correct Indonesian/Javanese words.\n5. Strictly treat the transliterated parts as Indonesian/Javanese Pegon, NOT standard Malay Jawi."
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": data_url
                        }
                    }
                ]
            }
        ]
    });

    let url = format!("{}/v1/chat/completions", model_url);

    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Model API Error {}: {}", status, text));
    }

    let json: Value = resp.json().await.map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(content)
}
