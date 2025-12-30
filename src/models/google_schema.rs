use serde::Deserialize;

// Payload yang dikirim dari Flutter
#[derive(Deserialize)]
pub struct GoogleLoginRequest {
    pub id_token: String,
}

// Respon dari Google saat kita validasi token
#[derive(Deserialize, Debug)]
pub struct GoogleTokenPayload {
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub aud: String, // Penting! Client ID
                     // sub, iss, exp, dll (bisa ditambah jika butuh)
}
