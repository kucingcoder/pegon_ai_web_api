use rocket::form::{Form, FromForm};
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, patch, post};
use sea_orm::sea_query::{Expr, Func};
use sea_orm::*;
use sea_orm::{EntityTrait, QuerySelect};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;

use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::Category;
use crate::models::{image_transliteration_model, user_model};

fn make_full_url(path: &str) -> String {
    let app_url = env::var("APP_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    format!("{}/images/{}", app_url, path)
}

#[derive(FromForm)]
pub struct ImageTransliterationRequest<'r> {
    pub image: TempFile<'r>,
}

#[post("/transliteration/image", data = "<form>")]
pub async fn transliterate(
    db: &State<DatabaseConnection>,
    client: &State<Client>,
    auth: AuthenticatedUser,
    mut form: Form<ImageTransliterationRequest<'_>>,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let user_category: Category = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::Category)
        .into_tuple()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // jika user standard hanya boleh transliterate 4 kali dalam sehari
    if user_category == Category::Standard {
        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let count = image_transliteration_model::Entity::find()
            .filter(image_transliteration_model::Column::IdUser.eq(auth.id))
            .filter(image_transliteration_model::Column::CreatedAt.gt(today_start))
            .count(db)
            .await
            .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

        if count >= 4 {
            return Err((
                Status::Forbidden,
                "Transliteration limit reached".to_string(),
            ));
        }
    }

    let ref mut file = form.image;
    let file_len = file.len();
    if file_len > 20 * 1024 * 1024 {
        return Err((Status::PayloadTooLarge, "Maksimal 20MB".to_string()));
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

    let upload_dir = "images/transliterations";
    let filename = format!("{}.{}", Uuid::new_v4(), ext);
    let save_path = Path::new(upload_dir).join(&filename);

    file.persist_to(&save_path)
        .await
        .map_err(|_| (Status::InternalServerError, "Gagal simpan file".to_string()))?;

    let url = make_full_url(&format!("transliterations/{}", filename));
    
    let result = call_llama_cpp_vision(client, &save_path, ext)
        .await
        .map_err(|e| (Status::InternalServerError, format!("AI Error: {}", e)))?;
    let title = chrono::Utc::now().format("%d-%m-%Y %H:%M").to_string();

    let new_image_transliteration = image_transliteration_model::ActiveModel {
        id_user: Set(auth.id),
        image: Set(url),
        title: Set(title),
        result: Set(result.clone()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    Ok(Json(json!({
        "history": new_image_transliteration.id,
    })))
}

#[get("/transliteration/image/history?<page>&<limit>")]
pub async fn history(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    page: Option<u64>,
    limit: Option<u64>,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let page = page.unwrap_or(1);
    let limit = limit.unwrap_or(10);

    let paginator = image_transliteration_model::Entity::find()
        .filter(image_transliteration_model::Column::IdUser.eq(auth.id))
        .order_by_desc(image_transliteration_model::Column::CreatedAt)
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
        .paginate(db, limit);

    let total_pages = paginator.num_pages().await.map_err(|_| {
        (
            Status::InternalServerError,
            "Gagal menghitung halaman".to_string(),
        )
    })?;

    let pagination_result = paginator.fetch_page(page.saturating_sub(1)).await;

    match pagination_result {
        Ok(items) => Ok(Json(json!({
            "data": items,
            "meta": {
                "current_page": page,
                "per_page": limit,
                "total_pages": total_pages
            }
        }))),
        Err(_) => Err((
            Status::InternalServerError,
            "Gagal mengambil data riwayat".to_string(),
        )),
    }
}

#[get("/transliteration/image/history/read?<id>")]
pub async fn read(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    id: String,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let parsed_id = Uuid::parse_str(&id)
        .map_err(|_| (Status::BadRequest, "Format ID tidak valid".to_string()))?;

    let image_transliteration = image_transliteration_model::Entity::find()
        .filter(image_transliteration_model::Column::IdUser.eq(auth.id))
        .filter(image_transliteration_model::Column::Id.eq(parsed_id))
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "Data not found".to_string()))?;

    Ok(Json(json!({
        "title": image_transliteration.title,
        "image": image_transliteration.image,
        "result": image_transliteration.result,
        "created_at": image_transliteration.created_at
    })))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateTitleRequest {
    pub id: String,
    pub title: String,
}

#[patch("/transliteration/image/history/update-title", data = "<data>")]
pub async fn update_title(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    data: Json<UpdateTitleRequest>,
) -> Result<Status, (Status, String)> {
    let db = db as &DatabaseConnection;

    let parsed_id = Uuid::parse_str(&data.id)
        .map_err(|_| (Status::BadRequest, "Format ID tidak valid".to_string()))?;

    let image_transliteration = image_transliteration_model::Entity::find()
        .filter(image_transliteration_model::Column::IdUser.eq(auth.id))
        .filter(image_transliteration_model::Column::Id.eq(parsed_id))
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "Data not found".to_string()))?;

    let mut image_transliteration: image_transliteration_model::ActiveModel =
        image_transliteration.into();
    image_transliteration.title = Set(data.title.clone());

    image_transliteration
        .update(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    Ok(Status::Ok)
}

async fn call_llama_cpp_vision(client: &Client, image_path: &Path, ext: &str) -> Result<String, String> {
    let model_url = env::var("MODEL_URL").map_err(|_| "MODEL_URL not set".to_string())?;
    let api_key = env::var("MODEL_API_KEY").unwrap_or_default();

    let image_data = std::fs::read(image_path).map_err(|e| format!("Failed to read image: {}", e))?;
    let base64_image = general_purpose::STANDARD.encode(&image_data);
    let data_url = format!("data:image/{};base64,{}", ext, base64_image);

    let request_body = json!({
        "messages": [
            {
                "role": "system",
                "content": "Pegon text is an Arabic-like script used to write manuscripts in Javanese, Indonesian, Malay, Sundanese, and Madurese. Perform character recognition on the following Pegon text and render the results in Latin for easy reading."
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
