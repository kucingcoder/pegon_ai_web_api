use crate::middlewares::auth_guard::AuthenticatedUser;
use rocket::http::{ContentType, Status};
use rocket::serde::json::{serde_json::json, Json};
use rocket::{post, State};
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;

#[derive(Debug, Deserialize, Serialize)]
pub struct KajianRequest {
    pub text: String,
}

#[post("/kajian", data = "<data>")]
pub async fn generate_kajian(
    client: &State<Client>,
    _auth: AuthenticatedUser,
    data: Json<KajianRequest>,
) -> Result<(ContentType, Vec<u8>), (Status, String)> {
    // 1. Panggil Gemini API untuk generate skrip kajian
    let gemini_api_key = env::var("GEMINI_API_KEY")
        .map_err(|_| (Status::InternalServerError, "GEMINI_API_KEY not set".to_string()))?;
        
    let gemini_url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite:generateContent?key={}", gemini_api_key);
    
    let gemini_request_body = json!({
        "systemInstruction": {
            "parts": [{"text": "Anda adalah seorang ustadz gaul dan santai yang sedang memberikan kajian atau ceramah singkat. Anda akan menerima teks (mungkin hasil transliterasi bahasa daerah seperti Jawa/Sunda). Tugas Anda: sebutkan beberapa kalimat yang menarik dari teks tersebut, lalu berikan pembahasan atau nasihat keagamaan dengan gaya santai, hangat, dan mudah dipahami, layaknya podcast atau ceramah singkat. JANGAN menggunakan format markdown seperti bold, italic, atau bullet point. Tuliskan teks secara natural seperti orang berbicara. Panjang ceramah maksimal 2-3 paragraf saja."}]
        },
        "contents": [{
            "parts": [{"text": data.text.clone()}]
        }]
    });
    
    let gemini_resp = client.post(&gemini_url)
        .header("Content-Type", "application/json")
        .json(&gemini_request_body)
        .send()
        .await
        .map_err(|e| (Status::InternalServerError, format!("Gagal memanggil Gemini API: {}", e)))?;
        
    if !gemini_resp.status().is_success() {
        let text = gemini_resp.text().await.unwrap_or_default();
        return Err((Status::BadGateway, format!("Gemini API Error: {}", text)));
    }
    
    let gemini_json: rocket::serde::json::serde_json::Value = gemini_resp.json().await.map_err(|e| (Status::InternalServerError, format!("Failed to parse Gemini JSON: {}", e)))?;
    
    let kajian_text = gemini_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if kajian_text.is_empty() {
        return Err((Status::InternalServerError, "Teks kajian gagal di-generate".to_string()));
    }

    // 2. Panggil ElevenLabs API untuk merubah teks menjadi audio
    let elevenlabs_api_key = env::var("ELEVENLABS_API_KEY")
        .map_err(|_| (Status::InternalServerError, "ELEVENLABS_API_KEY not set".to_string()))?;
    
    let elevenlabs_voice_id = env::var("ELEVENLABS_VOICE_ID")
        .unwrap_or_else(|_| "2EiwWnXFnvU5JabPnv8n".to_string()); // Default ke Clyde (Free standard voice)
        
    let elevenlabs_url = format!("https://api.elevenlabs.io/v1/text-to-speech/{}/stream", elevenlabs_voice_id);
    
    let elevenlabs_request_body = json!({
        "text": kajian_text.trim(),
        "model_id": "eleven_multilingual_v2",
        "voice_settings": {
            "stability": 0.5,
            "similarity_boost": 0.75
        }
    });

    let tts_resp = client.post(&elevenlabs_url)
        .header("xi-api-key", elevenlabs_api_key)
        .header("Content-Type", "application/json")
        .header("Accept", "audio/mpeg")
        .json(&elevenlabs_request_body)
        .send()
        .await
        .map_err(|e| (Status::InternalServerError, format!("Gagal memanggil ElevenLabs API: {}", e)))?;

    if !tts_resp.status().is_success() {
        let text = tts_resp.text().await.unwrap_or_default();
        return Err((Status::BadGateway, format!("ElevenLabs API Error: {}", text)));
    }

    let audio_bytes = tts_resp.bytes().await
        .map_err(|e| (Status::InternalServerError, format!("Gagal membaca stream audio: {}", e)))?
        .to_vec();

    Ok((ContentType::new("audio", "mpeg"), audio_bytes))
}
