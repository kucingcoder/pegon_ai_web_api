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
    std::fs::create_dir_all("images/photo_profiles").expect("Gagal buat folder photo_profiles");
    std::fs::create_dir_all("images/transliterations").expect("Gagal buat folder transliterations");

    // build rocket
    rocket::build()
        .manage(db)
        // landing page
        .mount("/", routes![controllers::home_controller::index])
        // file servers
        .mount("/static", FileServer::from(relative!("static")))
        .mount("/images", FileServer::from(relative!("images")))
        // api routes
        .mount(
            "/api",
            routes![
                controllers::auth_controller::login,
                controllers::auth_controller::logout,
                controllers::user_controller::get_profile,
                controllers::user_controller::get_profile_detail,
                controllers::user_controller::update_profile,
                controllers::text_transliterations::transliterate,
                controllers::image_transliterations::transliterate,
                controllers::image_transliterations::history,
                controllers::image_transliterations::read,
                controllers::image_transliterations::update_title
            ],
        )
}
