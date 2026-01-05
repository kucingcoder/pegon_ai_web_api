use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::Category;
use crate::models::{prelude::*, text_transliterations};
use rocket::http::Status;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, post};
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

        let count = TextTransliterations::find()
            .filter(text_transliterations::Column::IdUser.eq(auth.id))
            .filter(text_transliterations::Column::CreatedAt.gt(today_start))
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

    // transliterate text
    let instrution = if data.harakat {
        "dengan harakat".to_string()
    } else {
        "tanpa harakat".to_string()
    };
    let generated_result = "hello world".to_string();

    // simpan hasil transliteration
    text_transliterations::ActiveModel {
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
