use crate::middlewares::auth_guard::AuthenticatedUser;
use rocket::http::Status;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, post};
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;

#[derive(Debug, Deserialize, Serialize)]
pub struct TranslationRequest {
    pub text: String,
}

#[post("/translate", data = "<data>")]
pub async fn translate(
    client: &State<Client>,
    _auth: AuthenticatedUser,
    data: Json<TranslationRequest>,
) -> Result<Json<Value>, (Status, String)> {
    let api_key = env::var("GEMINI_API_KEY").map_err(|_| (Status::InternalServerError, "GEMINI_API_KEY not set".to_string()))?;
    
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite:generateContent?key={}", api_key);
    
    let request_body = json!({
        "systemInstruction": {
            "parts": [{"text": "Anda adalah sistem API penerjemah otomatis. Terjemahkan teks yang diberikan pengguna ke dalam bahasa Indonesia secara akurat dan natural. ATURAN SANGAT KETAT: Anda HANYA boleh membalas dengan teks hasil terjemahannya saja. DILARANG KERAS memberikan teks pengantar/penjelasan (seperti 'Berikut adalah terjemahannya'). DILARANG menggunakan format markdown apa pun (jangan gunakan bold, italic, maupun blok kode). Hasil akhir harus murni plain text."}]
        },
        "contents": [{
            "parts": [{"text": data.text}]
        }]
    });
    
    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| (Status::InternalServerError, format!("Gagal memanggil API: {}", e)))?;
        
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err((Status::BadGateway, format!("Gemini API Error: {}", text)));
    }
    
    let json: Value = resp.json().await.map_err(|e| (Status::InternalServerError, format!("Failed to parse JSON: {}", e)))?;
    
    let translated_text = json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();
        
    Ok(Json(json!({
        "result": translated_text.trim()
    })))
}
