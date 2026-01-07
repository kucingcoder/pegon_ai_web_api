use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::Category;
use crate::models::{text_transliteration_model, user_model};
use rocket::http::Status;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, post};
use sea_orm::QuerySelect;
use sea_orm::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TextTransliterationRequest {
    pub text: String,
    pub harakat: bool,
}

#[post("/transliteration/text", data = "<data>")]
pub async fn transliterate(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    data: Json<TextTransliterationRequest>,
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

    // jika user standard hanya boleh transliterate 10 kali dalam sehari
    if user_category == Category::Standard {
        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let count = text_transliteration_model::Entity::find()
            .filter(text_transliteration_model::Column::IdUser.eq(auth.id))
            .filter(text_transliteration_model::Column::CreatedAt.gt(today_start))
            .count(db)
            .await
            .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

        if count >= 10 {
            return Err((
                Status::Forbidden,
                "Transliteration limit reached".to_string(),
            ));
        }
    }

    // transliterate text
    let instrution = if data.harakat {
        "dengan harakat".to_string()
    } else {
        "tanpa harakat".to_string()
    };
    let generated_result = "hello world".to_string();

    // simpan hasil transliteration
    text_transliteration_model::ActiveModel {
        id_user: Set(auth.id),
        instruction: Set(instrution.clone()),
        input: Set(data.text.clone()),
        result: Set(generated_result.clone()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?;

    Ok(Json(json!({
        "result": generated_result
    })))
}
