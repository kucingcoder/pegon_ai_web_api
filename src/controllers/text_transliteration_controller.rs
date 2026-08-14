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
    client: &State<Client>,
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
    let generated_result = call_llama_cpp(client, &data.text, data.harakat)
        .await
        .map_err(|_| (Status::InternalServerError, "Gagal (Model dalam pemeliharaan)".to_string()))?;

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
    client: &State<Client>,
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
    
    let generated_result = match call_llama_cpp(client, &data.text, data.harakat).await {
        Ok(res) => res,
        Err(_) => {
            return Ok(Json(json!({
                "status": "error",
                "message": "Gagal (Model dalam pemeliharaan)"
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

async fn call_llama_cpp(client: &Client, text: &str, harakat: bool) -> Result<String, String> {
    let model_url = env::var("MODEL_URL").map_err(|_| "MODEL_URL not set".to_string())?;
    let api_key = env::var("MODEL_API_KEY").unwrap_or_default();

    let system_prompt = if harakat {
        "You are an expert transliteration engine specializing in converting Indonesian and Javanese Latin text into Pegon script (a modified Arabic script traditionally used in the Nusantara region).\n\nYour task is to convert the provided Latin text into Pegon script strictly following these transliteration rules:\n1. Output ONLY the Pegon text. Do not include any introductory text, explanations, or extra characters.\n2. Base Letters: Use standard Arabic letters for common sounds, but strictly use these specific Pegon modifiers for local consonants:\n   - 'C' or 'c' = چ\n   - 'P' or 'p' = ڤ\n   - 'NG' or 'ng' = ڠ\n   - 'G' or 'g' = کٜ\n   - 'NY' or 'ny' = پ\n3. Full Harakat (Vowels): Every consonant must have the appropriate harakat (fathah, kasrah, dammah, sukun).\n4. Specific Pegon Vowel Rules:\n   - 'E' (taling / clear 'e') = fathah followed by ya sukun.\n   - 'E' or 'E'' (pepet / muted 'e') = Arabic maddah above the consonant.\n   - 'O' or 'o' = fathah followed by waw sukun.\n5. Numbers: Convert all Latin numerals into Arabic numerals (٠, ١, ٢, ٣, ٤, ٥, ٦, ٧, ٨, ٩).\n\nStrictly adhere to these rules to maintain the integrity of traditional Indonesian and Javanese Pegon orthography. Do not use standard Malay Jawi variants."
    } else {
        "You are an expert transliteration engine specializing in converting Indonesian and Javanese Latin text into Pegon script (a modified Arabic script traditionally used in the Nusantara region).\n\nYour task is to convert the provided Latin text into bare Pegon script (Pegon Gundul / without harakat) strictly following these transliteration rules:\n1. Output ONLY the Pegon text. Do not include any introductory text, explanations, or extra characters.\n2. Base Letters: Use standard Arabic letters for common sounds, but strictly use these specific Pegon modifiers for local consonants:\n   - 'C' or 'c' = چ\n   - 'P' or 'p' = ڤ\n   - 'NG' or 'ng' = ڠ\n   - 'G' or 'g' = کٜ\n   - 'NY' or 'ny' = پ\n3. NO HARAKAT (Gundul): Absolutely do NOT use any vowel diacritics (fathah, kasrah, dammah, sukun, maddah, or tanwin). The output must be completely bare consonants and vowel letters (alif, waw, ya) according to standard Pegon spelling.\n5. Numbers: Convert all Latin numerals into Arabic numerals (٠, ١, ٢, ٣, ٤, ٥, ٦, ٧, ٨, ٩).\n\nStrictly adhere to these rules to maintain the integrity of traditional Indonesian and Javanese Pegon orthography. Do not use standard Malay Jawi variants."
    };
    
    // Append instruction to ensure the model follows context (harakat preference)
    let request_body = json!({
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": text.to_lowercase()
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
