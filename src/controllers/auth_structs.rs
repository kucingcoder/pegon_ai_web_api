use serde::{Deserialize, Serialize};

// 1. Request dari Flutter (Login)
// Ini JSON yang dikirim dari HP: { "id_token": "eyJhbG..." }
#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleLoginRequest {
    pub id_token: String,
}

// 2. Response dari Google API
// Saat backend kita tanya ke Google: "Token ini punya siapa?", Google balas JSON ini.
// Kita cuma butuh ambil field penting saja.
#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleTokenPayload {
    pub iss: String,             // Issuer (siapa yang ngeluarin token)
    pub sub: String,             // Subject (ID unik user di Google)
    pub aud: String,             // Audience (Harus cocok dengan Client ID kita)
    pub email: String,           // Email user
    pub email_verified: String,  // "true"/"false" (Google kadang kasih string)
    pub name: String,            // Nama Lengkap
    pub picture: Option<String>, // Foto Profil (Bisa ada bisa nggak)
    pub exp: u64,                // Waktu expired
}
