use rocket::form::{Form, FromForm};
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, post};
use sea_orm::*;
use std::env;
use std::path::Path;
use uuid::Uuid;

use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::Category;
use crate::models::{image_transliterations, prelude::*};

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
    auth: AuthenticatedUser,
    mut form: Form<ImageTransliterationRequest<'_>>,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let user_model = Users::find_by_id(auth.id)
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "User not found".to_string()))?;

    // jika user standard hanya boleh transliterate 3 kali dalam sehari
    if user_model.category == Category::Standard {
        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let count = ImageTransliterations::find()
            .filter(image_transliterations::Column::IdUser.eq(auth.id))
            .filter(image_transliterations::Column::CreatedAt.gt(today_start))
            .count(db)
            .await
            .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

        if count >= 3 {
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

    let result = "lorem ipsum".to_string();
    let url = make_full_url(&format!("transliterations/{}", filename));
    let title = chrono::Utc::now().format("%d-%m-%Y %H:%M").to_string();

    image_transliterations::ActiveModel {
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
        "result": result
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

    // Default page = 1, default limit = 10
    let page = page.unwrap_or(1);
    let limit = limit.unwrap_or(10);

    // Setup Query
    let paginator = ImageTransliterations::find()
        .filter(image_transliterations::Column::IdUser.eq(auth.id))
        .order_by_desc(image_transliterations::Column::CreatedAt)
        .paginate(db, limit);

    // Ambil data halaman spesifik
    // Gunakan saturating_sub agar tidak panic jika user kirim page 0
    let pagination_result = paginator.fetch_page(page.saturating_sub(1)).await;

    match pagination_result {
        Ok(items) => {
            let history_data: Vec<Value> = items
                .into_iter()
                .map(|item| {
                    let result_truncated: String = item.result.chars().take(100).collect();
                    let final_result = if item.result.chars().count() > 100 {
                        format!("{}...", result_truncated)
                    } else {
                        result_truncated
                    };

                    json!({
                        "title": item.title,
                        "image": item.image,
                        "created_at": item.created_at,
                        "result": final_result
                    })
                })
                .collect();

            let total_pages = paginator.num_pages().await.unwrap_or(0);

            Ok(Json(json!({
                "data": history_data,
                "meta": {
                    "current_page": page,
                    "per_page": limit,
                    "total_pages": total_pages
                }
            })))
        }
        Err(_) => Err((
            Status::InternalServerError,
            "Gagal mengambil data riwayat".to_string(),
        )),
    }
}
