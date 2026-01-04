#[macro_use]
extern crate rocket;
use rocket::fs::{FileServer, relative};
use sea_orm::Database;
mod controllers;
mod middlewares;
mod models;

#[launch]
async fn rocket() -> _ {
    // load .env
    dotenvy::dotenv().ok();

    // konek ke database
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL wajib ada di .env");
    let db = Database::connect(&db_url)
        .await
        .expect("Gagal konek ke Database");

    // buat folder images kalo belum ada
    std::fs::create_dir_all("images").expect("Gagal buat folder images");

    // build rocket
    rocket::build()
        .manage(db)
        .mount("/images", FileServer::from(relative!("images")))
        .mount(
            "/api",
            routes![
                controllers::auth_controller::google_login,
                controllers::auth_controller::logout,
                controllers::user_controller::get_profile,
                controllers::user_controller::update_profile,
            ],
        )
}
