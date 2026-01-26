use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::sea_orm_active_enums::Category;
use crate::models::{text_transliteration_model, user_model};
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, post, get};
use rocket_dyn_templates::{Template, context};
use sea_orm::QuerySelect;
use sea_orm::*;
use serde::{Deserialize, Serialize};

use std::env;
use reqwest::Client;

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
    let generated_result = call_llama_cpp(&data.text, &instrution)
        .await
        .map_err(|e| (Status::InternalServerError, format!("AI Error: {}", e)))?;

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

#[get("/transliteration/add-in-transliterate-view")]
pub async fn add_in_transliterate_view(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
) -> Result<Template, Redirect> {
    let db = db as &DatabaseConnection;

    let user_category: Category = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::Category)
        .into_tuple()
        .one(db)
        .await
        .map_err(|_| Redirect::to("/add-in/login/add-in-auth-view"))?
        .ok_or(Redirect::to("/add-in/login/add-in-auth-view"))?;

    if user_category != Category::Premium {
        return Err(Redirect::to("/add-in/login/add-in-auth-view"));
    }

    Ok(Template::render("addin-transliterate", context! {}))
}

#[post("/transliteration/add-in-transliterate-handle", data = "<data>")]
pub async fn add_in_transliterate_handle(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    data: Json<TextTransliterationRequest>,
) -> Result<Json<Value>, Redirect> {
    let db = db as &DatabaseConnection;

    let user_category: Category = user_model::Entity::find_by_id(auth.id)
        .select_only()
        .column(user_model::Column::Category)
        .into_tuple()
        .one(db)
        .await
        .map_err(|_| Redirect::to("/add-in/login/add-in-auth-view"))?
        .ok_or(Redirect::to("/add-in/login/add-in-auth-view"))?;

    if user_category != Category::Premium {
        return Err(Redirect::to("/add-in/login/add-in-auth-view"));
    }

    // transliterate text
    let instrution = if data.harakat {
        "dengan harakat".to_string()
    } else {
        "tanpa harakat".to_string()
    };
    
    let generated_result = match call_llama_cpp(&data.text, &instrution).await {
        Ok(res) => res,
        Err(e) => {
            return Ok(Json(json!({
                "status": "error",
                "message": format!("Gagal transliterasi: {}", e)
            })));
        }
    };

    // simpan hasil transliteration
    let save_result = text_transliteration_model::ActiveModel {
        id_user: Set(auth.id),
        instruction: Set(instrution.clone()),
        input: Set(data.text.clone()),
        result: Set(generated_result.clone()),
        ..Default::default()
    }
    .insert(db)
    .await;

    if let Err(_e) = save_result {
        return Ok(Json(json!({
            "status": "error",
            "message": "Gagal menyimpan data transliterasi"
        })));
    }

    Ok(Json(json!({
        "result": generated_result
    })))
}

async fn call_llama_cpp(text: &str, instruction: &str) -> Result<String, String> {
    let model_url = env::var("MODEL_URL").map_err(|_| "MODEL_URL not set".to_string())?;
    let api_key = env::var("MODEL_API_KEY").unwrap_or_default();

    let client = Client::new();
    
    // Append instruction to ensure the model follows context (harakat preference)
    // Using the user provided template structure
    let request_body = json!({
        "messages": [
            {
                "role": "system",
                "content": "You are a script converter that transforms Latin text into Arabic Pegon text."
            },
            {
                "role": "user",
                "content": text
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
