#[macro_use]
extern crate rocket;
use rocket::fs::{FileServer, relative};
use sea_orm::Database;
mod controllers;
mod middlewares;
mod models;

#[launch]
async fn rocket() -> _ {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL wajib ada di .env");
    let db = Database::connect(&db_url)
        .await
        .expect("Gagal konek ke Database");
    println!("Sukses konek ke database: {}", db_url);

    rocket::build()
        .manage(db)
        .mount("/images", FileServer::from(relative!("images")))
        .mount(
            "/api",
            routes![
                controllers::user_controller::get_profile,
                controllers::user_controller::update_profile,
            ],
        )
}
